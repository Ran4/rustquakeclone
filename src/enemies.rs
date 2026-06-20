//! Monsters: spawning, perception, the AI state machine, and attacks.

use bevy::prelude::*;

use crate::common::{tune::EYE_OFFSET, *};
use crate::effects::Lifetime;
use crate::level::{MonsterKind, SpawnPlan};
use crate::physics::{line_of_sight, move_and_slide};
use crate::player::Player;
use crate::projectiles::{spawn_projectile, ProjKind};

/// A thin emissive tracer line (Grunt shot telegraph / hit feedback).
fn draw_tracer(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3) {
    let mid = (a + b) * 0.5;
    let len = a.distance(b).max(0.05);
    let dir = (b - a).normalize_or_zero();
    let tf = Transform::from_translation(mid)
        .looking_to(dir, Vec3::Y)
        .with_scale(Vec3::new(0.03, 0.03, len));
    commands.spawn((
        Mesh3d(gfx.unit_cube.clone()),
        MeshMaterial3d(gfx.muzzle.clone()),
        tf,
        Lifetime(0.06),
        LevelEntity,
    ));
}

pub struct EnemiesPlugin;
impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, enemy_ai.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                (enemy_hit_flash, key_ambush).run_if(in_state(GameState::Playing)),
            );
    }
}

/// When the player grabs the Silver Key, the vault erupts: a reinforcement wave
/// teleports in and every monster still in the level wakes up. Classic Quake.
fn key_ambush(
    mut commands: Commands,
    mut mission: ResMut<Mission>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut sprung: Local<bool>,
    mut q: Query<&mut Enemy>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
) {
    if *sprung {
        // Reset the latch if the mission restarted (key cleared).
        if !mission.has_key {
            *sprung = false;
        }
        return;
    }
    if !mission.has_key {
        return;
    }
    *sprung = true;

    // Wake everything already placed.
    for mut en in &mut q {
        en.awake = true;
    }
    // Ambush wave near the vault exit corridor.
    let wave = [
        (MonsterKind::Knight, Vec3::new(47.0, 1.0, 1.5)),
        (MonsterKind::Knight, Vec3::new(47.0, 1.0, 2.5)),
        (MonsterKind::Ogre, Vec3::new(50.0, 1.0, 6.0)),
    ];
    for (kind, pos) in wave {
        spawn_monster(&mut commands, &mut meshes, &mut materials, kind, pos);
    }
    mission.total_enemies += wave.len() as u32;
    sfx.write(Sfx::global(Sound::Door));
    notify.write(Notify::new("The vault erupts — the dungeon awakens!"));
}

