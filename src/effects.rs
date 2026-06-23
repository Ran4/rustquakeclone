//! Visual juice: particles, explosions, gibs, muzzle flashes, screen shake, bob.

use bevy::prelude::*;

use crate::common::{tune::*, *};
use crate::player::{Player, PlayerCamera};

pub struct EffectsPlugin;
impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShakeState>().add_systems(
            Update,
            (
                handle_shake,
                handle_explosion_fx,
                handle_impact_fx,
                update_particles,
                update_lifetimes,
                update_explosions,
                update_corpses,
                camera_fx,
            )
                .run_if(in_state(GameState::Playing)),
        );
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------
#[derive(Component)]
pub struct Particle {
    pub vel: Vec3,
    pub life: f32,
    pub max_life: f32,
    pub gravity: f32,
    pub drag: f32,
    pub start_scale: f32,
    pub end_scale: f32,
}

/// Generic "despawn after N seconds" marker (lights, flashes).
#[derive(Component)]
pub struct Lifetime(pub f32);

#[derive(Component)]
pub struct ExplosionFx {
    pub t: f32,
    pub max: f32,
    pub radius: f32,
}

#[derive(Resource, Default)]
pub struct ShakeState {
    pub trauma: f32,
    pub rng: u32,
}
impl ShakeState {
    fn rand(&mut self) -> f32 {
        let mut x = self.rng.wrapping_add(0x9E3779B9) | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[inline]
fn rng(seed: u32) -> f32 {
    let mut x = seed | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x as f32 / u32::MAX as f32
}

// ---------------------------------------------------------------------------
// Spawn helpers (called from weapons/combat/projectiles)
// ---------------------------------------------------------------------------
#[allow(clippy::too_many_arguments)]
pub fn spawn_particle(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    vel: Vec3,
    life: f32,
    gravity: f32,
    drag: f32,
    start_scale: f32,
    end_scale: f32,
) {
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_translation(pos).with_scale(Vec3::splat(start_scale)),
        Particle { vel, life, max_life: life, gravity, drag, start_scale, end_scale },
        LevelEntity,
    ));
}

pub fn spawn_gibs(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3, count: u32) {
    for i in 0..count {
        let s = (pos.x * 71.0 + pos.z * 137.0 + pos.y * 17.0) as i32 as u32 ^ i.wrapping_mul(2654435761);
        let dir = Vec3::new(rng(s) * 2.0 - 1.0, rng(s ^ 0xAA).abs() + 0.4, rng(s ^ 0xBB) * 2.0 - 1.0)
            .normalize_or_zero();
        let speed = 3.0 + rng(s ^ 0xCC) * 6.0;
        let mat = if i % 3 == 0 { gfx.spark.clone() } else { gfx.gib.clone() };
        spawn_particle(commands, gfx.small_sphere.clone(), mat, pos + Vec3::Y * 0.6, dir * speed, 1.3, 11.0, 0.2, 1.4, 0.0);
    }
}

pub fn spawn_blood(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3, normal: Vec3) {
    for i in 0u32..8 {
        let s = (pos.x * 91.0 + pos.z * 53.0) as i32 as u32 ^ i.wrapping_mul(40503);
        let spread = Vec3::new(rng(s) * 2.0 - 1.0, rng(s ^ 7).abs(), rng(s ^ 9) * 2.0 - 1.0);
        let dir = (normal * 0.6 + spread).normalize_or_zero();
        spawn_particle(commands, gfx.small_sphere.clone(), gfx.blood.clone(), pos, dir * (2.0 + rng(s ^ 3) * 3.0), 0.7, 9.0, 0.1, 0.9, 0.0);
    }
}

pub fn spawn_sparks(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3, normal: Vec3) {
    for i in 0u32..6 {
        let s = (pos.x * 33.0 + pos.y * 77.0) as i32 as u32 ^ i.wrapping_mul(2246822519);
        let spread = Vec3::new(rng(s) * 2.0 - 1.0, rng(s ^ 5) * 2.0 - 1.0, rng(s ^ 6) * 2.0 - 1.0);
        let dir = (normal + spread * 0.8).normalize_or_zero();
        spawn_particle(commands, gfx.small_sphere.clone(), gfx.spark.clone(), pos + normal * 0.05, dir * (3.0 + rng(s ^ 2) * 4.0), 0.35, 6.0, 0.05, 0.6, 0.0);
    }
}

