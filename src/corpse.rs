//! Ragdoll corpses (feature 17): a cleanly-killed monster's body topples with real
//! physics. Its SKELETON — the same parented bone hierarchy that was just fighting
//! you — is handed to a small Verlet/PBD solver: one particle per bone joint, the
//! bone lengths as distance constraints, light bend-stiffness so the trunk stays
//! semi-rigid while the limbs flop, and per-particle world collision so the body
//! drapes over floors, ledges and lava lips instead of clipping through them.
//!
//! ## How it hooks into the rest of the engine
//!
//! The bones are NOT reparented — we keep the hierarchy and each frame overwrite
//! every bone's LOCAL `Transform` so the rendered, parented meshes follow the
//! solved pose (`reconstruct`, the hard part — see there). The animator
//! (`animate_monsters`) yields the bones the moment a `Ragdoll` exists, so the two
//! never fight.
//!
//! To OTHER actors (the player, monsters, hitscan, line-of-sight) the flopped body
//! is a single body-sized tracking [`Aabb`] rewritten every frame from the particle
//! cloud into a reserved slot in [`WorldColliders`] — the truck's moving-brush trick
//! ([`crate::vehicle`]). Where the truck reserves ONE slot for the level's life,
//! corpses come and go, so this carves a small **pool** of slots at build time
//! (`CORPSE_CAP`) and hands them out via a runtime free-list (claim on a clean kill,
//! release on freeze/despawn). A disabled slot holds the far-away [`degenerate`]
//! sentinel a closed door / parked truck uses. Past the cap a kill still ragdolls —
//! the visual drape needs no slot; it just isn't solid to others until one frees.
//!
//! Knockback feeds in for free: the body keeps its [`Knockback`] component, so a
//! rocket splash (`combat.rs`), a Whip fling (`weapons.rs`) and a truck ram
//! (`vehicle.rs`) all accumulate into it through the normal `DamageEvent` path — we
//! consume it as an upper-body-biased impulse, so a blow flings AND tumbles the body.
//! The `Dying` sink/despawn timeline stays in `monster_model::animate_death`; at
//! `CORPSE_SINK_BEGINS` the ragdoll stops simulating and holds its final pose while
//! the (un-rotated) root sinks it straight into the floor.

use std::f32::consts::PI;

use bevy::prelude::*;

use crate::common::{tune::*, *};
use crate::enemies::Enemy;
use crate::monster_model::{AnimRole, Bone, Dying, Severed};
use crate::physics::{depenetrate, Aabb};

/// Sentinel slot index meaning "this ragdoll got no collision slot (pool full)" —
/// it still simulates and drapes on the world, it's just not solid to other actors.
const NO_SLOT: usize = usize::MAX;

