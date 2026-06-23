//! Projectiles: rockets, grenades, nails, and enemy energy bolts.

use bevy::prelude::*;

use crate::common::tune::*;
use crate::common::*;
use crate::effects::{spawn_particle, spawn_sparks};
use crate::monster_model::nearest_limb_hit;
use crate::physics::{ray_aabb, raycast_world, Aabb};

pub struct ProjectilePlugin;
impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WhipParry>().add_systems(
            Update,
            // The parry sweep runs BEFORE the integrator so a projectile flipped
            // player-owned this frame is already friendly when `projectile_move`
            // resolves its collision (and a popped one is gone before it can hit).
            (whip_parry, projectile_move)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// The Whip's active parry window, opened by a left-click Whip swing
/// (`weapons::fire_weapon`) and consumed by `whip_parry`. While `window > 0` the
/// Whip's melee arc is swept against enemy-owned projectiles each frame. The
/// captured `origin`/`dir` are the camera's at swing time — the SAME geometry the
/// melee lash uses — so the parry reuses the lash's reach, not a second hitbox.
#[derive(Resource, Default)]
pub struct WhipParry {
    /// Seconds left in the active window (0 = closed).
    pub window: f32,
    /// Seconds left in the leading PERFECT sub-window (0 = past the perfect slice).
    pub perfect: f32,
    /// Swing ray origin (camera position at swing time).
    pub origin: Vec3,
    /// Swing aim direction (normalized).
    pub dir: Vec3,
    /// Whip melee reach (m) at swing time, used as the parry sweep length.
    pub reach: f32,
    /// The player entity that swung — the new owner of anything it bats back.
    pub player: Option<Entity>,
}
impl WhipParry {
    /// Open (or refresh) the window for a fresh Whip swing.
    pub fn open(&mut self, origin: Vec3, dir: Vec3, reach: f32, player: Entity) {
        self.window = crate::common::tune::PARRY_WINDOW;
        self.perfect = crate::common::tune::PARRY_PERFECT_WINDOW;
        self.origin = origin;
        self.dir = dir;
        self.reach = reach;
        self.player = Some(player);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjKind {
    Rocket,
    Grenade,
    Nail,
    Bolt,
}

// --- Lodestone (gravity-well grenade) tuning ---------------------------------
/// Pull reach (m): the radius of the inward gravity well.
const LODESTONE_RADIUS: f32 = 10.0;
/// Inward acceleration on monsters (m/s; scaled by dt where it's emitted).
const LODESTONE_PULL_ACCEL: f32 = 45.0;
/// Inward acceleration on loose projectiles / enemy bolts (m/s; scaled by dt).
const PROJ_PULL_ACCEL: f32 = 30.0;

#[derive(Component)]
pub struct Projectile {
    pub vel: Vec3,
    pub life: f32,
    pub fuse: f32,
    pub gravity: f32,
    pub kind: ProjKind,
    pub from_player: bool,
    pub source: Option<Entity>,
    pub damage: f32,
    pub splash_radius: f32,
    pub splash_damage: f32,
    pub push: f32,
    pub trail: f32,
    /// A Lodestone gravity-well grenade: bounces & settles like a normal grenade
    /// but for its fuse emits an inward (imploding) pull instead of detonating on
    /// contact, then pops with a small real blast when the fuse runs out.
    pub lodestone: bool,
    /// True once this projectile was batted back by a Whip parry (feature 39). It
    /// now belongs to the player (`from_player = true`, `source = player`), but
    /// this flag makes its immunity to its NEW owner AIRTIGHT: a returned grenade's
    /// `ExplosionEvent` carries `returned = true`, and `combat::handle_explosions`
    /// excludes the source (the player) from a returned blast entirely — so the
    /// player can NEVER be hurt by the splash of a shot they batted back, no matter
    /// the geometry.
    pub returned: bool,
}

impl Projectile {
    fn hits_faction(&self, f: Faction) -> bool {
        match f {
            Faction::Player => !self.from_player,
            Faction::Monster => self.from_player,
        }
    }
    fn explosive(&self) -> bool {
        matches!(self.kind, ProjKind::Rocket | ProjKind::Grenade)
    }
}

/// Spawn a projectile with kind-appropriate stats.
pub fn spawn_projectile(
    commands: &mut Commands,
    gfx: &GfxAssets,
    kind: ProjKind,
    pos: Vec3,
    dir: Vec3,
    from_player: bool,
    source: Option<Entity>,
) {
    let dir = dir.normalize_or_zero();
    let (speed, life, fuse, gravity, dmg, sr, sd, push, mat, scale, light) = match kind {
        ProjKind::Rocket => (30.0, 6.0, 0.0, 0.0, 0.0, 4.0, 90.0, 16.0, gfx.rocket.clone(), 0.25, true),
        ProjKind::Grenade => (16.0, 6.0, 2.0, 18.0, 0.0, 3.6, 85.0, 13.0, gfx.grenade.clone(), 0.22, false),
        ProjKind::Nail => (55.0, 3.0, 0.0, 0.0, 9.0, 0.0, 0.0, 1.5, gfx.nail.clone(), 0.10, false),
        ProjKind::Bolt => (20.0, 5.0, 0.0, 0.0, 10.0, 0.0, 0.0, 2.0, gfx.plasma.clone(), 0.18, false),
    };
    let mut e = commands.spawn((
        Mesh3d(gfx.sphere.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(pos + dir * 0.4).with_scale(Vec3::splat(scale)),
        Projectile {
            vel: dir * speed,
            life,
            fuse,
            gravity,
            kind,
            from_player,
            source,
            damage: dmg,
            splash_radius: sr,
            splash_damage: sd,
            push,
            trail: 0.0,
            lodestone: false,
            returned: false,
        },
        LevelEntity,
    ));
    if light {
        e.with_children(|p| {
            p.spawn((
                PointLight { color: rgb(1.0, 0.6, 0.3), intensity: 120_000.0, range: 8.0, shadow_maps_enabled: false, ..default() },
                Transform::default(),
            ));
        });
    }
}

/// Spawn a Lodestone: a Grenade-physics projectile flagged `lodestone`. It bounces
/// off world geometry and settles like a normal grenade, but for its fuse it acts
/// as an inward gravity well (handled in `projectile_move`) rather than detonating
/// on contact; the splash stats below are the small real pop when the fuse expires.
pub fn spawn_lodestone(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3, dir: Vec3, source: Entity) {
    let dir = dir.normalize_or_zero();
    commands
        .spawn((
            Mesh3d(gfx.sphere.clone()),
            MeshMaterial3d(gfx.plasma.clone()),
            Transform::from_translation(pos + dir * 0.4).with_scale(Vec3::splat(0.3)),
            Projectile {
                vel: dir * 14.0,
                life: 2.2,
                fuse: 2.0,
                gravity: 18.0,
                kind: ProjKind::Grenade,
                from_player: true,
                source: Some(source),
                damage: 0.0,
                splash_radius: 3.0,
                splash_damage: 45.0,
                push: 8.0,
                trail: 0.0,
                lodestone: true,
                returned: false,
            },
            LevelEntity,
        ))
        .with_children(|p| {
            p.spawn((
                PointLight { color: rgb(0.5, 0.2, 1.0), intensity: 150_000.0, range: 10.0, shadow_maps_enabled: false, ..default() },
                Transform::default(),
            ));
        });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn projectile_move(
    mut commands: Commands,
    time: Res<Time>,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
    limb_boxes: Res<LimbBoxes>,
    mut q: Query<(Entity, &mut Transform, &mut Projectile)>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut expl: MessageWriter<ExplosionEvent>,
    mut impact: MessageWriter<ImpactEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let target_list: Vec<(Entity, Vec3, Vec3)> = targets
        .iter()
        .map(|(e, gt, hb, _f)| (e, gt.translation(), hb.half))
        .collect();
    let target_fac: std::collections::HashMap<Entity, Faction> =
        targets.iter().map(|(e, _, _, f)| (e, *f)).collect();

    // Active gravity wells (live Lodestones). Collected up front with an immutable
    // iter so the mutable per-projectile loop below can pull every other projectile
    // toward them without a borrow conflict.
    let wells: Vec<(Entity, Vec3, f32)> = q
        .iter()
        .filter_map(|(e, tf, p)| (p.lodestone && p.fuse > 0.0).then(|| (e, tf.translation, LODESTONE_RADIUS)))
        .collect();

    for (e, mut tf, mut p) in &mut q {
        p.life -= dt;
        if p.fuse > 0.0 {
            p.fuse -= dt;
        }
        if p.gravity != 0.0 {
            p.vel.y -= p.gravity * dt;
        }

        // Trail.
        p.trail -= dt;
        if p.trail <= 0.0 && p.explosive() {
            p.trail = 0.02;
            spawn_particle(&mut commands, gfx.small_sphere.clone(), gfx.smoke.clone(), tf.translation, Vec3::ZERO, 0.4, -0.5, 0.0, 0.5, 0.0);
        }

        // Lodestone: bounce off world geometry only (it never detonates on contact —
        // it settles), emit the per-frame inward pull, pop on fuse expiry, then skip
        // the normal collision/explosion sweep entirely.
        if p.lodestone {
            // World-only swept bounce, then settle.
            let mut remaining = dt;
            for _ in 0..3 {
                if remaining <= 1e-5 {
                    break;
                }
                let step = p.vel * remaining;
                let dist = step.length();
                if dist < 1e-5 {
                    break;
                }
                let dir = step / dist;
                if let Some((t, _pt, n)) = raycast_world(tf.translation, dir, dist, &colliders.solids) {
                    tf.translation += dir * t;
                    remaining *= 1.0 - (t / dist);
                    let into = p.vel.dot(n);
                    if into < 0.0 {
                        p.vel -= n * into * 1.4;
                    }
                    p.vel *= 0.6;
                    tf.translation += n * 0.05;
                } else {
                    tf.translation += step;
                    break;
                }
            }
            // Per-frame inward pull (handled by handle_explosions, implode=true). push is the
            // per-Update-frame velocity impulse magnitude (PULL_ACCEL * dt) => framerate independent.
            expl.write(ExplosionEvent {
                pos: tf.translation,
                radius: LODESTONE_RADIUS,
                damage: 0.0,
                source: p.source,
                from_player: true,
                color: rgb(0.6, 0.3, 1.0),
                push: LODESTONE_PULL_ACCEL * dt,
                implode: true,
                direct_limb: None,
                returned: false,
            });
            if p.fuse <= 0.0 {
                let pos = tf.translation;
                expl.write(ExplosionEvent {
                    pos,
                    radius: p.splash_radius,
                    damage: p.splash_damage,
                    source: p.source,
                    from_player: true,
                    color: rgb(1.0, 0.6, 0.2),
                    push: p.push,
                    implode: false,
                    direct_limb: None,
                    returned: false,
                });
                sfx.write(Sfx::pitched(Sound::Explosion, pos, 0.8));
                commands.entity(e).despawn();
                continue;
            }
            if p.life <= 0.0 {
                commands.entity(e).despawn();
            }
            continue;
        }

        // Loose projectiles (grenades, enemy bolts, nails…) curve toward each live
        // well, so a fired-back grenade or an Enforcer volley becomes trap fodder too.
        if !wells.is_empty() {
            for &(we, wpos, wr) in &wells {
                if we == e {
                    continue;
                }
                let to = wpos - tf.translation;
                let d = to.length();
                if d > 0.5 && d < wr {
                    let reach = 1.0 - d / wr;
                    p.vel += (to / d) * PROJ_PULL_ACCEL * dt * reach;
                }
            }
        }

        let mut remaining = dt;
        let mut exploded = false;

        for _ in 0..3 {
            if remaining <= 1e-5 {
                break;
            }
            let step = p.vel * remaining;
            let dist = step.length();
            if dist < 1e-5 {
                break;
            }
            let dir = step / dist;

            // World first, so monster/limb hits only count in front of a wall.
            let mut best_t = dist;
            let mut hit_world: Option<(Vec3, Vec3)> = None;
            if let Some((t, pt, n)) = raycast_world(tf.translation, dir, best_t, &colliders.solids) {
                best_t = t;
                hit_world = Some((pt, n));
            }
            // Player projectiles refine to a specific bone (preferred over the
            // broad body box — same rule as the hitscan weapons).
            let limb = if p.from_player {
                nearest_limb_hit(tf.translation, dir, best_t, &limb_boxes.boxes, 0.0)
            } else {
                None
            };
            // Broad body box: fallback, and the path for enemy bolts hitting the player.
            let mut body_t = best_t;
            let mut body_hit: Option<(Entity, Vec3, Vec3)> = None;
            for &(te, center, half) in &target_list {
                let f = target_fac.get(&te).copied().unwrap_or(Faction::Monster);
                if !p.hits_faction(f) {
                    continue;
                }
                let b = Aabb::from_center_half(center, half);
                if let Some((t, n)) = ray_aabb(tf.translation, dir, body_t, &b) {
                    body_t = t;
                    body_hit = Some((te, tf.translation + dir * t, n));
                }
            }

            // Resolve: a bone hit wins, else the broad body box, else the wall.
            let mut hit_target: Option<(Entity, Vec3, Vec3)> = None;
            let mut hit_group: Option<LimbGroup> = None;
            if let Some((e, g, t, n)) = limb {
                best_t = t;
                hit_target = Some((e, tf.translation + dir * t, n));
                hit_group = Some(g);
                hit_world = None;
            } else if let Some((e, pt, n)) = body_hit {
                best_t = body_t;
                hit_target = Some((e, pt, n));
                hit_world = None;
            }

            if hit_target.is_none() && hit_world.is_none() {
                tf.translation += step;
                break;
            }

            tf.translation += dir * best_t;
            remaining *= 1.0 - (best_t / dist);

            if p.explosive() {
                // Rockets explode on any contact; grenades only on a direct
                // monster/player hit (they bounce off world geometry).
                let explode_now = p.kind == ProjKind::Rocket || hit_target.is_some();
                if explode_now {
                    let pos = tf.translation;
                    let direct_limb = match (hit_target, hit_group) {
                        (Some((te, _, _)), Some(g)) => Some((te, g)),
                        _ => None,
                    };
                    expl.write(ExplosionEvent {
                        pos,
                        radius: p.splash_radius,
                        damage: p.splash_damage,
                        source: p.source,
                        from_player: p.from_player,
                        color: rgb(1.0, 0.6, 0.2),
                        push: p.push,
                        implode: false,
                        direct_limb,
                        returned: p.returned,
                    });
                    sfx.write(Sfx::at(Sound::Explosion, pos));
                    exploded = true;
                    break;
                } else if let Some((_pt, n)) = hit_world {
                    // Grenade bounce off the surface (reflect with restitution).
                    let into = p.vel.dot(n);
                    if into < 0.0 {
                        p.vel -= n * into * 1.6;
                    }
                    p.vel *= 0.7;
                    sfx.write(Sfx::at(Sound::GrenadeBounce, tf.translation));
                    tf.translation += n * 0.05;
                }
            } else {
                // Direct-hit projectile (nail / bolt).
                if let Some((te, pt, n)) = hit_target {
                    let knock = dir * p.push;
                    if let Some(g) = hit_group {
                        dmg.write(DamageEvent::limb(te, p.damage, p.source, knock, g));
                    } else {
                        dmg.write(DamageEvent::body(te, p.damage, p.source, knock));
                    }
                    impact.write(ImpactEvent { pos: pt, normal: n, blood: true });
                } else if let Some((pt, n)) = hit_world {
                    impact.write(ImpactEvent { pos: pt, normal: n, blood: false });
                }
                exploded = true; // reuse flag = "consumed"
                break;
            }
        }

        // Grenade fuse expiry. A timed (non-impact) detonation gets a 50% larger
        // blast radius than a grenade that explodes on a direct hit.
        if !exploded && p.kind == ProjKind::Grenade && p.fuse <= 0.0 {
            let pos = tf.translation;
            expl.write(ExplosionEvent { pos, radius: p.splash_radius * 1.5, damage: p.splash_damage, source: p.source, from_player: p.from_player, color: rgb(1.0, 0.6, 0.2), push: p.push, implode: false, direct_limb: None, returned: p.returned });
            sfx.write(Sfx::at(Sound::Explosion, pos));
            exploded = true;
        }

        if exploded || p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Pure reflect-and-aim for a parried projectile (feature 39). `vel` is the
/// incoming (toward-player) velocity, `aim` the unit swing/aim direction (the
/// caller has already verified `vel.dot(aim) < 0`, i.e. it's heading at the
/// player). Reflects `vel` about the swing plane (normal = `aim`) using the same
/// `v -= n*(v·n)*coeff` math the grenade wall-bounce uses (coeff = 2 → a clean
/// mirror), so the shot screams back along the reflection of its own path. Then it
/// is nudged toward the aim (which the player has pointed at the shooter): a LATE
/// parry gets a small nudge (`PARRY_HOMING_LATE`) so it has a fighting chance to
/// connect; a `perfect` parry both speeds it up and homes harder (`PARRY_HOMING`).
/// Both homing amounts keep a positive aim-component (`out·aim > 0` after the
/// mirror), so the result can never point back at the player.
fn parry_reflect(vel: Vec3, aim: Vec3, perfect: bool) -> Vec3 {
    let into = vel.dot(aim);
    let mut out = vel - aim * into * 2.0; // mirror about the swing plane
    let homing = if perfect { PARRY_HOMING } else { PARRY_HOMING_LATE };
    if perfect {
        out *= PARRY_PERFECT_SPEED;
    }
    let speed = out.length();
    let homed = out.normalize_or_zero().lerp(aim, homing).normalize_or_zero();
    if homed.length_squared() > 1e-6 {
        out = homed * speed;
    }
    out
}

/// Whip parry (feature 39): while the parry window is open, sweep the Whip's
/// melee arc against every ENEMY-owned projectile and bat the ones it catches.
///
/// A CLEAN parry (window open, projectile in the arc):
///  - reflects the projectile's velocity about the swing plane (normal = aim) with
///    the very same `v -= n*(v·n)*coeff` math the grenade bounce uses, so it screams
///    back along the reflection of its own path (a PERFECT parry adds a speed bonus
///    and a homing nudge toward the shooter);
///  - FLIPS ownership enemy->player (`from_player = true`, `source = player`), so on
///    its next collision it runs the normal PLAYER-damage path against MONSTERS and
///    can NEVER harm the player (see the ownership note below).
/// A LATE parry (still inside `PARRY_WINDOW` but past the perfect slice on a
/// projectile that for any reason can't be cleanly returned — here we treat every
/// in-arc catch as a deflect; the perfect/late split is the bonus, not pop-vs-return)
/// — see the brief: a sloppy swing just POPS it. We pop (despawn) only the rare
/// degenerate case where the incoming velocity isn't actually moving toward the
/// player along the aim (nothing sensible to reflect), so the player still eats
/// nothing and gets no free kill.
///
/// OWNERSHIP SAFETY (the #1 correctness requirement): a parried projectile is the
/// SAME entity, but with `from_player` flipped to `true`, `source` set to the
/// player, and `returned` set to `true`. Every damage decision keys off these
/// fields and nothing else — and the player is immune by construction, NOT by
/// geometry:
///  - DIRECT hits in `projectile_move` skip any target whose faction fails
///    `Projectile::hits_faction(f)`, which returns `!from_player` for the player —
///    i.e. `false` once flipped, so the player is never a valid direct target;
///  - SPLASH from a batted grenade writes `ExplosionEvent { from_player: true,
///    source: Some(player), returned: true }`. `combat::handle_explosions` runs the
///    player-faction path (which would normally clip the firer for half damage), but
///    the `returned && self_blast` guard there skips the explosion's source — the
///    player — entirely. So a returned grenade can hurt the monsters in its radius
///    and NEVER the player who sent it back, no matter where it detonates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn whip_parry(
    mut commands: Commands,
    time: Res<Time>,
    gfx: Res<GfxAssets>,
    mut parry: ResMut<WhipParry>,
    mut q: Query<(Entity, &Transform, &mut Projectile)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    // Decay the window. Bail (cheaply) when it's closed.
    if parry.window <= 0.0 {
        return;
    }
    let was_perfect = parry.perfect > 0.0;
    parry.window -= dt;
    parry.perfect = (parry.perfect - dt).max(0.0);
    if parry.dir.length_squared() < 1e-6 {
        if parry.window <= 0.0 {
            parry.window = 0.0;
        }
        return;
    }

    let origin = parry.origin;
    let aim = parry.dir.normalize_or_zero();
    let reach = parry.reach + PARRY_REACH_PAD;
    let r2 = PARRY_CATCH_RADIUS * PARRY_CATCH_RADIUS;
    let player = parry.player;

    for (e, tf, mut p) in &mut q {
        // Only ENEMY-owned projectiles can be parried (never re-bat your own shots
        // or an already-returned projectile).
        if p.from_player {
            continue;
        }
        // Project the projectile onto the swing ray. In-arc = within `reach` along
        // the aim and within the catch radius perpendicular to it (a fat cylinder
        // hugging the visible lash, not a second hand-tuned hitbox).
        let to = tf.translation - origin;
        let along = to.dot(aim);
        if along < 0.0 || along > reach {
            continue;
        }
        let perp = (to - aim * along).length_squared();
        if perp > r2 {
            continue;
        }

        // Reflect the incoming velocity about the swing plane (normal = aim), the
        // same `v -= n*(v·n)*coeff` math the grenade wall-bounce uses, coeff = 2 for
        // a clean mirror. `into < 0` means it's actually heading toward the player
        // along the aim (the normal case for an incoming shot).
        let into = p.vel.dot(aim);
        if into >= 0.0 {
            // Degenerate: not moving toward the player along the swing — nothing
            // sensible to bat back. POP it harmlessly (no damage, no free kill).
            // Neutralise it first (player-owned, stationary) so that even if the
            // despawn command hasn't flushed before `projectile_move` runs this
            // frame it can't harm the player. `chain()` inserts a sync point so the
            // despawn normally lands before the integrator; this is belt-and-braces.
            p.from_player = true;
            p.source = player;
            p.returned = true;
            p.vel = Vec3::ZERO;
            p.fuse = f32::INFINITY; // don't let a grenade fuse-detonate
            spawn_sparks(&mut commands, &gfx, tf.translation, -aim);
            sfx.write(Sfx::at(Sound::MetallicTing, tf.translation));
            commands.entity(e).despawn();
            continue;
        }
        p.vel = parry_reflect(p.vel, aim, was_perfect);

        // FLIP OWNERSHIP enemy -> player. This is the entire safety mechanism:
        // from here every damage decision treats the projectile as the player's.
        // `returned` makes a batted grenade's splash skip the player airtight.
        p.from_player = true;
        p.source = player;
        p.returned = true;

        // Turn the tables: a returned shot at base enemy damage (a bolt is only 10)
        // can't threaten the shooter, so scale it up — modest on a late parry, hard
        // on a perfect one (the kill window). This is offence-only: the player is
        // immune to a returned shot regardless of its damage (direct hits fail
        // `hits_faction`, splash is skipped by the `returned && self_blast` guard).
        let dmg_mult = if was_perfect { PARRY_PERFECT_DAMAGE } else { PARRY_RETURN_DAMAGE };
        p.damage *= dmg_mult;
        p.splash_damage *= dmg_mult;

        // Feedback: a sharp metallic ting (pitched up on a perfect parry) + a spark.
        let pitch = if was_perfect { 1.5 } else { 1.0 };
        sfx.write(Sfx::pitched(Sound::MetallicTing, tf.translation, pitch));
        spawn_sparks(&mut commands, &gfx, tf.translation, -aim);
    }

    if parry.window <= 0.0 {
        parry.window = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bolt flying straight at the player (opposite the aim) is mirrored back out
    /// along the aim — it screams back toward the shooter, at the same speed on a
    /// non-perfect parry.
    #[test]
    fn parry_mirrors_a_head_on_bolt_back_out() {
        let aim = Vec3::Z; // player looking +Z, shooter ahead at +Z
        let incoming = Vec3::new(0.0, 0.0, -20.0); // bolt heading -Z, at the player
        let out = parry_reflect(incoming, aim, false);
        // Returned along +Z (back at the shooter), magnitude preserved.
        assert!(out.z > 0.0, "should be sent back out along +aim, got {out:?}");
        assert!((out.length() - incoming.length()).abs() < 1e-3, "speed preserved on a late parry");
        assert!(out.is_finite());
    }

    /// An off-axis bolt has its into-player component flipped to away-from-player
    /// and is nudged toward the aim (the late-parry homing). Its tangential sign is
    /// preserved (still a reflection, not a flip), but the magnitude is pulled in
    /// toward the aim, and speed is preserved.
    #[test]
    fn parry_reflects_off_axis_like_a_wall() {
        let aim = Vec3::Z;
        let incoming = Vec3::new(4.0, 0.0, -10.0); // angling in toward the player
        let out = parry_reflect(incoming, aim, false);
        // Heads back out along +aim (away from the player), never back at them.
        assert!(out.dot(aim) > 0.0, "into-component flipped outward, got {out:?}");
        // Tangential component keeps its sign but is pulled in by the homing nudge.
        assert!(out.x > 0.0 && out.x < 4.0, "tangential x reduced toward aim, got {}", out.x);
        // Speed is preserved on a late parry (homing renormalizes, no speed bonus).
        assert!((out.length() - incoming.length()).abs() < 1e-3, "speed preserved, got {}", out.length());
    }

    /// A PERFECT parry returns the shot faster than a late one and never NaNs.
    #[test]
    fn perfect_parry_is_faster_and_finite() {
        let aim = Vec3::Z;
        let incoming = Vec3::new(1.0, 0.0, -20.0);
        let late = parry_reflect(incoming, aim, false);
        let perfect = parry_reflect(incoming, aim, true);
        assert!(perfect.length() > late.length() * 1.2, "perfect should add a speed bonus");
        assert!(perfect.is_finite());
        // Homing pulls the heading toward the aim (more +Z-aligned than the mirror).
        let mirror_align = late.normalize().dot(aim);
        let perfect_align = perfect.normalize().dot(aim);
        assert!(perfect_align >= mirror_align - 1e-4, "homing should not steer away from the shooter");
    }
}
