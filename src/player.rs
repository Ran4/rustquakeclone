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
                (cursor_grab, player_look, player_move, wallrun_audio)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                record_player_history
                    .after(player_move)
                    .run_if(in_state(GameState::Playing)),
            )
            // Stop the wall-run scuff loop on death/victory/level change (the
            // run_if(Playing) path can't fire its normal peel-off release there).
            .add_systems(OnExit(GameState::Playing), cleanup_wallrun_audio);
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
    /// Wall-run state (feature 37). `latched` is true on a frame the player is
    /// sustained-running along a wall; `normal` is the smoothed outward wall
    /// normal driving the run (kept through a short seam gap via `coyote`);
    /// `stamina` counts down while latched and refills on the ground; `speed` is
    /// the planar wall-run speed the audio scrub reads.
    pub wallrun: WallRun,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            vel: Vec3::ZERO,
            on_ground: false,
            yaw: 0.0,
            pitch: 0.0,
            bob: 0.0,
            wallrun: WallRun::default(),
        }
    }
}

/// Per-player wall-run bookkeeping (feature 37). See [`Player::wallrun`].
pub struct WallRun {
    /// True on a frame the player is actively wall-running (used by the audio scrub).
    pub latched: bool,
    /// Smoothed outward wall normal currently being run along (unit, horizontal).
    /// `None` when not latched and the coyote grace has lapsed.
    pub normal: Option<Vec3>,
    /// Remaining stamina (s); ticks down while latched, recharges GRADUALLY while
    /// grounded (after a brief dwell). A latch is refused once it hits zero until
    /// you've spent enough time back on the ground to build some back.
    pub stamina: f32,
    /// Seconds of "wall not seen but stay latched" grace left (seam coyote-time).
    pub coyote: f32,
    /// Seconds the player has held continuous grounded contact since last airborne.
    /// Gates the stamina recharge so a single floor-graze frame mid-run (or a
    /// one-frame bunny-hop touch) can't refill the run.
    pub ground_dwell: f32,
    /// Planar wall-run speed (m/s) this frame, for the audio scrub.
    pub speed: f32,
}
impl Default for WallRun {
    fn default() -> Self {
        Self {
            latched: false,
            normal: None,
            stamina: WALLRUN_STAMINA,
            coyote: 0.0,
            ground_dwell: 0.0,
            speed: 0.0,
        }
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
    mount: Res<crate::mount::ActiveMount>,
    mut q: Query<(&mut Transform, &mut Player, &mut Knockback, &mut crate::weapons::Grapple)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok((mut tf, mut p, mut kb, mut grap)) = q.single_mut() else { return };

    // While driving a vehicle OR riding a mounted Ogre, WASD/Space belong to the
    // vehicle/mount — the player just stands on the deck/saddle (carried by
    // `vehicle_carry` / `mount_carry`) but still falls/collides.
    let driving = active.0.is_some() || mount.0.is_some();

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

    // Did a fresh jump press kick us off a wall this frame? If so, the wall-run
    // band below must yield this frame — the tap wins (it's the "drop off for
    // height" decision), exactly as it did before wall-run existed.
    let mut walljumped = false;

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
        // This is the TAP; the sustained wall-run below is a separate, additive
        // band — tapping Space mid-wall-run still kicks off here.
        if let Some(n) = nearby_wall_normal(tf.translation, half, &colliders.solids, WALLJUMP_REACH) {
            vel.y = WALLJUMP_UP;
            // Cancel any motion into the wall and guarantee an outward push,
            // while preserving velocity along the wall (keeps momentum flowing).
            let outward = vel.dot(n);
            if outward < WALLJUMP_PUSH {
                vel += n * (WALLJUMP_PUSH - outward);
            }
            sfx.write(Sfx::global(Sound::Jump));
            walljumped = true;
        }
    }

