//! Projectiles: rockets, grenades, nails, and enemy energy bolts.

use bevy::prelude::*;

use crate::common::*;
use crate::effects::spawn_particle;
use crate::monster_model::nearest_limb_hit;
use crate::physics::{ray_aabb, raycast_world, Aabb};

pub struct ProjectilePlugin;
impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, projectile_move.run_if(in_state(GameState::Playing)));
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
            expl.write(ExplosionEvent { pos, radius: p.splash_radius * 1.5, damage: p.splash_damage, source: p.source, from_player: p.from_player, color: rgb(1.0, 0.6, 0.2), push: p.push, implode: false, direct_limb: None });
            sfx.write(Sfx::at(Sound::Explosion, pos));
            exploded = true;
        }

        if exploded || p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}
