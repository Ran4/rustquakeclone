//! The player: spawn, FPS mouse-look, Quake-style movement physics, cursor grab.

use bevy::prelude::*;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::post_process::bloom::Bloom;
use bevy::render::view::Msaa;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::common::{tune::*, *};
use crate::level::{apply_fog, PlayerStart};
use crate::physics::move_and_slide;

const SENS: f32 = 0.0022;

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // Movement runs in Update (per rendered frame), not FixedUpdate. The
        // camera is a child of the player root, so stepping the root at a fixed
        // 60 Hz while rendering at the monitor's (higher) refresh rate made the
        // view judder. Per-frame movement is smooth and lower-latency — and it's
        // how Quake itself ran. All the movement math is dt-scaled, so a variable
        // timestep is fine. Order: grab → look → move so aim is applied first.
        app.add_systems(
            Update,
            (cursor_grab, player_look, player_move)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

#[derive(Component)]
pub struct Player {
    pub vel: Vec3,
    pub on_ground: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub bob: f32,
}
impl Default for Player {
    fn default() -> Self {
        Self { vel: Vec3::ZERO, on_ground: false, yaw: 0.0, pitch: 0.0, bob: 0.0 }
    }
}

#[derive(Component)]
pub struct PlayerCamera;

/// Spawn the player root (carrying movement state) with a child first-person
/// camera that holds all the post-processing. Called on entering Playing.
pub fn spawn_player(mut commands: Commands, start: Res<PlayerStart>, style: Res<LevelStyle>) {
    let fov = 80f32.to_radians();
    let fog = apply_fog(&style);
    commands
        .spawn((
            Player { yaw: start.yaw, ..default() },
            Transform::from_translation(start.pos)
                .with_rotation(Quat::from_rotation_y(start.yaw)),
            Visibility::default(),
            Faction::Player,
            Health::new(100.0),
            Armor::default(),
            Knockback::default(),
            Hurtbox { half: Vec3::from_array(PLAYER_HALF) },
            LevelEntity,
            Name::new("Player"),
        ))
        .with_children(|p| {
            p.spawn((
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection { fov, near: 0.04, ..default() }),
                Hdr,
                Msaa::Sample4,
                Tonemapping::AcesFitted,
                Bloom::NATURAL,
                fog,
                Transform::from_xyz(0.0, EYE_OFFSET, 0.0),
                PlayerCamera,
            ));
        });
}

fn player_look(
    motion: Res<AccumulatedMouseMotion>,
    windows: Query<&CursorOptions, With<PrimaryWindow>>,
    mut q_root: Query<(&mut Player, &mut Transform), Without<PlayerCamera>>,
    mut q_cam: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    // Only look around while the cursor is grabbed.
    let grabbed = windows
        .single()
        .map(|c| c.grab_mode != CursorGrabMode::None)
        .unwrap_or(false);
    if !grabbed {
        return;
    }
    let d = motion.delta;
    let Ok((mut p, mut root_tf)) = q_root.single_mut() else { return };
    p.yaw -= d.x * SENS;
    p.pitch = (p.pitch - d.y * SENS).clamp(-1.54, 1.54);
    root_tf.rotation = Quat::from_rotation_y(p.yaw);
    if let Ok(mut cam_tf) = q_cam.single_mut() {
        cam_tf.rotation = Quat::from_rotation_x(p.pitch);
    }
}

fn cursor_grab(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut c) = windows.single_mut() else { return };
    if keys.just_pressed(KeyCode::Escape) {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
    if mouse.just_pressed(MouseButton::Left) {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
}

fn player_move(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    colliders: Res<WorldColliders>,
    mut q: Query<(&mut Transform, &mut Player, &mut Knockback)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((mut tf, mut p, mut kb)) = q.single_mut() else { return };

    // Consume any accumulated knockback (rocket jumps, enemy hits).
    if kb.0 != Vec3::ZERO {
        p.vel += kb.0;
        kb.0 = Vec3::ZERO;
        p.on_ground = false;
    }

    // Desired direction from WASD, relative to current yaw.
    let mut wish = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish.x -= 1.0;
    }
    let (s, c) = p.yaw.sin_cos();
    let forward = Vec3::new(-s, 0.0, -c);
    let right = Vec3::new(c, 0.0, -s);
    let wishdir = (forward * wish.y + right * wish.x).normalize_or_zero();

    let mut vel = p.vel;
    let on_ground = p.on_ground;

    if on_ground {
        vel = friction(vel, dt);
        if keys.pressed(KeyCode::Space) {
            vel.y = JUMP_SPEED;
            p.on_ground = false;
            sfx.write(Sfx::global(Sound::Jump));
        }
    }

    // Accelerate toward wishdir (ground vs air-cap gives strafe-jumping feel).
    let (accel, wishspeed) = if p.on_ground {
        (GROUND_ACCEL, MAX_GROUND_SPEED)
    } else {
        (AIR_ACCEL, AIR_CAP)
    };
    vel = accelerate(vel, wishdir, wishspeed, accel, dt);

    if !p.on_ground {
        vel.y -= GRAVITY * dt;
    }

    let incoming_vy = vel.y;
    let half = Vec3::from_array(PLAYER_HALF);
    let res = move_and_slide(tf.translation, half, vel, dt, &colliders.solids, STEP_HEIGHT);
    tf.translation = res.pos;
    p.vel = res.vel;

    let was = on_ground;
    p.on_ground = res.on_ground;
    if !was && p.on_ground && incoming_vy < -4.0 {
        sfx.write(Sfx::global(Sound::Land));
    }

    // View bob phase advances with horizontal speed while grounded.
    let hspeed = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
    if p.on_ground {
        p.bob += hspeed * dt * 1.2;
    }
}

fn friction(vel: Vec3, dt: f32) -> Vec3 {
    let horiz = Vec3::new(vel.x, 0.0, vel.z);
    let speed = horiz.length();
    if speed < 1e-4 {
        return vel;
    }
    let control = speed.max(STOP_SPEED);
    let drop = control * FRICTION * dt;
    let newspeed = (speed - drop).max(0.0) / speed;
    Vec3::new(vel.x * newspeed, vel.y, vel.z * newspeed)
}

fn accelerate(vel: Vec3, wishdir: Vec3, wishspeed: f32, accel: f32, dt: f32) -> Vec3 {
    let current = vel.dot(wishdir);
    let add = wishspeed - current;
    if add <= 0.0 {
        return vel;
    }
    let accelspeed = (accel * dt * wishspeed).min(add);
    vel + wishdir * accelspeed
}
