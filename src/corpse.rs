//! Ragdoll-brush corpses (feature 17): a cleanly-killed monster's body becomes a
//! real, shovable solid for the few seconds it lingers — cover that didn't exist
//! a second ago, a barricade you can plug a doorway with, a thing to ram into a
//! sludge canal or boot off a ledge.
//!
//! ## How it hooks into the rest of the engine
//!
//! It reuses the truck's whole trick from [`crate::vehicle`]: a slot in
//! [`WorldColliders`] rewritten every frame from a transform and swept through
//! [`move_and_slide`], so the player's collision, the monsters' pathing, hitscan
//! and line-of-sight all treat the body as solid for free. Where the truck
//! reserves ONE slot for the level's life, corpses come and go, so this carves a
//! small **pool** of slots at build time (`CORPSE_CAP`) and hands them out via a
//! runtime free-list (claim on a clean kill, release on despawn). A disabled slot
//! holds the same far-away [`degenerate`] sentinel a closed door / parked strand
//! uses, so the slot is inert but its index stays stable for the whole level.
//!
//! Knockback feeds in for free: the body keeps its [`Knockback`] component, so a
//! rocket splash (`combat.rs`), a Whip fling (`weapons.rs`) and a truck ram
//! (`vehicle.rs`) all accumulate into it through the normal `DamageEvent` path —
//! we just consume it here and integrate. Gravity + restitution make a flung body
//! skid, tumble down stairs and drop off ledges; ground-drag settles it into
//! cover quickly. The topple/sink/despawn timeline stays in `monster_model.rs`'s
//! `animate_death`; we only own the collider, releasing the slot when the entity
//! despawns (timer-out OR dissolved in lava by `gamestate.rs`).
//!
//! ### Why the cap matters — and why we don't push the player around
//!
//! A swept box that keeps shoving could wedge the player into a wall or jam a
//! door. We keep that safe two ways: the cap is tight (6), and a grounded corpse
//! is parked dead-still below `CORPSE_SLEEP_SPEED` (its slot stops moving), so a
//! body at rest is just static cover — it never creeps. The player resolves its
//! own collision *after* us (`corpse_physics` runs before `player_move`), and the
//! solver's `depenetrate` net pushes the player cleanly out if a body lands on
//! them, so a corpse can block you but never grind you through geometry.

use bevy::prelude::*;

use crate::common::{tune::*, *};
use crate::monster_model::Dying;
use crate::physics::{move_and_slide, Aabb};

/// Step-up for a sliding corpse (cross small seams; don't let bodies climb stairs).
const CORPSE_STEP: f32 = 0.2;