    // --- Wall-run (feature 37) ----------------------------------------------
    // Latch conditions: airborne, NOT mid-wall-jump this frame, a move key held
    // that points roughly along the wall (not pulling away from it), planar speed
    // above WALLRUN_MIN_SPEED, a wall adjacent (reuse the wall-jump probe, a touch
    // wider), and stamina left. While latched we cancel most of gravity (stick to
    // the face), accelerate ALONG the wall in the direction we already travel
    // (Quake-accelerate, capped at WALLRUN_MAX_SPEED), and ease vel.y up toward a
    // small climb. On release (stamina dry, key let go, wall lost past the coyote
    // grace, or a wall-jump) we peel off keeping the momentum built — no hard stop.
    //
    // Seams: convex brush corners make the wall probe flicker its normal frame to
    // frame. We keep the latch alive through a short WALLRUN_COYOTE gap with NO
    // wall seen, and low-pass the normal toward each fresh probe, so a wall join
    // flows instead of stuttering the run off.
    let planar = Vec3::new(vel.x, 0.0, vel.z);
    let planar_speed = planar.length();
    let probe = if !driving && !p.on_ground {
        nearby_wall_normal(tf.translation, half, &colliders.solids, WALLRUN_REACH)
    } else {
        None
    };
    // Recharge stamina GRADUALLY while grounded — never instantly — so a single
    // floor-graze frame during a run (a wall that meets the floor) or a one-frame
    // bunny-hop touch between runs can't top the run back up and make it endless.
    // A brief grounded dwell must build up first; only then does stamina refill,
    // and at a finite rate, so chaining back-to-back full runs actually costs you
    // ground time. Airborne, the dwell resets so the gate re-arms each landing.
    if p.on_ground {
        p.wallrun.ground_dwell += dt;
        if p.wallrun.ground_dwell >= WALLRUN_GROUND_DWELL {
            let rate = WALLRUN_STAMINA / WALLRUN_RECHARGE;
            p.wallrun.stamina = (p.wallrun.stamina + rate * dt).min(WALLRUN_STAMINA);
        }
        p.wallrun.coyote = 0.0;
        p.wallrun.normal = None;
    } else {
        p.wallrun.ground_dwell = 0.0;
    }
    // Smooth/debounce the working wall normal across seams.
    let wall_normal = match (probe, p.wallrun.normal) {
        (Some(fresh), Some(prev)) => {
            // Low-pass toward the fresh probe; if it flipped to the opposite face
            // (a sharp convex corner) just snap to it rather than blending through
            // zero (which would briefly lose the wall).
            let blended = if fresh.dot(prev) > 0.0 {
                let k = 1.0 - (-WALLRUN_NORMAL_SMOOTH * dt).exp();
                (prev + (fresh - prev) * k).normalize_or_zero()
            } else {
                fresh
            };
            p.wallrun.coyote = WALLRUN_COYOTE;
            Some(if blended == Vec3::ZERO { fresh } else { blended })
        }
        (Some(fresh), None) => {
            p.wallrun.coyote = WALLRUN_COYOTE;
            Some(fresh)
        }
        (None, Some(prev)) => {
            // No wall this frame: ride the coyote grace on the last good normal.
            p.wallrun.coyote = (p.wallrun.coyote - dt).max(0.0);
            if p.wallrun.coyote > 0.0 { Some(prev) } else { None }
        }
        (None, None) => None,
    };

    // Does the player want to hold the wall? Require the move key to point INTO or
    // exactly ALONG the face (a non-positive dot against the outward normal), not
    // merely "not very far away". The old 0.20 slack let a wish pointing slightly
    // away from the wall still latch, which (with the speed floor) flipped tight
    // corridor dodge-hops into accidental wall-runs. `<= 0.0` keeps the canonical
    // parallel "run along it" wish (dot == 0) latching while rejecting any wish
    // with an outward component.
    let into_wall = wall_normal.map_or(false, |n| {
        wishdir != Vec3::ZERO && wishdir.dot(n) <= 0.0
    });
    let wallrunning = !walljumped
        && !p.on_ground
        && !grappling_taut
        && planar_speed >= WALLRUN_MIN_SPEED
        && p.wallrun.stamina > 0.0
        && into_wall;

    p.wallrun.latched = wallrunning;
    p.wallrun.normal = if wallrunning { wall_normal } else { None };
    p.wallrun.speed = if wallrunning { planar_speed } else { 0.0 };

