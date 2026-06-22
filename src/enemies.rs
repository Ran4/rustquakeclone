//! Monsters: spawning, perception, the AI state machine, and attacks.

use bevy::prelude::*;

use crate::common::{tune::EYE_OFFSET, *};
use crate::effects::Lifetime;
use crate::level::{MonsterKind, SpawnPlan};
use crate::monster_model::{build_monster_visual, Dying, MonsterMats, MonsterTextures};
use crate::physics::{line_of_sight, move_and_slide, ray_aabb, raycast_world, Aabb};
use crate::player::{Player, PlayerHistory};
use crate::projectiles::{spawn_projectile, ProjKind};

/// How far in the past the Grunt aims — it shoots at where the player was this
/// long ago, so moving makes the shot lag behind you and become dodgeable.
const GRUNT_AIM_LAG: f32 = 0.090;
/// Random aim error per axis (yaw + pitch), uniform in ±this many degrees.
const GRUNT_AIM_SPREAD_DEG: f32 = 5.0;

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
    tex: Res<MonsterTextures>,
    plan: Res<SpawnPlan>,
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
    // Ambush wave from this level's plan (placed near the key/exit).
    for spawn in &plan.ambush {
        spawn_monster(&mut commands, &mut meshes, &mut materials, &tex, spawn.kind, spawn.pos);
    }
    if !plan.ambush.is_empty() {
        mission.total_enemies += plan.ambush.len() as u32;
        sfx.write(Sfx::global(Sound::Door));
        notify.write(Notify::new("The way erupts — the dimension awakens!"));
    }
}