/// A degenerate AABB parked a million metres away — the same inert sentinel a
/// closed door / parked truck slot uses. Nothing can reach it, so the slot is "off"
/// without changing the collider Vec's length (its index stays stable).
pub(crate) fn degenerate() -> Aabb {
    Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

/// `Dying.t` at which the ragdoll freezes and `animate_death` takes over the sink.
const SINK_BEGINS: f32 = CORPSE_SINK_BEGINS;

// ----------------------------------------------------------------------------
// Slot pool (reused verbatim from the old box-corpse: still how a body goes solid).
// ----------------------------------------------------------------------------

/// The runtime free-list over the corpse collider pool. The pool is `CORPSE_CAP`
/// degenerate slots reserved at level build time (so every index is stable for the
/// level's life); `free` lists which are unused, and `active` maps each slotted
/// corpse entity to the slot it holds so we can reclaim it the frame it freezes or
/// vanishes (whatever despawned it).
#[derive(Resource, Default)]
pub struct CorpseSlots {
    /// Slot indices currently free to claim.
    free: Vec<usize>,
    /// (corpse entity, slot) for every corpse holding a collision slot.
    active: Vec<(Entity, usize)>,
}
impl CorpseSlots {
    /// Carve `CORPSE_CAP` fresh degenerate slots onto the end of `solids` and reset
    /// the free-list to own exactly them. Call once per level build, AFTER all the
    /// build-time door/truck/strand reservations, so the corpse indices come last.
    pub fn reserve_pool(&mut self, solids: &mut Vec<Aabb>) {
        self.active.clear();
        self.free.clear();
        for _ in 0..CORPSE_CAP {
            self.free.push(solids.len());
            solids.push(degenerate());
        }
    }

    fn claim(&mut self, e: Entity) -> Option<usize> {
        let slot = self.free.pop()?;
        self.active.push((e, slot));
        Some(slot)
    }
}

/// Carve the corpse collider pool onto the level's freshly-built solids list. Runs
/// in the `OnEnter(Playing)` chain right after `setup_level`, so every build-time
/// door/truck/strand slot is already in place and the corpse indices come last.
pub fn reserve_corpse_pool(mut slots: ResMut<CorpseSlots>, mut colliders: ResMut<WorldColliders>) {
    slots.reserve_pool(&mut colliders.solids);
}

// ----------------------------------------------------------------------------
// The ragdoll.
// ----------------------------------------------------------------------------

/// One simulated joint: a Verlet particle (current + previous position, so velocity
/// is implicit in `pos - prev`) plus whether it's an upper-body bone (drives the
/// topple bias and the knockback impulse weighting).
struct Particle {
    pos: Vec3,
    prev: Vec3,
    is_upper: bool,
}

/// A distance / bend constraint held at `rest` length with stiffness `stiff` ∈ (0,1].
struct Stick {
    a: usize,
    b: usize,
    rest: f32,
    stiff: f32,
}

/// The Verlet ragdoll for one dead monster, on its (root) `Enemy` entity. Particle
/// index `i` corresponds 1:1 with bone entity `bones[i]`. The `rest_*` and topology
/// arrays are captured once at the death frame; `pos/prev` are integrated each frame.
#[derive(Component)]
pub struct Ragdoll {
    particles: Vec<Particle>,
    sticks: Vec<Stick>,
    /// `bones[i]` is the joint entity particle `i` drives.
    bones: Vec<Entity>,
    /// Parent particle index, or -1 for the root bone (whose parent is the Enemy entity).
    parent_idx: Vec<i32>,
    /// Primary child particle index used to orient this bone, or -1 for a leaf.
    primary_child: Vec<i32>,
    /// Parents-before-children traversal order for the per-frame reconstruction.
    order: Vec<usize>,
    /// Each bone's world rotation at the death frame.
    rest_world_rot: Vec<Quat>,
    /// World direction joint→primary-child at the death frame (unit), ZERO for a leaf.
    rest_dir_world: Vec<Vec3>,
    /// Collision slot in `WorldColliders.solids`, or [`NO_SLOT`] if the pool was full.
    slot: usize,
    /// Tracking AABB recomputed each frame (also read by `gamestate::corpse_hazard`).
    pub body_box: Aabb,
    /// Cooldown gating the settle/scrape cue so a long skid isn't a stutter of thuds.
    scrape_cd: f32,
}
impl Ragdoll {
    /// The body's collision slot, or `None` if it never got one (pool was full).
    pub fn slot(&self) -> Option<usize> {
        (self.slot != NO_SLOT).then_some(self.slot)
    }
}

pub struct CorpsePlugin;
impl Plugin for CorpsePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CorpseSlots>().add_systems(
            Update,
            (reclaim_ragdoll_slots, build_ragdolls, ragdoll_solve)
                .chain()
                // Before the player resolves its own collision, so the body has
                // already moved this frame and the player slides off / is pushed
                // out of it cleanly (never the other way around).
                .before(crate::player::player_move)
                // Before the sink animator, so on the freeze-handoff frame the slot
                // is released ahead of `animate_death`'s sink.
                .before(crate::monster_model::animate_death)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// Reclaim collision slots: free a slotted corpse's slot once it has either begun
/// its sink (`Dying.t >= SINK_BEGINS` — the ragdoll freezes and `animate_death` owns
/// the descent) or despawned (timer-out / dissolved in lava). The `Ragdoll`
/// component itself lingers until the entity despawns so `animate_monsters` keeps
/// yielding the bones — only the *solid* slot is recycled here.
fn reclaim_ragdoll_slots(
    mut slots: ResMut<CorpseSlots>,
    mut colliders: ResMut<WorldColliders>,
    q: Query<&Dying, With<Ragdoll>>,
) {
    let CorpseSlots { active, free } = &mut *slots;
    active.retain(|&(e, slot)| {
        if let Ok(d) = q.get(e) {
            if d.t < SINK_BEGINS {
                return true; // still a solid corpse
            }
        }
        // Sinking now, or the entity is gone: release the slot.
        if let Some(s) = colliders.solids.get_mut(slot) {
            *s = degenerate();
        }
        free.push(slot);
        false
    });
}

/// Promote freshly toppled monsters (clean-death `Dying` with no ragdoll yet) into
/// Verlet ragdolls: snapshot every live bone's world pose into a particle, wire the
/// bone-length + bend constraints, seed a fall-OVER velocity from the kill, and claim
/// a collision slot if one's free (else the body still ragdolls, just not solid).
fn build_ragdolls(
    mut commands: Commands,
    mut slots: ResMut<CorpseSlots>,
    mut q_new: Query<(Entity, &Dying, &Enemy, &mut Knockback), Without<Ragdoll>>,
    q_bones: Query<(Entity, &Bone, &ChildOf, &GlobalTransform), Without<Severed>>,
) {
    for (root, dying, en, mut kb) in &mut q_new {
        // Only the FRESH topple (t≈0); a body already mid-sink never starts a ragdoll.
        if dying.t > 0.5 {
            continue;
        }

        // --- Gather this monster's live bones in a stable local index order. ---
        let mut bones: Vec<Entity> = Vec::new();
        let mut pos: Vec<Vec3> = Vec::new();
        let mut parent_e: Vec<Entity> = Vec::new();
        let mut rest_world_rot: Vec<Quat> = Vec::new();
        let mut is_upper: Vec<bool> = Vec::new();
        let mut is_trunk: Vec<bool> = Vec::new();
        for (be, bone, child_of, gt) in &q_bones {
            if bone.owner != root {
                continue;
            }
            bones.push(be);
            pos.push(gt.translation());
            parent_e.push(child_of.0);
            rest_world_rot.push(gt.rotation());
            is_upper.push(!is_leg(bone.role));
            is_trunk.push(role_is_trunk(bone.role));
        }
        let n = bones.len();
        if n == 0 {
            continue; // nothing to simulate — let animate_death just sink it
        }
        let index_of = |e: Entity| bones.iter().position(|&b| b == e);

        // parent index (-1 = the Enemy root entity, i.e. the root bone). Severing
        // only ever removes leaf-ward subtrees, so a live bone's immediate parent is
        // always either another live bone or the Enemy root — no walk-up needed.
        let parent_idx: Vec<i32> =
            (0..n).map(|i| index_of(parent_e[i]).map(|p| p as i32).unwrap_or(-1)).collect();

        // primary child + rest direction: the child that best continues the bone's
        // chain (root bone: the most-upward child = the spine). Drives orientation.
        let mut primary_child = vec![-1i32; n];
        let mut rest_dir_world = vec![Vec3::ZERO; n];
        for i in 0..n {
            let kids: Vec<usize> = (0..n).filter(|&j| parent_idx[j] == i as i32).collect();
            if kids.is_empty() {
                continue;
            }
            let pick = if parent_idx[i] < 0 {
                // root bone: the child rising highest above it is the trunk.
                *kids.iter().max_by(|&&a, &&b| (pos[a].y - pos[i].y).total_cmp(&(pos[b].y - pos[i].y))).unwrap()
            } else {
                let incoming = (pos[i] - pos[parent_idx[i] as usize]).normalize_or_zero();
                *kids
                    .iter()
                    .max_by(|&&a, &&b| {
                        let da = (pos[a] - pos[i]).normalize_or_zero().dot(incoming);
                        let db = (pos[b] - pos[i]).normalize_or_zero().dot(incoming);
                        da.total_cmp(&db)
                    })
                    .unwrap()
            };
            primary_child[i] = pick as i32;
            rest_dir_world[i] = (pos[pick] - pos[i]).normalize_or_zero();
        }

        // --- Constraints: inextensible bones + skip-one bend (trunk stiff, limbs floppy). ---
        let mut sticks: Vec<Stick> = Vec::new();
        for i in 0..n {
            let p = parent_idx[i];
            if p >= 0 {
                let p = p as usize;
                sticks.push(Stick { a: p, b: i, rest: pos[i].distance(pos[p]), stiff: 1.0 });
                let gp = parent_idx[p];
                if gp >= 0 {
                    let gp = gp as usize;
                    let stiff = if is_trunk[i] && is_trunk[gp] { RAGDOLL_BEND_TRUNK } else { RAGDOLL_BEND_LIMB };
                    sticks.push(Stick { a: gp, b: i, rest: pos[i].distance(pos[gp]), stiff });
                }
            }
        }

        // --- Seed a fall-OVER velocity. The body tips toward `push_dir`: the
        // killing-blow direction if it landed a real shove, else the way it faced
        // (a face-plant). Upper bones get the full throw, feet barely move — the
        // height-scaled differential is the angular momentum that rotates it over. ---
        let kb_h = Vec3::new(kb.0.x, 0.0, kb.0.z);
        let push_dir = if kb_h.length() > 0.1 {
            kb_h.normalize()
        } else {
            (Quat::from_rotation_y(dying.yaw) * Vec3::NEG_Z).normalize_or_zero()
        };
        let pivot_y = pos.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let seed_dt = 1.0 / 60.0;
        let particles: Vec<Particle> = (0..n)
            .map(|i| {
                let bias = if is_upper[i] { RAGDOLL_UPPER_BIAS } else { RAGDOLL_LOWER_BIAS };
                let v = en.vel + push_dir * (bias * RAGDOLL_TOPPLE_OMEGA * (pos[i].y - pivot_y).max(0.0));
                Particle { pos: pos[i], prev: pos[i] - v * seed_dt, is_upper: is_upper[i] }
            })
            .collect();

        // Consume the killing knockback (it's now baked into the seed) so the per-frame
        // solver doesn't re-apply it; later splash/whip/ram impulses land fresh there.
        kb.0 = Vec3::ZERO;

        let order = build_topo_order(&parent_idx);
        let slot = slots.claim(root).unwrap_or(NO_SLOT);

        commands.entity(root).insert(Ragdoll {
            particles,
            sticks,
            bones,
            parent_idx,
            primary_child,
            order,
            rest_world_rot,
            rest_dir_world,
            slot,
            body_box: degenerate(),
            scrape_cd: 0.0,
        });
    }
}

/// Integrate every ragdoll one Verlet step, satisfy its constraints, drape it on the
/// world, rewrite its tracking box, and reconstruct the bone transforms so the
/// rendered rig follows. Frozen once the sink begins (`animate_death` owns it then).
fn ragdoll_solve(
    time: Res<Time>,
    mut colliders: ResMut<WorldColliders>,
    mut q: Query<(&mut Ragdoll, &mut Knockback, &Dying, &GlobalTransform)>,
    mut q_tf: Query<&mut Transform, With<Bone>>,
    q_player: Query<(&GlobalTransform, &Hurtbox), With<crate::player::Player>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    if dt <= 0.0 {
        return;
    }
    let player_box =
        q_player.single().ok().map(|(gt, hb)| Aabb::from_center_half(gt.translation(), hb.half));

    for (mut rag, mut kb, dying, root_gt) in &mut q {
        // Handed back to `animate_death` for the sink — hold the last pose.
        if dying.t >= SINK_BEGINS {
            continue;
        }
        let root_gt = *root_gt;
        // Reborrow out of Bevy's `Mut<>` so the field accesses below (sticks vs.
        // particles, etc.) can split-borrow disjointly.
        let rag = &mut *rag;

        // Take this body's own box out of the world while it solves, so its particles
        // can't collide with it (the truck/door "degenerate before sweep" trick).
        if rag.slot != NO_SLOT {
            if let Some(s) = colliders.solids.get_mut(rag.slot) {
                *s = degenerate();
            }
        }

        // 1. Soak up whatever knockback the damage path accumulated this frame as an
        //    upper-biased velocity impulse (Verlet: nudge `prev` back to add velocity).
        if kb.0 != Vec3::ZERO {
            let imp = kb.0 * RAGDOLL_KNOCK_SCALE;
            for p in &mut rag.particles {
                let b = if p.is_upper { 1.0 } else { 0.4 };
                p.prev -= imp * (b * dt);
            }
            kb.0 = Vec3::ZERO;
        }

        // 2. Verlet integrate under gravity.
        let g = Vec3::NEG_Y * GRAVITY;
        for p in &mut rag.particles {
            let vel = (p.pos - p.prev) * RAGDOLL_DAMPING;
            let next = p.pos + vel + g * (dt * dt);
            p.prev = p.pos;
            p.pos = next;
        }

        // 3. Satisfy the bone-length + bend constraints.
        for _ in 0..RAGDOLL_ITERS {
            for s in &rag.sticks {
                let pa = rag.particles[s.a].pos;
                let pb = rag.particles[s.b].pos;
                let delta = pb - pa;
                let len = delta.length();
                if len < 1e-6 {
                    continue;
                }
                let corr = delta * ((len - s.rest) / len * 0.5 * s.stiff);
                rag.particles[s.a].pos += corr;
                rag.particles[s.b].pos -= corr;
            }
        }

        // 4. Drape on the world: push every particle out of the solids. A particle
        //    shoved upward is resting on something (ground contact).
        let radius = Vec3::splat(RAGDOLL_PARTICLE_RADIUS);
        let mut resting = false;
        for p in &mut rag.particles {
            let before_y = p.pos.y;
            let healed = depenetrate(p.pos, radius, &colliders.solids);
            if healed.y - before_y > 1.0e-3 {
                resting = true;
            }
            p.pos = healed;
        }

        // 5. Realized motion this frame → scrape cue + sleep snap.
        let n = rag.particles.len() as f32;
        let mut max_step = 0.0f32;
        let mut sum_step = Vec3::ZERO;
        for p in &rag.particles {
            let step = p.pos - p.prev;
            sum_step += step;
            max_step = max_step.max(step.length());
        }
        let max_speed = max_step / dt;
        let skid = {
            let mv = sum_step / n / dt;
            Vec3::new(mv.x, 0.0, mv.z).length()
        };
        if resting && skid > CORPSE_SCRAPE_SPEED {
            rag.scrape_cd -= dt;
            if rag.scrape_cd <= 0.0 {
                rag.scrape_cd = 0.18;
                let vol = ((skid - CORPSE_SCRAPE_SPEED) / 8.0).clamp(0.15, 0.6);
                let at = rag.body_box.center();
                sfx.write(Sfx { sound: Sound::CorpseSettle, pos: Some(at), volume: vol, pitch: 1.0 });
            }
        } else {
            rag.scrape_cd = 0.0;
        }
        // Park a grounded, near-still body so solver micro-jitter can't creep it (and
        // keep nudging the player). Only when grounded, so a slow apex doesn't freeze.
        if resting && max_speed < RAGDOLL_SLEEP_SPEED {
            for p in &mut rag.particles {
                p.prev = p.pos;
            }
        }

        // 6. Recompute the tracking box from the particle cloud, clamped so even a
        //    wide sprawl reads as low cover and never pins the player.
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in &rag.particles {
            lo = lo.min(p.pos);
            hi = hi.max(p.pos);
        }
        let center = (lo + hi) * 0.5;
        let half = ((hi - lo) * 0.5).clamp(Vec3::splat(0.15), Vec3::splat(RAGDOLL_BODYBOX_MAX_HALF));
        let body_box = Aabb::from_center_half(center, half);
        rag.body_box = body_box;
        if rag.slot != NO_SLOT {
            // Don't stamp the body solid on a frame where it'd overlap (and shove) the
            // player — leave the slot inert that frame; it self-heals as they separate.
            let solid = !player_box.map_or(false, |pb| pb.overlaps(&body_box));
            if let Some(s) = colliders.solids.get_mut(rag.slot) {
                *s = if solid { body_box } else { degenerate() };
            }
        }

        // 7. Reconstruct bone transforms from the solved particle pose.
        reconstruct(rag, &root_gt, &mut q_tf);
    }
}

/// Drive every bone's LOCAL `Transform` from the solved particle world positions.
///
/// We keep the parent hierarchy, so we can't lean on Bevy's not-yet-propagated
/// `GlobalTransform`s; instead we compute each bone's world transform ourselves in
/// parents-before-children order (`order`), then express it relative to the parent
/// world we just computed. A bone aims its rest direction-to-primary-child onto the
/// solved one (`quat_from_arc`); a leaf rides rigidly with its parent's rotation.
fn reconstruct(rag: &mut Ragdoll, root_gt: &GlobalTransform, q_tf: &mut Query<&mut Transform, With<Bone>>) {
    let n = rag.particles.len();
    let mut world_rot = vec![Quat::IDENTITY; n];
    for &i in &rag.order {
        let pos_i = rag.particles[i].pos;
        world_rot[i] = if rag.primary_child[i] >= 0 {
            let c = rag.primary_child[i] as usize;
            let cur_dir = (rag.particles[c].pos - pos_i).try_normalize().unwrap_or(rag.rest_dir_world[i]);
            quat_from_arc(rag.rest_dir_world[i], cur_dir) * rag.rest_world_rot[i]
        } else if rag.parent_idx[i] >= 0 {
            // Leaf: ride with the parent's rotation-from-rest.
            let p = rag.parent_idx[i] as usize;
            let parent_delta = world_rot[p] * rag.rest_world_rot[p].inverse();
            parent_delta * rag.rest_world_rot[i]
        } else {
            rag.rest_world_rot[i]
        };
    }
    for &i in &rag.order {
        let own = GlobalTransform::from(Transform {
            translation: rag.particles[i].pos,
            rotation: world_rot[i],
            scale: Vec3::ONE,
        });
        let parent_world = if rag.parent_idx[i] >= 0 {
            let p = rag.parent_idx[i] as usize;
            GlobalTransform::from(Transform {
                translation: rag.particles[p].pos,
                rotation: world_rot[p],
                scale: Vec3::ONE,
            })
        } else {
            *root_gt
        };
        if let Ok(mut tf) = q_tf.get_mut(rag.bones[i]) {
            *tf = own.reparented_to(&parent_world);
        }
    }
}

// ----------------------------------------------------------------------------
// Pure helpers (unit-tested below).
// ----------------------------------------------------------------------------

/// Legs (the planted base) get the reduced topple/knockback bias; everything else
/// is "upper body" and takes the full throw.
fn is_leg(role: AnimRole) -> bool {
    use AnimRole::*;
    matches!(role, ThighL | ThighR | ShinL | ShinR | FootL | FootR)
}

/// Trunk roles keep the spine semi-rigid (stiff skip-one bend); limbs flop.
fn role_is_trunk(role: AnimRole) -> bool {
    use AnimRole::*;
    matches!(role, Pelvis | Torso | Chest | Head | Jaw | Tail | Cape)
}

/// Rotation taking unit `from` onto unit `to`, robust to the antiparallel case
/// (`from_rotation_arc` is undefined there) and degenerate inputs.
fn quat_from_arc(from: Vec3, to: Vec3) -> Quat {
    let (from, to) = (from.normalize_or_zero(), to.normalize_or_zero());
    if from == Vec3::ZERO || to == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let d = from.dot(to);
    if d > 0.999_99 {
        return Quat::IDENTITY;
    }
    if d < -0.999_99 {
        // Opposite: spin π about any axis perpendicular to `from`.
        let mut axis = from.cross(Vec3::X);
        if axis.length_squared() < 1e-6 {
            axis = from.cross(Vec3::Y);
        }
        return Quat::from_axis_angle(axis.normalize(), PI);
    }
    Quat::from_rotation_arc(from, to)
}

/// Parents-before-children traversal order over the `parent_idx` forest (roots have
/// parent -1). A plain BFS from the roots — the per-frame reconstruction needs each
/// parent's world transform computed before its children's.
fn build_topo_order(parent_idx: &[i32]) -> Vec<usize> {
    let n = parent_idx.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        if parent_idx[i] < 0 {
            order.push(i);
        } else {
            children[parent_idx[i] as usize].push(i);
        }
    }
    let mut head = 0;
    while head < order.len() {
        let cur = order[head];
        head += 1;
        for &c in &children[cur] {
            order.push(c);
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_degenerate(a: &Aabb) -> bool {
        a.min.x > 1.0e5
    }

    /// The pool is carved onto the END of the existing solids (build-time door /
    /// truck / strand slots stay put), and every reserved slot is inert.
    #[test]
    fn reserve_pool_appends_degenerate_slots_after_build() {
        let mut solids = vec![
            Aabb::from_center_half(Vec3::ZERO, Vec3::ONE),
            Aabb::from_center_half(Vec3::X, Vec3::ONE),
        ];
        let base = solids.len();
        let mut slots = CorpseSlots::default();
        slots.reserve_pool(&mut solids);

        assert_eq!(solids.len(), base + CORPSE_CAP, "pool grows the list by exactly the cap");
        assert_eq!(slots.free.len(), CORPSE_CAP, "every pool slot starts free");
        assert!(!is_degenerate(&solids[0]) && !is_degenerate(&solids[1]));
        for i in base..solids.len() {
            assert!(is_degenerate(&solids[i]), "reserved slot {i} must start inert");
            assert!(slots.free.contains(&i), "slot {i} must be in the free-list");
        }
    }

    /// Claiming hands out distinct slots until the cap, then returns `None` — the
    /// hard body-count cap on how many corpses can be solid at once.
    #[test]
    fn claim_is_capped_and_unique() {
        let mut solids = Vec::new();
        let mut slots = CorpseSlots::default();
        slots.reserve_pool(&mut solids);

        let mut claimed = Vec::new();
        for i in 0..CORPSE_CAP {
            let e = Entity::from_raw_u32(i as u32).unwrap();
            let s = slots.claim(e).expect("a free slot is available below the cap");
            assert!(!claimed.contains(&s), "claimed slots are distinct");
            claimed.push(s);
        }
        let extra = Entity::from_raw_u32(999).unwrap();
        assert!(slots.claim(extra).is_none(), "claiming past the cap yields None");
        assert_eq!(slots.active.len(), CORPSE_CAP);
    }

    /// `quat_from_arc` handles the identity, the antiparallel (180°) case `glam`'s
    /// raw `from_rotation_arc` leaves undefined, and a general rotation.
    #[test]
    fn quat_from_arc_is_robust() {
        let q = quat_from_arc(Vec3::Y, Vec3::Y);
        assert!((q * Vec3::Y).abs_diff_eq(Vec3::Y, 1e-5), "identity maps Y→Y");

        // Antiparallel: Y must map onto -Y with no NaNs.
        let q = quat_from_arc(Vec3::Y, Vec3::NEG_Y);
        let r = q * Vec3::Y;
        assert!(r.is_finite() && r.abs_diff_eq(Vec3::NEG_Y, 1e-4), "Y→-Y, got {r:?}");

        // General: +X onto +Z.
        let q = quat_from_arc(Vec3::X, Vec3::Z);
        assert!((q * Vec3::X).abs_diff_eq(Vec3::Z, 1e-5), "X→Z");

        // Degenerate input is identity, not NaN.
        assert_eq!(quat_from_arc(Vec3::ZERO, Vec3::Y), Quat::IDENTITY);
    }

    /// One distance-constraint pass pulls a stretched stick back toward its rest
    /// length, moving both endpoints symmetrically at full stiffness.
    #[test]
    fn stick_solve_pulls_to_rest() {
        let mut p = [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0)];
        let s = Stick { a: 0, b: 1, rest: 1.0, stiff: 1.0 };
        let delta = p[s.b] - p[s.a];
        let len = delta.length();
        let corr = delta * ((len - s.rest) / len * 0.5 * s.stiff);
        p[s.a] += corr;
        p[s.b] -= corr;
        // Stretched 2→1: each endpoint moves inward by 0.5 in one full pass.
        assert!(p[0].abs_diff_eq(Vec3::new(0.5, 0.0, 0.0), 1e-5), "a moved in, got {:?}", p[0]);
        assert!(p[1].abs_diff_eq(Vec3::new(1.5, 0.0, 0.0), 1e-5), "b moved in, got {:?}", p[1]);
        assert!((p[1] - p[0]).length() - 1.0 < 1e-5, "now at rest length");
    }

    /// The topo order lists every parent before its children for a small forest.
    #[test]
    fn topo_order_is_parents_first() {
        // 0:root → 1,2 ; 1 → 3 ; 3 → 4
        let parent = [-1, 0, 0, 1, 3];
        let order = build_topo_order(&parent);
        assert_eq!(order.len(), 5, "every node appears once");
        let pos: std::collections::HashMap<usize, usize> =
            order.iter().enumerate().map(|(rank, &node)| (node, rank)).collect();
        for (child, &p) in parent.iter().enumerate() {
            if p >= 0 {
                assert!(pos[&child] > pos[&(p as usize)], "child {child} after parent {p}");
            }
        }
    }
}