    // Hold Shift to run: a flat speed-up of the ground movement (no stamina).
    let run = !driving && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight));
    let run_mul = if run { RUN_MULTIPLIER } else { 1.0 };

    if wallrunning {
        let n = wall_normal.unwrap();
        // Tick stamina down — a single latch only lasts WALLRUN_STAMINA seconds.
        p.wallrun.stamina = (p.wallrun.stamina - dt).max(0.0);
        // Project velocity onto the wall plane (drop any into-the-wall component)
        // so we glide flat along the face instead of grinding into it.
        let into = vel.dot(n);
        if into < 0.0 {
            vel -= n * into;
        }
        // Along-the-wall travel direction (horizontal tangent of current motion).
        let along = Vec3::new(vel.x, 0.0, vel.z);
        if along.length() > 1e-3 {
            let dir = along.normalize();
            // Quake-accelerate along the wall toward the sprint cap.
            vel = accelerate(vel, dir, WALLRUN_MAX_SPEED, WALLRUN_ACCEL, dt);
        }
        // Make WALLRUN_MAX_SPEED a real CEILING, not just an accelerate target:
        // accelerate() only ever ADDS toward its wishspeed, so a fast entry (a
        // Shift-run carried in at ~36 m/s) would otherwise keep all its speed and
        // turn the run into gravity-off flight. Clamp the planar (x,z) speed down
        // to the cap each frame so excess bleeds off — the run is finite and
        // earned for fast and slow entries alike. (vel.y is left to the
        // gravity/up-bias logic below.)
        let planar_now = Vec3::new(vel.x, 0.0, vel.z);
        let psp = planar_now.length();
        if psp > WALLRUN_MAX_SPEED {
            let scale = WALLRUN_MAX_SPEED / psp;
            vel.x *= scale;
            vel.z *= scale;
        }
        // Stick to the face: only a fraction of gravity survives.
        vel.y -= GRAVITY * WALLRUN_GRAVITY_FRAC * dt;
        // A gentle assisted climb: ease vel.y up toward a small ceiling.
        if vel.y < WALLRUN_UP_MAX {
            let k = 1.0 - (-WALLRUN_UP_BIAS * dt).exp();
            vel.y += (WALLRUN_UP_MAX - vel.y) * k;
        }
    } else {
        // Normal locomotion: accelerate toward wishdir (ground vs air-cap gives
        // strafe-jumping feel). Run also scales AIR_ACCEL — snappier air control
        // toward the same AIR_CAP ceiling, so the air-strafe skill curve is
        // preserved, just quicker.
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

/// The single looping wall-run scuff audio entity (present only while the player
/// is wall-running). Spawned on the first latched frame, its volume scrubbed by
/// wall-run speed, despawned the instant the player peels off — mirrors the
/// truck-engine / resonant-hum looping-`AudioSink` pattern.
#[derive(Component)]
struct WallrunScuffSound;

/// Spawn / scrub / tear down the wall-run scuff loop (feature 37). Reads the
/// latch state `player_move` stamped on `Player.wallrun` this frame: spawns the
/// loop on the first latched frame, fades its volume toward a speed-driven target
/// while latched (and toward silence once peeled off), and despawns it once the
/// player isn't wall-running and the loop has faded out — so teardown never
/// clicks and the sink never leaks. Mirrors `vehicle_audio`'s three-state
/// `AudioSink` handling (the sink goes live a frame or two after the spawn).
fn wallrun_audio(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<Sounds>,
    q_player: Query<&Player>,
    mut q_scuff: Query<(Entity, Option<&mut AudioSink>), With<WallrunScuffSound>>,
) {
    use bevy::audio::Volume;
    let dt = time.delta_secs();
    let (latched, speed) = q_player
        .single()
        .map(|p| (p.wallrun.latched, p.wallrun.speed))
        .unwrap_or((false, 0.0));

    // Target volume rises with wall-run speed between the floor and sprint cap.
    let target_vol = if latched {
        let frac = ((speed - WALLRUN_MIN_SPEED) / (WALLRUN_MAX_SPEED - WALLRUN_MIN_SPEED))
            .clamp(0.0, 1.0);
        WALLRUN_SCUFF_MIN_VOL + (WALLRUN_SCUFF_MAX_VOL - WALLRUN_SCUFF_MIN_VOL) * frac
    } else {
        0.0
    };

    match q_scuff.single_mut() {
        Ok((e, Some(mut sink))) => {
            let k = 1.0 - (-WALLRUN_SCUFF_SMOOTH * dt).exp();
            let cv = sink.volume().to_linear();
            let nv = cv + (target_vol - cv) * k;
            sink.set_volume(Volume::Linear(nv));
            // Despawn only once faded out on peel-off, so teardown doesn't click.
            if !latched && nv < 0.01 {
                commands.entity(e).despawn();
            }
        }
        Ok((e, None)) => {
            // Entity spawned but sink not live yet: if we already peeled off the
            // same frame, just tear it down (nothing audible to fade).
            if !latched {
                commands.entity(e).despawn();
            }
        }
        Err(_) => {
            if latched {
                commands.spawn((
                    AudioPlayer::new(sounds.get(Sound::WallrunScuff)),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
                    WallrunScuffSound,
                    Name::new("WallrunScuffSound"),
                ));
            }
        }
    }
}

/// Tear the scuff loop down on death/victory/level change — the `run_if(Playing)`
/// path can't fire its normal "peeled off the wall" release there.
fn cleanup_wallrun_audio(mut commands: Commands, q: Query<Entity, With<WallrunScuffSound>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