/// A degenerate AABB parked a million metres away — the same inert sentinel a
/// closed door / cut strand / parked truck slot uses. Nothing can reach it, so the
/// slot is "off" without changing the collider Vec's length (index stays stable).
pub(crate) fn degenerate() -> Aabb {
    Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

/// A corpse that has been promoted to a live moving collider. Holds its reserved
/// slot, its velocity (gravity + knockback skid), and the AABB half-extents sized
/// to the rig's footprint at death. The slot is released by [`reclaim_corpse_slots`]
/// once the entity despawns (topple-timer out or dissolved in a hazard).
#[derive(Component)]
pub struct CorpseBody {
    /// Index of this corpse's slot in `WorldColliders.solids`.
    pub slot: usize,
    /// World velocity: x/z = knockback skid bled off by drag, y = gravity/settling.
    pub vel: Vec3,
    /// Collider half-extents (the dead monster's body box, a touch squatter than
    /// the standing hurtbox so a toppled body reads as low cover).
    pub half: Vec3,
    /// Cooldown gating the settle/scrape cue so a long skid isn't a stutter of thuds.
    pub scrape_cd: f32,
}

/// The runtime free-list over the corpse collider pool. The pool is `CORPSE_CAP`
/// degenerate slots reserved at level build time (so every index is stable for the
/// level's life); `free` lists which of them are currently unused, and `active`
/// maps each live corpse entity to the slot it holds so we can reclaim it the
/// frame the entity vanishes (whatever despawned it).
#[derive(Resource, Default)]
pub struct CorpseSlots {
    /// Slot indices currently free to claim.
    free: Vec<usize>,
    /// (corpse entity, slot) for every live corpse collider.
    active: Vec<(Entity, usize)>,
}
impl CorpseSlots {
    /// Carve `CORPSE_CAP` fresh degenerate slots onto the end of `solids` and reset
    /// the free-list to own exactly them. Call once per level build, AFTER all the
    /// build-time door/truck/strand reservations, so the corpse indices come last
    /// and never collide with them.
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

pub struct CorpsePlugin;
impl Plugin for CorpsePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CorpseSlots>().add_systems(
            Update,
            (reclaim_corpse_slots, promote_corpses, corpse_physics)
                .chain()
                // Before the player resolves its own collision, so the body has
                // already moved this frame and the player slides off / is pushed
                // out of it cleanly (never the other way around).
                .before(crate::player::player_move)
                // Before the topple/sink animator, so on the sink-handoff frame the
                // corpse's swept translation write is gated ahead of `animate_death`'s
                // sink (the two never fight over the Transform).
                .before(crate::monster_model::animate_death)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// The `Dying.t` at which `monster_model::animate_death` begins sinking the body
/// into the floor before it despawns. Once sinking starts we hand the body back to
/// the topple animator: release its collider slot and strip `CorpseBody` so the two
/// systems never both write its `Transform` (the physics sweep would fight the sink).
/// Hoisted into `tune::` so it stays coupled with `animate_death`'s sink/despawn `t`.
const SINK_BEGINS: f32 = CORPSE_SINK_BEGINS;

/// Reclaim corpse slots, freeing them in two cases: the corpse despawned since last
/// frame (topple-timer out, or dissolved in lava — its `CorpseBody` is gone with the
/// entity), or it has begun its sink (then we drop `CorpseBody` so `animate_death`
/// owns the final descent uncontested). We track (entity, slot) in the resource so
/// a reclaim works however the corpse left collider duty.
fn reclaim_corpse_slots(
    mut commands: Commands,
    mut slots: ResMut<CorpseSlots>,
    mut colliders: ResMut<WorldColliders>,
    q_body: Query<&Dying, With<CorpseBody>>,
) {
    let CorpseSlots { active, free } = &mut *slots;
    active.retain(|&(e, slot)| {
        // Keep the slot only while the entity is still a *solid* corpse: it exists
        // (CorpseBody present) and hasn't started sinking yet.
        match q_body.get(e) {
            Ok(d) if d.t < SINK_BEGINS => return true,
            Ok(_) => {
                // Sinking now: release the slot and hand the body to the animator.
                commands.entity(e).remove::<CorpseBody>();
            }
            Err(_) => {} // entity (and its CorpseBody) is gone
        }
        if let Some(s) = colliders.solids.get_mut(slot) {
            *s = degenerate();
        }
        free.push(slot);
        false
    });
}

/// Promote freshly toppled monsters (clean-death `Dying` with no collider yet) into
/// ragdoll-brush corpses: claim a free slot, size an AABB to the body, and lift it
/// off the ground a hair so the swept solver settles it on its SKIN gap rather than
/// frozen on the floor boundary. When the pool is full the kill stays a decorative
/// topple (the existing bodies keep their slots — see `CORPSE_CAP`).
fn promote_corpses(
    mut commands: Commands,
    mut slots: ResMut<CorpseSlots>,
    mut colliders: ResMut<WorldColliders>,
    mut q_new: Query<
        (Entity, &mut Transform, &Hurtbox, &Dying, &crate::enemies::Enemy),
        (Without<CorpseBody>, Without<crate::player::Player>),
    >,
    q_player: Query<(&Transform, &Hurtbox), With<crate::player::Player>>,
) {
    // Once the pool is empty there's nothing to hand out — bail before the query
    // work so a massacre past the cap is essentially free.
    if slots.free.is_empty() {
        return;
    }
    // The player's resolved box this frame: don't stamp a solid corpse on top of it
    // (a point-blank Whip/SSG kill in a tight corridor could otherwise depenetrate
    // the player into the wall). We leave such a kill a decorative topple instead.
    let player_box = q_player
        .single()
        .ok()
        .map(|(ptf, phb)| Aabb::from_center_half(ptf.translation, phb.half));
    for (e, mut tf, hb, dying, en) in &mut q_new {
        // Only the FRESH topple (t≈0); skip a body already mid-sink/despawn so a
        // late-claimed slot can't pop a corpse back up.
        if dying.t > 0.5 {
            continue;
        }
        // A toppled body is squatter than the standing rig: drop the height so it
        // reads as low cover, and pull the footprint in (so even the widest body
        // never approaches half a 4m corridor and pins the player) — capped so a
        // Grunt and an Ogre corpse read as similar low cover, not a wall.
        let half = Vec3::new(
            (hb.half.x * 0.7).min(0.45),
            (hb.half.y * 0.55).max(0.3),
            (hb.half.z * 0.7).min(0.45),
        );
        let lifted = tf.translation + Vec3::Y * CORPSE_SPAWN_LIFT;
        let body_box = Aabb::from_center_half(lifted, half);
        // Don't go solid right on the player — stay a decorative topple this frame.
        if player_box.map_or(false, |pb| pb.overlaps(&body_box)) {
            continue;
        }
        let Some(slot) = slots.claim(e) else { break };
        // Lift onto the SKIN gap (the truck's "don't sit exactly on the floor"
        // gotcha) so the first sweep leaves a settling gap instead of freezing.
        tf.translation = lifted;
        if let Some(s) = colliders.solids.get_mut(slot) {
            *s = body_box;
        }
        // Seed the skid from the dying monster's last velocity so the fatal blow's
        // momentum (e.g. a point-blank rocket fling) carries into the corpse instead
        // of being dropped — the body keeps moving the way the kill threw it.
        commands.entity(e).insert(CorpseBody { slot, vel: en.vel, half, scrape_cd: 0.0 });
    }
}

/// Integrate every live corpse body and rewrite its collider slot — the same
/// per-frame "degenerate before sweep, footprint after" trick the truck uses.
/// Pulls in knockback the normal `DamageEvent` path accumulated (rocket splash,
/// Whip fling, truck ram), applies gravity + restitution off walls, and bleeds the
/// skid to rest with drag so a body settles into cover and stops dead.
fn corpse_physics(
    time: Res<Time>,
    mut colliders: ResMut<WorldColliders>,
    mut q: Query<(&mut Transform, &mut CorpseBody, &mut Knockback, &Dying)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut tf, mut body, mut kb, dying) in &mut q {
        // Once sinking starts the body is handed back to `animate_death`; skip it
        // here so the two systems never both drive its Transform (and so a slot
        // that `reclaim_corpse_slots` freed this frame isn't re-stamped by a corpse
        // whose `CorpseBody` removal hasn't flushed yet).
        if dying.t >= SINK_BEGINS {
            continue;
        }
        // Soak up whatever knockback the damage path accumulated this frame
        // (explosion splash, Whip fling, ram impulse) as a velocity impulse.
        if kb.0 != Vec3::ZERO {
            body.vel += kb.0;
            kb.0 = Vec3::ZERO;
        }

        let half = body.half;
        let vy = body.vel.y - GRAVITY * dt;
        let vel = Vec3::new(body.vel.x, vy, body.vel.z);

        // Sweep the body box excluding its own slot, exactly like the truck, so it
        // can't self-collide during the move.
        if let Some(s) = colliders.solids.get_mut(body.slot) {
            *s = degenerate();
        }
        let res = move_and_slide(tf.translation, half, vel, dt, &colliders.solids, CORPSE_STEP);

        // Restitution off a wall: rebound out along the surface normal so a rocketed
        // body skips off a wall instead of sticking flat to it.
        let mut out = res.vel;
        if res.hit_wall {
            let n = res.wall_normal;
            if n.length_squared() > 1e-6 {
                let n = n.normalize();
                let into = vel.dot(n); // < 0 when driving into the wall
                if into < 0.0 {
                    out += n * (-into * CORPSE_REST);
                }
            }
        }

        // Drag: heavy on the ground (settle into cover fast and quit nudging the
        // player), light in the air (keep a flung arc). Then park a slow grounded
        // body dead-still so it stops creeping from solver micro-jitter.
        let mut planar = Vec3::new(out.x, 0.0, out.z);
        let drag = if res.on_ground { CORPSE_GROUND_DRAG } else { CORPSE_AIR_DRAG };
        planar *= (-drag * dt).exp();
        let out_vy = if res.on_ground { out.y.max(0.0) } else { out.y };
        if res.on_ground && planar.length() < CORPSE_SLEEP_SPEED {
            planar = Vec3::ZERO;
        }

        // A faint settle/scrape cue while the body is actually skidding along the
        // ground (gated by a cooldown so a long slide isn't a machine-gun of thuds).
        let speed = planar.length();
        if res.on_ground && speed > CORPSE_SCRAPE_SPEED {
            body.scrape_cd -= dt;
            if body.scrape_cd <= 0.0 {
                body.scrape_cd = 0.18;
                let vol = ((speed - CORPSE_SCRAPE_SPEED) / 8.0).clamp(0.15, 0.6);
                sfx.write(Sfx { sound: Sound::CorpseSettle, pos: Some(tf.translation), volume: vol, pitch: 1.0 });
            }
        } else {
            body.scrape_cd = 0.0;
        }

        tf.translation = res.pos;
        body.vel = Vec3::new(planar.x, out_vy, planar.z);

        // Rewrite the slot to the body's new resting box (centre = transform).
        if let Some(s) = colliders.solids.get_mut(body.slot) {
            *s = Aabb::from_center_half(tf.translation, half);
        }
    }
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
            // a couple of "build-time" brushes already in the list
            Aabb::from_center_half(Vec3::ZERO, Vec3::ONE),
            Aabb::from_center_half(Vec3::X, Vec3::ONE),
        ];
        let base = solids.len();
        let mut slots = CorpseSlots::default();
        slots.reserve_pool(&mut solids);

        assert_eq!(solids.len(), base + CORPSE_CAP, "pool grows the list by exactly the cap");
        assert_eq!(slots.free.len(), CORPSE_CAP, "every pool slot starts free");
        // The pre-existing brushes are untouched; the new ones are all degenerate.
        assert!(!is_degenerate(&solids[0]) && !is_degenerate(&solids[1]));
        for i in base..solids.len() {
            assert!(is_degenerate(&solids[i]), "reserved slot {i} must start inert");
            assert!(slots.free.contains(&i), "slot {i} must be in the free-list");
        }
    }

    /// Claiming hands out distinct slots until the cap, then returns `None` — the
    /// hard body-count cap that stops a massacre spawning dozens of swept boxes.
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
        // Pool exhausted: the next kill gets no slot (stays a decorative topple).
        let extra = Entity::from_raw_u32(999).unwrap();
        assert!(slots.claim(extra).is_none(), "claiming past the cap yields None");
        assert_eq!(slots.active.len(), CORPSE_CAP);
    }

    /// A fresh corpse is lifted onto the SKIN gap, never resting its centre exactly
    /// on the floor (the truck's "don't sit exactly on the floor" freeze gotcha).
    #[test]
    fn spawn_lift_is_positive() {
        assert!(CORPSE_SPAWN_LIFT > 0.0, "corpses must spawn a hair above the floor");
    }
}