/// Pop the monster's emissive when it takes a hit (drives `Enemy.flash`).
fn enemy_hit_flash(
    time: Res<Time>,
    mut q: Query<(&mut Enemy, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (mut en, mat) in &mut q {
        if en.flash <= 0.0 {
            continue;
        }
        en.flash = (en.flash - dt * 6.0).max(0.0);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            let k = en.flash.clamp(0.0, 1.0);
            m.emissive = LinearRgba::rgb(
                en.base_emissive.red + 3.0 * k,
                en.base_emissive.green + 3.0 * k,
                en.base_emissive.blue + 3.0 * k,
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiState {
    Idle,
    Chase,
    Attack,
}

#[derive(Component)]
pub struct Enemy {
    pub kind: MonsterKind,
    pub vel: Vec3,
    pub state: AiState,
    pub attack_cd: f32,
    pub awake: bool,
    pub flying: bool,
    pub speed: f32,
    pub sight: f32,
    pub attack_range: f32,
    pub melee_range: f32,
    pub cd: f32,
    pub damage: f32,
    pub eye_h: f32,
    pub bob: f32,
    pub half: Vec3,
    /// Pain stagger: while >0 the monster flinches (can't move or attack).
    pub pain: f32,
    /// Cooldown gating how often pain can re-trigger.
    pub pain_cd: f32,
    /// Hit-flash intensity, decayed each frame to drive an emissive pop.
    pub flash: f32,
    /// Base material emissive, restored after a hit-flash.
    pub base_emissive: LinearRgba,
    /// Hitscan wind-up (Grunt telegraph): a pending shot resolves at <=0.
    pub windup: f32,
}

struct MStats {
    health: f32,
    speed: f32,
    sight: f32,
    attack_range: f32,
    melee_range: f32,
    cd: f32,
    damage: f32,
    half: Vec3,
    flying: bool,
    color: Color,
    emissive: LinearRgba,
}

fn stats(kind: MonsterKind) -> MStats {
    use MonsterKind::*;
    match kind {
        Grunt => MStats { health: 30.0, speed: 3.4, sight: 30.0, attack_range: 24.0, melee_range: 0.0, cd: 1.3, damage: 9.0, half: Vec3::new(0.4, 0.9, 0.4), flying: false, color: rgb(0.45, 0.36, 0.28), emissive: LinearRgba::BLACK },
        Enforcer => MStats { health: 55.0, speed: 3.1, sight: 32.0, attack_range: 26.0, melee_range: 0.0, cd: 1.5, damage: 10.0, half: Vec3::new(0.45, 0.95, 0.45), flying: false, color: rgb(0.32, 0.4, 0.5), emissive: LinearRgba::rgb(0.0, 0.05, 0.2) },
        Knight => MStats { health: 60.0, speed: 5.2, sight: 28.0, attack_range: 2.2, melee_range: 2.4, cd: 0.85, damage: 16.0, half: Vec3::new(0.4, 0.9, 0.4), flying: false, color: rgb(0.55, 0.56, 0.6), emissive: LinearRgba::BLACK },
        Scrag => MStats { health: 45.0, speed: 4.0, sight: 32.0, attack_range: 28.0, melee_range: 0.0, cd: 1.4, damage: 10.0, half: Vec3::new(0.5, 0.7, 0.5), flying: true, color: rgb(0.3, 0.55, 0.3), emissive: LinearRgba::rgb(0.05, 0.3, 0.05) },
        Ogre => MStats { health: 200.0, speed: 2.7, sight: 28.0, attack_range: 22.0, melee_range: 2.8, cd: 1.9, damage: 22.0, half: Vec3::new(0.6, 1.1, 0.6), flying: false, color: rgb(0.4, 0.3, 0.22), emissive: LinearRgba::BLACK },
        DeathKnight => MStats { health: 350.0, speed: 3.4, sight: 32.0, attack_range: 26.0, melee_range: 3.0, cd: 1.6, damage: 22.0, half: Vec3::new(0.6, 1.2, 0.6), flying: false, color: rgb(0.4, 0.12, 0.14), emissive: LinearRgba::rgb(0.4, 0.0, 0.05) },
    }
}

fn sight_sound(_kind: MonsterKind) -> Sound {
    Sound::EnemySight
}

/// Voice pitch per kind so the bestiary sounds distinct (big = deep).
pub fn kind_pitch(kind: MonsterKind) -> f32 {
    use MonsterKind::*;
    match kind {
        Ogre | DeathKnight => 0.7,
        Scrag => 1.45,
        Knight => 1.15,
        _ => 1.0,
    }
}

/// Spawn one monster of `kind` at `pos` (feet just above pos so it settles).
pub fn spawn_monster(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kind: MonsterKind,
    pos: Vec3,
) {
    let s = stats(kind);
    let radius = s.half.x;
    let length = (s.half.y * 2.0 - radius * 2.0).max(0.1);
    let body = meshes.add(Capsule3d::new(radius, length));
    let mat = materials.add(StandardMaterial {
        base_color: s.color,
        emissive: s.emissive,
        perceptual_roughness: 0.8,
        ..default()
    });
    let eye_mat = materials.add(StandardMaterial {
        base_color: rgb(1.0, 0.3, 0.1),
        emissive: LinearRgba::rgb(6.0, 0.6, 0.1),
        unlit: true,
        ..default()
    });
    let eye_mesh = meshes.add(Sphere::new(0.08));
    let eye_h = s.half.y * 0.6;
    commands
        .spawn((
            Mesh3d(body),
            MeshMaterial3d(mat),
            Transform::from_translation(pos + Vec3::Y * s.half.y),
            Enemy {
                kind,
                vel: Vec3::ZERO,
                state: AiState::Idle,
                attack_cd: 0.5,
                awake: false,
                flying: s.flying,
                speed: s.speed,
                sight: s.sight,
                attack_range: s.attack_range,
                melee_range: s.melee_range,
                cd: s.cd,
                damage: s.damage,
                eye_h,
                bob: 0.0,
                half: s.half,
                pain: 0.0,
                pain_cd: 0.0,
                flash: 0.0,
                base_emissive: s.emissive,
                windup: 0.0,
            },
            Health::new(s.health),
            Faction::Monster,
            Hurtbox { half: s.half },
            Knockback::default(),
            LevelEntity,
            Name::new(format!("{kind:?}")),
        ))
        .with_children(|p| {
            for sx in [-0.18f32, 0.18] {
                p.spawn((
                    Mesh3d(eye_mesh.clone()),
                    MeshMaterial3d(eye_mat.clone()),
                    Transform::from_xyz(sx, eye_h, -radius * 0.9),
                ));
            }
        });
}

/// Spawn every monster from the level's SpawnPlan. Runs in the OnEnter chain.
pub fn spawn_monsters(
    mut commands: Commands,
    plan: Res<SpawnPlan>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mission: ResMut<Mission>,
) {
    mission.total_enemies = plan.monsters.len() as u32;
    for spawn in &plan.monsters {
        spawn_monster(&mut commands, &mut meshes, &mut materials, spawn.kind, spawn.pos);
    }
}

#[allow(clippy::too_many_arguments)]
fn enemy_ai(
    time: Res<Time>,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
    mut commands: Commands,
    mut rng_state: Local<u32>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Knockback), Without<Player>>,
    q_player: Query<(Entity, &Transform), With<Player>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((player_e, player_tf)) = q_player.single() else { return };
    let player_pos = player_tf.translation;
    let player_eye = player_pos + Vec3::Y * EYE_OFFSET;

    if *rng_state == 0 {
        *rng_state = 0x9e37_79b9;
    }
    let mut rand = || {
        let mut x = *rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *rng_state = x;
        x as f32 / u32::MAX as f32
    };

    for (e, mut tf, mut en, mut kb) in &mut q {
        en.attack_cd = (en.attack_cd - dt).max(0.0);
        en.pain = (en.pain - dt).max(0.0);
        en.pain_cd = (en.pain_cd - dt).max(0.0);
        en.bob += dt * 3.0;

        let pos = tf.translation;
        let eye = pos + Vec3::Y * en.eye_h;
        let to_player = player_pos - pos;
        let dist = to_player.length();
        let los = line_of_sight(eye, player_eye, &colliders.solids);

        // Wake up when the player is seen.
        if !en.awake {
            if dist < en.sight && los {
                en.awake = true;
                en.state = AiState::Chase;
                sfx.write(Sfx::pitched(sight_sound(en.kind), pos, kind_pitch(en.kind)));
            } else {
                // idle: still apply gravity so they rest on the floor
                settle(&mut tf, &mut en, dt, &colliders.solids);
                continue;
            }
        }

        // Face the player (yaw only).
        if to_player.x.abs() + to_player.z.abs() > 0.01 {
            let yaw = to_player.x.atan2(to_player.z) + std::f32::consts::PI;
            tf.rotation = Quat::from_rotation_y(yaw);
        }

        // Consume knockback.
        if kb.0 != Vec3::ZERO {
            en.vel += kb.0;
            kb.0 = Vec3::ZERO;
        }

        // Resolve a pending Grunt hitscan once the wind-up telegraph elapses.
        if en.windup > 0.0 {
            en.windup -= dt;
            if en.windup <= 0.0 && los {
                draw_tracer(&mut commands, &gfx, eye, player_eye);
                sfx.write(Sfx::at(Sound::Shotgun, eye));
                if rand() < 0.7 {
                    let to = (player_eye - eye).normalize_or_zero();
                    dmg.write(DamageEvent { target: player_e, amount: en.damage, source: Some(e), knockback: to * 2.0 });
                }
            }
        }

        let staggered = en.pain > 0.0 || en.windup > 0.0;
        let in_attack = (dist <= en.attack_range || dist <= en.melee_range) && los;
        let needs_approach = en.melee_range > 0.0 && dist > en.melee_range * 0.9;

        // Decide desired horizontal movement (flinching/aiming monsters hold still).
        let mut wish = Vec3::ZERO;
        if staggered {
            // hold position while flinching or telegraphing a shot
        } else if !in_attack || needs_approach {
            let dir = if en.flying { to_player } else { Vec3::new(to_player.x, 0.0, to_player.z) };
            wish = dir.normalize_or_zero() * en.speed;
            en.state = AiState::Chase;
        } else {
            en.state = AiState::Attack;
            let strafe = Vec3::new(-to_player.z, 0.0, to_player.x).normalize_or_zero();
            wish = strafe * (en.speed * 0.4) * if rand() > 0.5 { 1.0 } else { -1.0 };
        }

        // Initiate an attack if able (Grunt telegraphs via a wind-up first).
        if !staggered && in_attack && en.attack_cd <= 0.0 && dist > 0.3 {
            en.attack_cd = en.cd;
            if en.kind == MonsterKind::Grunt {
                en.windup = 0.35;
                en.flash = en.flash.max(0.5); // brief "aim" glint
            } else {
                do_attack(&mut en, &mut commands, &gfx, e, player_e, eye, player_eye, dist, &mut dmg, &mut sfx, &mut rand);
            }
        }

        // Integrate movement.
        if en.flying {
            // Hover toward a point near the player, no gravity.
            let mut v = wish;
            // keep some altitude relative to player
            let target_y = player_pos.y + 1.5 + (en.bob).sin() * 0.4;
            v.y += (target_y - pos.y) * 2.0;
            en.vel = en.vel.lerp(v, (dt * 4.0).min(1.0));
            let res = move_and_slide(pos, en.half, en.vel, dt, &colliders.solids, 0.0);
            tf.translation = res.pos;
            en.vel = res.vel;
        } else {
            // ground: accelerate horizontally toward wish, apply gravity.
            let cur_h = Vec3::new(en.vel.x, 0.0, en.vel.z);
            let new_h = cur_h.lerp(wish, (dt * 8.0).min(1.0));
            en.vel.x = new_h.x;
            en.vel.z = new_h.z;
            en.vel.y -= tune::GRAVITY * dt;
            let half = en.half;
            let res = move_and_slide(pos, half, en.vel, dt, &colliders.solids, tune::STEP_HEIGHT);
            tf.translation = res.pos;
            en.vel = res.vel;
            if res.on_ground && en.vel.y < 0.0 {
                en.vel.y = 0.0;
            }
        }
    }
}

/// Idle gravity settle so sleeping monsters sit on the ground.
fn settle(tf: &mut Transform, en: &mut Enemy, dt: f32, solids: &[crate::physics::Aabb]) {
    if en.flying {
        return;
    }
    en.vel.y -= tune::GRAVITY * dt;
    let res = move_and_slide(tf.translation, en.half, Vec3::new(0.0, en.vel.y, 0.0), dt, solids, tune::STEP_HEIGHT);
    tf.translation = res.pos;
    if res.on_ground {
        en.vel.y = 0.0;
    } else {
        en.vel.y = res.vel.y;
    }
}

#[allow(clippy::too_many_arguments)]
fn do_attack(
    en: &mut Enemy,
    commands: &mut Commands,
    gfx: &GfxAssets,
    self_e: Entity,
    player_e: Entity,
    eye: Vec3,
    player_eye: Vec3,
    dist: f32,
    dmg: &mut MessageWriter<DamageEvent>,
    sfx: &mut MessageWriter<Sfx>,
    rand: &mut impl FnMut() -> f32,
) {
    use MonsterKind::*;
    let mut to = (player_eye - eye).normalize_or_zero();
    if to == Vec3::ZERO {
        to = Vec3::NEG_Z;
    }
    let muzzle = eye + to * 0.6;
    match en.kind {
        Grunt => {
            sfx.write(Sfx::at(Sound::Shotgun, eye));
            if rand() < 0.6 {
                dmg.write(DamageEvent { target: player_e, amount: en.damage, source: Some(self_e), knockback: to * 2.0 });
            }
        }
        Enforcer | Scrag => {
            sfx.write(Sfx::at(Sound::Nailgun, eye));
            spawn_projectile(commands, gfx, ProjKind::Bolt, muzzle, to, false, Some(self_e));
        }
        Ogre => {
            if dist <= en.melee_range {
                sfx.write(Sfx::at(Sound::EnemyPain, eye));
                dmg.write(DamageEvent { target: player_e, amount: en.damage, source: Some(self_e), knockback: to * 4.0 });
            } else {
                sfx.write(Sfx::at(Sound::GrenadeFire, eye));
                let lob = (to + Vec3::Y * 0.35).normalize_or_zero();
                spawn_projectile(commands, gfx, ProjKind::Grenade, muzzle, lob, false, Some(self_e));
            }
        }
        Knight => {
            sfx.write(Sfx::at(Sound::EnemyPain, eye));
            dmg.write(DamageEvent { target: player_e, amount: en.damage, source: Some(self_e), knockback: to * 3.0 });
        }
        DeathKnight => {
            if dist <= en.melee_range {
                dmg.write(DamageEvent { target: player_e, amount: en.damage, source: Some(self_e), knockback: to * 4.0 });
                sfx.write(Sfx::at(Sound::EnemyPain, eye));
            } else {
                sfx.write(Sfx::at(Sound::RocketFire, eye));
                // fan of three bolts
                for off in [-0.12f32, 0.0, 0.12] {
                    let d = (to + Vec3::new(off, 0.0, 0.0)).normalize_or_zero();
                    spawn_projectile(commands, gfx, ProjKind::Bolt, muzzle, d, false, Some(self_e));
                }
            }
        }
    }
}