/// Pop every body-part's emissive when the monster takes a hit (drives `Enemy.flash`).
fn enemy_hit_flash(
    time: Res<Time>,
    mut q: Query<(&mut Enemy, &MonsterMats)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (mut en, mats) in &mut q {
        if en.flash <= 0.0 {
            continue;
        }
        en.flash = (en.flash - dt * 6.0).max(0.0);
        let k = en.flash.clamp(0.0, 1.0);
        for (handle, base) in &mats.0 {
            if let Some(mut m) = materials.get_mut(handle) {
                m.emissive = LinearRgba::rgb(
                    base.red + 3.0 * k,
                    base.green + 3.0 * k,
                    base.blue + 3.0 * k,
                );
            }
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
    /// Accumulated walk-cycle phase (advanced by horizontal speed) for the rig.
    pub gait: f32,
    /// Attack animation envelope (1 at strike, decays to 0) driving the weapon arm.
    pub atk_anim: f32,
    /// Cooldown gating how often a vehicle ram can re-hit this monster (so one
    /// pass-through deals a single hit, not one per frame of overlap).
    pub ram_cd: f32,
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
        Knight => MStats { health: 60.0, speed: 7.2, sight: 28.0, attack_range: 2.2, melee_range: 2.4, cd: 0.85, damage: 16.0, half: Vec3::new(0.4, 0.9, 0.4), flying: false, color: rgb(0.55, 0.56, 0.6), emissive: LinearRgba::BLACK },
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
    tex: &MonsterTextures,
    kind: MonsterKind,
    pos: Vec3,
) {
    let s = stats(kind);
    let eye_h = s.half.y * 0.6;
    // The root is an invisible transform carrying the AI/physics; the visible,
    // articulated, textured model is built as a bone hierarchy beneath it.
    let root = commands
        .spawn((
            Transform::from_translation(pos + Vec3::Y * s.half.y),
            Visibility::Visible,
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
                gait: 0.0,
                atk_anim: 0.0,
                ram_cd: 0.0,
            },
            Health::new(s.health),
            Faction::Monster,
            Hurtbox { half: s.half },
            Knockback::default(),
            LevelEntity,
            Name::new(format!("{kind:?}")),
        ))
        .id();
    let mats = build_monster_visual(commands, meshes, materials, tex, kind, root);
    commands.entity(root).insert(MonsterMats(mats));
}

/// Spawn every monster from the level's SpawnPlan. Runs in the OnEnter chain.
pub fn spawn_monsters(
    mut commands: Commands,
    plan: Res<SpawnPlan>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<MonsterTextures>,
    mut mission: ResMut<Mission>,
) {
    mission.total_enemies = plan.monsters.len() as u32;
    for spawn in &plan.monsters {
        spawn_monster(&mut commands, &mut meshes, &mut materials, &tex, spawn.kind, spawn.pos);
    }
}

#[allow(clippy::too_many_arguments)]
fn enemy_ai(
    time: Res<Time>,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
    history: Res<PlayerHistory>,
    mut commands: Commands,
    mut rng_state: Local<u32>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Knockback), (Without<Player>, Without<Dying>)>,
    q_player: Query<(Entity, &Transform), With<Player>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let now = time.elapsed_secs();
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
        en.atk_anim = (en.atk_anim - dt * 3.5).max(0.0);
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
        // The Grunt aims where the player *was* ~250ms ago (so moving lets the
        // shot lag behind you) with a small random spread, and only connects if
        // that ray truly strikes the player — no more guaranteed hitscan.
        if en.windup > 0.0 {
            en.windup -= dt;
            if en.windup <= 0.0 {
                let aim = history.position_ago(now, GRUNT_AIM_LAG).unwrap_or(player_eye);
                resolve_grunt_shot(
                    &mut commands, &gfx, &colliders.solids, e, player_e, eye, player_pos,
                    aim, en.damage, &mut dmg, &mut sfx, &mut rand,
                );
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
            en.atk_anim = 1.0;
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

        // Advance the walk-cycle phase by how fast we're actually moving.
        let speed_h = Vec3::new(en.vel.x, 0.0, en.vel.z).length();
        en.gait += speed_h * dt * 2.4;
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

/// Resolve a Grunt's hitscan. Unlike a perfect tracker, it aims at `aim` (where
/// the player was ~250ms ago), perturbs that aim by a uniform ±4° in yaw and
/// pitch, then traces the shot: it deals damage only if the spread ray strikes
/// the player's hurtbox before any wall. The tracer is drawn along the actual
/// shot so a miss is visible whizzing past.
#[allow(clippy::too_many_arguments)]
fn resolve_grunt_shot(
    commands: &mut Commands,
    gfx: &GfxAssets,
    solids: &[Aabb],
    self_e: Entity,
    player_e: Entity,
    eye: Vec3,
    player_pos: Vec3,
    aim: Vec3,
    damage: f32,
    dmg: &mut MessageWriter<DamageEvent>,
    sfx: &mut MessageWriter<Sfx>,
    rand: &mut impl FnMut() -> f32,
) {
    const MAX_RANGE: f32 = 100.0;
    sfx.write(Sfx::at(Sound::Shotgun, eye));

    // Base direction toward the lagged aim point.
    let mut base = (aim - eye).normalize_or_zero();
    if base == Vec3::ZERO {
        base = Vec3::NEG_Z;
    }
    // Apply a uniform ±spread jitter in yaw (left/right) and pitch (up/down).
    let spread = GRUNT_AIM_SPREAD_DEG.to_radians();
    let yaw = (rand() * 2.0 - 1.0) * spread;
    let pitch = (rand() * 2.0 - 1.0) * spread;
    let right = {
        let r = base.cross(Vec3::Y).normalize_or_zero();
        if r == Vec3::ZERO { Vec3::X } else { r }
    };
    let dir = (Quat::from_axis_angle(Vec3::Y, yaw)
        * Quat::from_axis_angle(right, pitch)
        * base)
        .normalize_or_zero();

    // Nearest wall along the shot, and whether the player is struck before it.
    let wall_t = raycast_world(eye, dir, MAX_RANGE, solids).map(|(t, _, _)| t);
    let player_aabb = Aabb::from_center_half(player_pos, Vec3::from_array(tune::PLAYER_HALF));
    let hit_t = ray_aabb(eye, dir, MAX_RANGE, &player_aabb)
        .map(|(t, _)| t)
        .filter(|&t| wall_t.map_or(true, |w| t <= w));

    let end_t = hit_t.or(wall_t).unwrap_or(MAX_RANGE);
    draw_tracer(commands, gfx, eye, eye + dir * end_t);

    if hit_t.is_some() {
        dmg.write(DamageEvent { target: player_e, amount: damage, source: Some(self_e), knockback: dir * 2.0 });
    }
}