/// Brief muzzle flash: an emissive blob + a quick point light, both auto-despawn.
pub fn spawn_muzzle_flash(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3) {
    commands.spawn((
        Mesh3d(gfx.sphere.clone()),
        MeshMaterial3d(gfx.muzzle.clone()),
        Transform::from_translation(pos).with_scale(Vec3::splat(0.35)),
        Lifetime(0.05),
        LevelEntity,
    ));
    commands.spawn((
        PointLight { color: rgb(1.0, 0.85, 0.5), intensity: 250_000.0, range: 8.0, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(pos),
        Lifetime(0.06),
        LevelEntity,
    ));
}

pub fn spawn_explosion_visual(commands: &mut Commands, gfx: &GfxAssets, pos: Vec3, radius: f32) {
    // Expanding fireball.
    commands.spawn((
        Mesh3d(gfx.sphere.clone()),
        MeshMaterial3d(gfx.explosion.clone()),
        Transform::from_translation(pos).with_scale(Vec3::splat(0.3)),
        ExplosionFx { t: 0.0, max: 0.45, radius },
        LevelEntity,
    ));
    // Flash light.
    commands.spawn((
        PointLight { color: rgb(1.0, 0.6, 0.25), intensity: 3_000_000.0, range: radius * 4.0, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(pos),
        Lifetime(0.18),
        LevelEntity,
    ));
    // Sparks + smoke.
    for i in 0u32..18 {
        let s = (pos.x * 19.0 + pos.z * 251.0) as i32 as u32 ^ i.wrapping_mul(2654435761);
        let dir = Vec3::new(rng(s) * 2.0 - 1.0, rng(s ^ 1).abs() + 0.2, rng(s ^ 2) * 2.0 - 1.0).normalize_or_zero();
        let (mat, life, grav, scale) = if i % 2 == 0 {
            (gfx.spark.clone(), 0.5, 7.0, 0.5)
        } else {
            (gfx.smoke.clone(), 1.1, -1.0, 1.2)
        };
        spawn_particle(commands, gfx.small_sphere.clone(), mat, pos, dir * (4.0 + rng(s ^ 3) * 7.0), life, grav, 0.4, scale, 0.0);
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------
fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut Particle)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut p) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        let damp = 1.0 - (p.drag * dt).min(1.0);
        p.vel *= damp;
        tf.translation += p.vel * dt;
        let f = (p.life / p.max_life).clamp(0.0, 1.0);
        let scale = p.end_scale + (p.start_scale - p.end_scale) * f;
        tf.scale = Vec3::splat(scale.max(0.001));
    }
}

fn update_corpses(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Transform, &mut Corpse)>) {
    let dt = time.delta_secs();
    for (e, mut tf, mut c) in &mut q {
        c.0 -= dt;
        if c.0 <= 0.0 {
            commands.entity(e).despawn();
        } else if c.0 < 1.5 {
            // sink into the floor as it fades
            tf.translation.y -= dt * 0.4;
            tf.scale.y = (tf.scale.y - dt * 0.2).max(0.02);
        }
    }
}

fn update_lifetimes(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Lifetime)>) {
    let dt = time.delta_secs();
    for (e, mut l) in &mut q {
        l.0 -= dt;
        if l.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

fn update_explosions(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut ExplosionFx)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut fx) in &mut q {
        fx.t += dt;
        let f = (fx.t / fx.max).clamp(0.0, 1.0);
        // Expand quickly then fade by shrinking.
        let grow = (f * 2.0).min(1.0);
        let fade = (1.0 - f).max(0.0);
        let scale = fx.radius * (0.3 + grow * 0.9) * (0.5 + 0.5 * fade);
        tf.scale = Vec3::splat(scale.max(0.01));
        if fx.t >= fx.max {
            commands.entity(e).despawn();
        }
    }
}

fn handle_shake(mut shake: ResMut<ShakeState>, mut reader: MessageReader<ScreenShake>) {
    for ev in reader.read() {
        shake.trauma = (shake.trauma + ev.amount).min(1.0);
    }
}

fn handle_explosion_fx(
    mut commands: Commands,
    mut reader: MessageReader<ExplosionEvent>,
    gfx: Res<GfxAssets>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut shake: MessageWriter<ScreenShake>,
) {
    let ear = cam.iter().next().map(|g| g.translation()).unwrap_or(Vec3::ZERO);
    for ex in reader.read() {
        // The Lodestone emits an implode pull event every frame for its whole fuse;
        // that's a silent gravity well, not a detonation — no fireball/flash/shake.
        // Only the real fuse-expiry pop (implode: false) gets the explosion visual.
        if ex.implode {
            continue;
        }
        spawn_explosion_visual(&mut commands, &gfx, ex.pos, ex.radius);
        let d = ex.pos.distance(ear);
        let amt = (1.0 - d / 25.0).clamp(0.0, 1.0) * 0.6;
        if amt > 0.0 {
            shake.write(ScreenShake { amount: amt });
        }
    }
}

fn handle_impact_fx(
    mut commands: Commands,
    mut reader: MessageReader<ImpactEvent>,
    gfx: Res<GfxAssets>,
) {
    for ev in reader.read() {
        if ev.blood {
            spawn_blood(&mut commands, &gfx, ev.pos, ev.normal);
        } else {
            spawn_sparks(&mut commands, &gfx, ev.pos, ev.normal);
        }
    }
}

fn camera_fx(
    time: Res<Time>,
    mut shake: ResMut<ShakeState>,
    q_player: Query<&Player>,
    mut q_cam: Query<&mut Transform, With<PlayerCamera>>,
) {
    let dt = time.delta_secs();
    let Ok(player) = q_player.single() else { return };
    let Ok(mut cam) = q_cam.single_mut() else { return };

    let speed = Vec3::new(player.vel.x, 0.0, player.vel.z).length();
    let amt = (speed / MAX_GROUND_SPEED).min(1.0) * 0.05;
    let bx = (player.bob).sin() * amt;
    let by = ((player.bob * 2.0).sin()).abs() * amt;

    let s = shake.trauma * shake.trauma;
    let (sx, sy, sz) = if s > 0.0 {
        (shake.rand() * s * 0.25, shake.rand() * s * 0.25, shake.rand() * s * 0.15)
    } else {
        (0.0, 0.0, 0.0)
    };
    shake.trauma = (shake.trauma - dt * 1.6).max(0.0);

    cam.translation = Vec3::new(bx + sx, EYE_OFFSET + by + sy, sz);
}
