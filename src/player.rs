//! The player: spawn, FPS mouse-look, Quake-style movement physics, cursor grab.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::post_process::bloom::Bloom;
use bevy::render::view::Msaa;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::common::{tune::*, *};
use crate::level::{apply_fog, PlayerStart};
use crate::physics::{depenetrate, move_and_slide, nearby_wall_normal};

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
        app.init_resource::<PlayerHistory>()
            .add_systems(
                Update,
                (cursor_grab, player_look, player_move)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                record_player_history
                    .after(player_move)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// A short trail of the player's recent eye positions, so monsters can aim where
/// the player *was* a moment ago instead of tracking them perfectly. Sampled
/// every frame; samples older than `MAX_AGE` are dropped.
#[derive(Resource, Default)]
pub struct PlayerHistory {
    /// (elapsed_secs, eye_pos), oldest at the front, newest at the back.
    samples: VecDeque<(f32, Vec3)>,
}
impl PlayerHistory {
    const MAX_AGE: f32 = 0.6;

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    fn record(&mut self, now: f32, eye: Vec3) {
        self.samples.push_back((now, eye));
        while let Some(&(t, _)) = self.samples.front() {
            if now - t > Self::MAX_AGE {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// The eye position `delay` seconds in the past (nearest recorded sample).
    /// `None` until any history has accumulated.
    pub fn position_ago(&self, now: f32, delay: f32) -> Option<Vec3> {
        let target = now - delay;
        let mut best: Option<(f32, Vec3)> = None;
        for &(t, p) in &self.samples {
            let d = (t - target).abs();
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, p));
            }
        }
        best.map(|(_, p)| p)
    }
}

/// Sample the player's eye position into `PlayerHistory` each frame.
fn record_player_history(
    time: Res<Time>,
    mut hist: ResMut<PlayerHistory>,
    q: Query<&Transform, With<Player>>,
) {
    let Ok(tf) = q.single() else { return };
    hist.record(time.elapsed_secs(), tf.translation + Vec3::Y * EYE_OFFSET);
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
pub fn spawn_player(
    mut commands: Commands,
    start: Res<PlayerStart>,
    style: Res<LevelStyle>,
    colliders: Res<WorldColliders>,
    mut hist: ResMut<PlayerHistory>,
) {
    hist.clear();
    let fov = 80f32.to_radians();
    let fog = apply_fog(&style);
    // Never spawn embedded in a brush: an overlapping start point freezes the
    // swept solver (see physics::depenetrate). Heal it here and warn so the map
    // author hears about it instead of shipping a soft-locked level.
    let half = Vec3::from_array(PLAYER_HALF);
    let spawn_pos = depenetrate(start.pos, half, &colliders.solids);
    if spawn_pos != start.pos {
        warn!("player spawn {:?} overlapped a solid; nudged to {:?}", start.pos, spawn_pos);
    }
    commands
        .spawn((
            Player { yaw: start.yaw, ..default() },
            Transform::from_translation(spawn_pos)
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

pub(crate) fn player_look(
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

/// Capture the mouse automatically when a level begins (startup and every
/// restart/level-advance into Playing), so the player can aim right away
/// instead of having to click first.
pub fn grab_cursor(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut c) = windows.single_mut() {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
}

pub(crate) fn player_move(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    colliders: Res<WorldColliders>,
    active: Res<crate::vehicle::ActiveVehicle>,
    mut q: Query<(&mut Transform, &mut Player, &mut Knockback, &mut crate::weapons::Grapple)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((mut tf, mut p, mut kb, mut grap)) = q.single_mut() else { return };

    // While driving a vehicle, WASD/Space belong to the truck — the player just
    // stands on the deck (carried by `vehicle_carry`) but still falls/collides.
    let driving = active.0.is_some();

    // Consume any accumulated knockback (rocket jumps, enemy hits).
    if kb.0 != Vec3::ZERO {
        p.vel += kb.0;
        kb.0 = Vec3::ZERO;
        p.on_ground = false;
    }

    // Desired direction from WASD, relative to current yaw (suppressed while driving).
    let mut wish = Vec2::ZERO;
    if !driving {
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
    }
    let (s, c) = p.yaw.sin_cos();
    let forward = Vec3::new(-s, 0.0, -c);
    let right = Vec3::new(c, 0.0, -s);
    let wishdir = (forward * wish.y + right * wish.x).normalize_or_zero();

    let mut vel = p.vel;
    let on_ground = p.on_ground;
    let half = Vec3::from_array(PLAYER_HALF);

    // A taut grapple keeps the player on "air" physics even when a low swing arc
    // grazes the floor: otherwise on_ground flips true at the bottom of the arc,
    // skipping the gravity that drives the pendulum and frictioning away the
    // swing's momentum (it would feel like the rope hit sludge near the ground).
    let grappling_taut = grap.hook.as_ref().map_or(false, |h| {
        (h.anchor - tf.translation).length() > h.len + GRAPPLE_SLACK
    });

    if on_ground {
        if !grappling_taut {
            vel = friction(vel, dt);
        }
        if !driving && keys.pressed(KeyCode::Space) {
            vel.y = JUMP_SPEED;
            p.on_ground = false;
            sfx.write(Sfx::global(Sound::Jump));
        }
    } else if !driving && keys.just_pressed(KeyCode::Space) {
        // Wall jump: a fresh jump press while airborne, if we're up against a
        // wall, kicks off it — an upward boost plus a push away from the wall.
        if let Some(n) = nearby_wall_normal(tf.translation, half, &colliders.solids, WALLJUMP_REACH) {
            vel.y = WALLJUMP_UP;
            // Cancel any motion into the wall and guarantee an outward push,
            // while preserving velocity along the wall (keeps momentum flowing).
            let outward = vel.dot(n);
            if outward < WALLJUMP_PUSH {
                vel += n * (WALLJUMP_PUSH - outward);
            }
            sfx.write(Sfx::global(Sound::Jump));
        }
    }

    // Hold Shift to run: a flat speed-up of the ground movement (no stamina).
    let run = !driving && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight));
    let run_mul = if run { RUN_MULTIPLIER } else { 1.0 };

    // Accelerate toward wishdir (ground vs air-cap gives strafe-jumping feel).
    // Run also scales AIR_ACCEL — snappier air control toward the same AIR_CAP
    // ceiling, so the air-strafe skill curve is preserved, just quicker.
    let (accel, wishspeed) = if p.on_ground && !grappling_taut {
        (GROUND_ACCEL * run_mul, MAX_GROUND_SPEED * run_mul)
    } else {
        (AIR_ACCEL * run_mul, AIR_CAP)
    };
    vel = accelerate(vel, wishdir, wishspeed, accel, dt);

    if !p.on_ground || grappling_taut {
        vel.y -= GRAVITY * dt;
        // Cap terminal velocity so a long drop is a steady plunge, not a
        // runaway accelerating blur (lets you track the world receding above).
        vel.y = vel.y.max(-TERMINAL_VELOCITY);
    }

    // --- grapnel rope: one-sided distance constraint on the swing pivot ------
    // Layered after gravity/air-accel and before the swept solver, so the
    // pendulum is driven by the velocity already built this frame while the
    // solver still owns position (the constraint only edits `vel`, never `tf`).
    if let Some(hook) = grap.hook.as_mut() {
        let center = tf.translation;
        // Reel-in: hold the reel key to climb toward the anchor. The one-sided
        // rope only ever REMOVES outward velocity, so shortening the radius alone
        // can't move you in — we inject the inward pull as velocity (which the
        // rope leaves untouched) and let the rope length track the closing
        // distance, floored at GRAPPLE_MIN_LEN so you never reel your face into
        // the anchored wall and the rope never goes slack-then-snaps.
        let to = hook.anchor - center;
        let dist = to.length();
        if keys.pressed(crate::weapons::GRAPPLE_REEL_KEY) && dist > GRAPPLE_EPS {
            let inward = to / dist;
            if dist > GRAPPLE_MIN_LEN {
                // Pull toward the anchor at up to the reel speed.
                let cur_in = vel.dot(inward);
                if cur_in < GRAPPLE_REEL_SPEED {
                    vel += inward * (GRAPPLE_REEL_SPEED - cur_in);
                }
                hook.len = (dist - GRAPPLE_REEL_SPEED * dt).max(GRAPPLE_MIN_LEN);
            } else {
                // Reached the min-length floor: arrest any residual inward drift so
                // we settle here instead of coasting on into the anchor (the
                // one-sided rope can't stop inward motion, so we must).
                let cur_in = vel.dot(inward);
                if cur_in > 0.0 {
                    vel -= inward * cur_in;
                }
            }
        }
        vel = crate::weapons::rope_constrain(center, vel, hook.anchor, hook.len);
    }

    let incoming_vy = vel.y;
    let res = move_and_slide(tf.translation, half, vel, dt, &colliders.solids, STEP_HEIGHT);
    tf.translation = res.pos;
    p.vel = res.vel;

    // Stuck watchdog: only count frames toward a detach when we're genuinely
    // pinned — a real wall contact THIS frame, the rope taut, and nearly stopped.
    // A free-air dangle or a gentle swing hits none of those (no wall), so it can
    // hang/swing indefinitely; only grinding a corner trips it. grapple_cast drops
    // the rope once the count tops GRAPPLE_STUCK_FRAMES.
    if let Some(hook) = grap.hook.as_mut() {
        let taut = (hook.anchor - res.pos).length() > hook.len + GRAPPLE_SLACK;
        if taut && res.hit_wall && res.vel.length_squared() < 1.0 {
            hook.stuck_frames += 1;
        } else {
            hook.stuck_frames = 0;
        }
    }

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
