//! Projectiles: rockets, grenades, nails, and enemy energy bolts.

use bevy::prelude::*;

use crate::common::*;
use crate::effects::spawn_particle;
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

#[allow(clippy::too_many_arguments)]
fn projectile_move(
    mut commands: Commands,
    time: Res<Time>,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
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

            // Nearest target hit.
            let mut best_t = dist;
            let mut hit_target: Option<(Entity, Vec3, Vec3)> = None;
            for &(te, center, half) in &target_list {
                let f = target_fac.get(&te).copied().unwrap_or(Faction::Monster);
                if !p.hits_faction(f) {
                    continue;
                }
                let b = Aabb::from_center_half(center, half);
                if let Some((t, n)) = ray_aabb(tf.translation, dir, best_t, &b) {
                    best_t = t;
                    hit_target = Some((te, tf.translation + dir * t, n));
                }
            }
            // Nearest world hit (closer than any target hit found).
            let mut hit_world: Option<(Vec3, Vec3)> = None;
            if let Some((t, pt, n)) = raycast_world(tf.translation, dir, best_t, &colliders.solids) {
                best_t = t;
                hit_world = Some((pt, n));
                hit_target = None;
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
                    expl.write(ExplosionEvent {
                        pos,
                        radius: p.splash_radius,
                        damage: p.splash_damage,
                        source: p.source,
                        from_player: p.from_player,
                        color: rgb(1.0, 0.6, 0.2),
                        push: p.push,
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
                    dmg.write(DamageEvent { target: te, amount: p.damage, source: p.source, knockback: dir * p.push });
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
            expl.write(ExplosionEvent { pos, radius: p.splash_radius * 1.5, damage: p.splash_damage, source: p.source, from_player: p.from_player, color: rgb(1.0, 0.6, 0.2), push: p.push });
            sfx.write(Sfx::at(Sound::Explosion, pos));
            exploded = true;
        }

        if exploded || p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}
