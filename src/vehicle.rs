//! Drivable vehicles. A vehicle is a hand-built low-poly model that moves as its
//! own body and that the player (or a monster) can stand on and ride — the deck
//! is a moving solid surface, and a per-frame "carry" step slides riders along
//! with it (the friction that keeps you aboard). Stand on the driver spot and
//! press **E** to take control: W/S throttle + brake/reverse, A/D steer.
//!
//! ## How it hooks into the rest of the engine
//!
//! The whole world collides against the flat list of axis-aligned brushes in
//! [`WorldColliders`]. A vehicle reserves ONE slot in that list at build time
//! and rewrites it every frame from its transform — the exact trick `Door` uses
//! for a moving brush. So the player's swept solver, the monsters' `move_and_slide`,
//! hitscan and line-of-sight all see the truck as solid with no special-casing.
//!
//! The collider is the truck's footprint box (ground → deck top). Its top face
//! is where you stand; because the body only rotates about +Y the deck height is
//! always exact, even when the AABB grows a little for a turned truck.

use std::f32::consts::FRAC_PI_2;

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::common::{tune::{GRAVITY, PLAYER_HALF}, *};
use crate::enemies::Enemy;
use crate::physics::{move_and_slide, Aabb};
use crate::player::Player;

// ----------------------------------------------------------------------------
// Truck dimensions (local space, meters). Forward is local -Z (matching the
// player's yaw=0 forward), so the truck and player share one heading convention.
// ----------------------------------------------------------------------------
/// Half width (across the truck, local X).
const HALF_W: f32 = 1.3;
/// Half length (along the truck, local Z).
const HALF_L: f32 = 2.6;
/// Deck height: the standing surface sits this high above the truck's ground
/// point, and the collider body spans ground → deck. It is taller than the
/// player step height, so you must *jump* onto the flatbed.
const BODY_H: f32 = 1.0;
/// Where the driver stands, in local space — front-centre of the open cab.
const DRIVER_LOCAL: Vec3 = Vec3::new(0.0, BODY_H, -HALF_L + 1.1);

// -- driving feel ------------------------------------------------------------
const ACCEL: f32 = 16.0; // throttle (W)
const BRAKE: f32 = 26.0; // brake / reverse (S)
const DRAG: f32 = 4.0; // passive deceleration when coasting
const MAX_SPEED: f32 = 40.0; // forward top speed
const MAX_REVERSE: f32 = 14.0; // reverse top speed
const TURN_RATE: f32 = 1.5; // rad/s of yaw at full steering authority
const TURN_REF: f32 = 7.0; // speed (m/s) at which steering reaches full authority
const STEP: f32 = 0.4; // step-up for the truck body (cross small seams, not rails)

// -- crash physics -----------------------------------------------------------
const GRIP: f32 = 7.0; // 1/s: how fast sideways (lateral) slide bleeds off — high = tracks heading, low = drifty
const REST: f32 = 0.45; // restitution: fraction of the head-on impact speed rebounded out of a wall
const SPIN_GAIN: f32 = 0.07; // rad/s of yaw spin imparted per (m/s of impact × off-axis factor)
const SPIN_DECAY: f32 = 2.5; // 1/s: how fast a collision spin winds down
const MAX_SPIN: f32 = 3.5; // rad/s cap on collision spin
const BONK_SPEED: f32 = 8.0; // impact speed (m/s) above which a crash plays a sound

// -- ramming monsters --------------------------------------------------------
const RAM_MIN_SPEED: f32 = 5.0; // min truck speed (m/s) to deal a ram hit
const RAM_CD: f32 = 0.5; // seconds before the same monster can be rammed again

// -- engine + tyre audio -----------------------------------------------------
// The engine is a single looping sample whose playback speed (pitch) and volume
// the driver scrubs from idle to redline by "RPM" (forward speed + throttle), so
// one clip covers idle burble → revving "vroom". The tyre screech is a second
// loop kept silent until lateral slip (a hard cornering/crash skid) fades it in.
const ENGINE_IDLE_PITCH: f32 = 0.55; // playback speed at idle (0 RPM)
const ENGINE_MAX_PITCH: f32 = 1.95; // playback speed at redline (full RPM)
const ENGINE_IDLE_VOL: f32 = 0.22; // volume at idle
const ENGINE_MAX_VOL: f32 = 0.6; // volume at redline
const ENGINE_THROTTLE_REV: f32 = 0.4; // RPM bump while throttle is held (rev in place)
const ENGINE_SMOOTH: f32 = 6.0; // 1/s: how fast engine pitch/volume chase the target
const TIRE_MAX_VOL: f32 = 0.6; // screech volume at full slip
const TIRE_SLIP_ON: f32 = 3.0; // lateral slip (m/s) where the screech starts
const TIRE_SLIP_FULL: f32 = 11.0; // lateral slip (m/s) at full screech volume
const TIRE_MIN_SPEED: f32 = 4.0; // need to be rolling this fast to screech at all
const TIRE_ATTACK: f32 = 14.0; // 1/s: fast screech fade-in
const TIRE_RELEASE: f32 = 6.0; // 1/s: slower screech fade-out

/// Which vehicle the player is currently driving (`None` = on foot). A resource
/// rather than a marker component so `player_move` and `vehicle_drive` both see
/// the change the same frame, with no command-buffer latency.
#[derive(Resource, Default)]
pub struct ActiveVehicle(pub Option<Entity>);

/// The single looping engine-note audio entity (only present while driving). Its
/// [`AudioSink`] speed/volume are scrubbed each frame to track engine RPM.
#[derive(Component)]
struct EngineSound;

/// The single looping tyre-screech audio entity (only present while driving).
/// Kept silent and faded in by lateral slip when cornering/crashing hard.
#[derive(Component)]
struct TireSound;

/// A drivable vehicle. The entity's `Transform.translation` is its ground point
/// (centre of the footprint, at floor level); `yaw` is its heading.
#[derive(Component)]
pub struct Vehicle {
    pub yaw: f32,
    /// Full world velocity (x/z = planar drift + rebound, y = gravity/settling).
    /// A real vector — not a forward-only scalar — so a wall can bounce it any
    /// direction and the truck keeps that momentum until grip realigns it.
    pub vel: Vec3,
    /// Angular velocity about +Y (rad/s): collision-induced spin that decays.
    pub yaw_rate: f32,
    /// Index of this vehicle's slot in `WorldColliders.solids`.
    pub collider: usize,
    /// World translation applied this frame, consumed by the rider carry.
    pub last_delta: Vec3,
    /// Yaw change applied this frame, consumed by the rider carry.
    pub last_dyaw: f32,
}

pub struct VehiclePlugin;
impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveVehicle>()
            .add_systems(
                Update,
                (vehicle_activate, vehicle_drive, vehicle_carry, vehicle_ram, vehicle_audio)
                    .chain()
                    // After look so the carry's yaw add lands on the freshly
                    // mouse-updated heading; before move so the player's own
                    // collision resolves on the already-carried position. Ram +
                    // audio trail the drive so they read the freshly integrated
                    // velocity.
                    .after(crate::player::player_look)
                    .before(crate::player::player_move)
                    .run_if(in_state(GameState::Playing)),
            )
            // The driven vehicle is a LevelEntity (despawned on rebuild); drop the
            // dangling handle so a fresh level never starts in a phantom "driving",
            // and silence the looping engine/tyre sounds on death/victory/exit.
            .add_systems(OnExit(GameState::Playing), cleanup_vehicle_audio);
    }
}

// ----------------------------------------------------------------------------
// Geometry helpers
// ----------------------------------------------------------------------------
fn degenerate() -> Aabb {
    Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

/// World-space half-extents (x, z) of the footprint rotated by `yaw`. The solver
/// is axis-aligned, so a turned truck gets the bounding box of its rotated
/// footprint — exact at right angles, a little generous on the diagonal.
fn footprint_half(yaw: f32) -> Vec2 {
    let (s, c) = yaw.sin_cos();
    Vec2::new(HALF_W * c.abs() + HALF_L * s.abs(), HALF_W * s.abs() + HALF_L * c.abs())
}

/// The footprint collider (ground → deck top) for a truck at `t`/`yaw`.
fn footprint_aabb(t: Vec3, yaw: f32) -> Aabb {
    let h = footprint_half(yaw);
    let center = Vec3::new(t.x, t.y + BODY_H * 0.5, t.z);
    Aabb::from_center_half(center, Vec3::new(h.x, BODY_H * 0.5, h.y))
}

/// If `rider` is resting on the deck of the truck at `t`/`yaw`, return where the
/// truck's motion this frame (`delta` translation + `dyaw` rotation about its
/// centre) carries it to. `None` if the rider isn't on the deck.
fn ride(rider: Vec3, half_y: f32, t: Vec3, yaw: f32, delta: Vec3, dyaw: f32) -> Option<Vec3> {
    // Project the rider into the truck's local frame to test the footprint.
    let (s, c) = yaw.sin_cos();
    let (rx, rz) = (rider.x - t.x, rider.z - t.z);
    let lx = rx * c - rz * s;
    let lz = rx * s + rz * c;
    if lx.abs() > HALF_W + 0.4 || lz.abs() > HALF_L + 0.4 {
        return None;
    }
    let deck_top = t.y + BODY_H;
    let feet = rider.y - half_y;
    if feet < deck_top - 0.3 || feet > deck_top + 0.85 {
        return None;
    }
    // Rigid-body carry: rotate about the truck's old centre by dyaw, then translate.
    let (ocx, ocz) = (t.x - delta.x, t.z - delta.z);
    let (r0x, r0z) = (rider.x - ocx, rider.z - ocz);
    let (sd, cd) = dyaw.sin_cos();
    let nx = ocx + (r0x * cd + r0z * sd) + delta.x;
    let nz = ocz + (-r0x * sd + r0z * cd) + delta.z;
    Some(Vec3::new(nx, rider.y + delta.y, nz))
}

/// Whether a monster at `enemy` (centre) with half-extents `ehalf` is overlapping
/// the truck *body* at `t`/`yaw` — i.e. the truck is driving into it. Same local-
/// frame footprint test as [`ride`], grown by the monster's plan radius, plus a
/// vertical band so we hit grounded/low monsters with the body but not flyers
/// hovering well above the deck.
fn rammed(enemy: Vec3, ehalf: Vec3, t: Vec3, yaw: f32) -> bool {
    let (s, c) = yaw.sin_cos();
    let (rx, rz) = (enemy.x - t.x, enemy.z - t.z);
    let lx = rx * c - rz * s;
    let lz = rx * s + rz * c;
    let r = ehalf.x.max(ehalf.z);
    if lx.abs() > HALF_W + r || lz.abs() > HALF_L + r {
        return false;
    }
    let elow = enemy.y - ehalf.y;
    let ehigh = enemy.y + ehalf.y;
    elow < t.y + BODY_H + 0.5 && ehigh > t.y - 0.3
}

fn coast(speed: f32, dt: f32) -> f32 {
    let d = DRAG * dt;
    if speed > 0.0 { (speed - d).max(0.0) } else { (speed + d).min(0.0) }
}

/// Horizontal unit forward vector for a heading (local -Z, matching the player).
fn forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// Crash response off a wall. `drive_planar` is the truck's intended planar
/// velocity this frame, `fwd` its heading, `slid_planar` the tangential leftover
/// the swept solver kept after clipping the into-wall component, and `wall_normal`
/// the (un-normalized) summed outward normal the solver reports. Returns the
/// post-crash planar velocity (slide + restitution rebound), the yaw-spin impulse
/// to fold into the angular velocity, and the head-on impact speed (0 = no real
/// collision, e.g. already moving away from the wall).
fn crash_response(drive_planar: Vec3, fwd: Vec3, slid_planar: Vec3, wall_normal: Vec3) -> (Vec3, f32, f32) {
    let n = Vec3::new(wall_normal.x, 0.0, wall_normal.z);
    if n.length_squared() <= 1e-6 {
        return (slid_planar, 0.0, 0.0);
    }
    let n = n.normalize();
    let into = drive_planar.dot(n); // < 0 when driving into the wall
    if into >= 0.0 {
        return (slid_planar, 0.0, 0.0);
    }
    let impact = -into;
    // Restitution: rebound back out along the wall normal.
    let out_planar = slid_planar + n * (impact * REST);
    // Off-axis hit spins the truck toward the way it scrapes along the wall — the
    // cross sign rotates the heading into the slide direction.
    let slide_dir = (drive_planar - n * into).normalize_or_zero();
    let dyaw_rate = SPIN_GAIN * impact * fwd.cross(slide_dir).y;
    (out_planar, dyaw_rate, impact)
}

// ----------------------------------------------------------------------------
// Spawning the truck model
// ----------------------------------------------------------------------------
/// Spawn a truck (model + reserved collider slot) at `pos` facing `yaw`, and
/// return its entity. Reserves one slot in `colliders` (the moving footprint
/// brush) — call this during level build so the index stays stable.
pub fn spawn_truck(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    colliders: &mut Vec<Aabb>,
    pos: Vec3,
    yaw: f32,
) -> Entity {
    // Lift a hair off the ground so the body settles with the swept solver's
    // SKIN gap above the floor. Resting a box *exactly* on a floor puts its
    // centre on the floor's Minkowski boundary, where `segment_aabb` degenerates
    // (t=0, zero normal) and the truck would be frozen in place. Falling the
    // last few millimetres lets the solver leave the gap that keeps it drivable.
    let pos = pos + Vec3::Y * 0.05;
    let collider = colliders.len();
    colliders.push(footprint_aabb(pos, yaw));

    // Materials: a rugged safety-orange work truck so it reads as a vehicle.
    let body = materials.add(StandardMaterial {
        base_color: rgb(0.62, 0.42, 0.16),
        perceptual_roughness: 0.5,
        metallic: 0.4,
        emissive: LinearRgba::rgb(0.04, 0.02, 0.005),
        ..default()
    });
    let dark = materials.add(StandardMaterial {
        base_color: rgb(0.16, 0.17, 0.2),
        perceptual_roughness: 0.7,
        metallic: 0.55,
        ..default()
    });
    let wood = materials.add(StandardMaterial {
        base_color: rgb(0.33, 0.25, 0.16),
        perceptual_roughness: 0.9,
        ..default()
    });
    let rubber = materials.add(StandardMaterial {
        base_color: rgb(0.05, 0.05, 0.06),
        perceptual_roughness: 0.95,
        ..default()
    });
    let light = materials.add(StandardMaterial {
        base_color: rgb(1.0, 0.96, 0.75),
        emissive: LinearRgba::rgb(5.0, 4.6, 2.6),
        ..default()
    });

    // Parts (baked-size meshes so transforms are just translate+rotate).
    let cube = |m: &mut Assets<Mesh>, sz: Vec3| m.add(Cuboid::from_size(sz));
    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> = vec![
        // chassis frame (under the deck), then the two top plates you stand on
        (cube(meshes, Vec3::new(2.5, 0.45, 4.9)), dark.clone(), Transform::from_xyz(0.0, 0.62, 0.0)),
        (cube(meshes, Vec3::new(2.5, 0.16, 3.3)), wood.clone(), Transform::from_xyz(0.0, 0.92, 0.95)), // flatbed
        (cube(meshes, Vec3::new(2.5, 0.16, 1.9)), dark.clone(), Transform::from_xyz(0.0, 0.92, -1.65)), // cab floor
        // open cab: front grille wall, two low side walls, a tall headboard behind
        (cube(meshes, Vec3::new(2.5, 0.85, 0.2)), body.clone(), Transform::from_xyz(0.0, 1.42, -2.5)),
        (cube(meshes, Vec3::new(0.16, 0.6, 1.7)), body.clone(), Transform::from_xyz(-1.17, 1.3, -1.55)),
        (cube(meshes, Vec3::new(0.16, 0.6, 1.7)), body.clone(), Transform::from_xyz(1.17, 1.3, -1.55)),
        (cube(meshes, Vec3::new(2.5, 1.05, 0.2)), body.clone(), Transform::from_xyz(0.0, 1.52, -0.7)),
        // headlights
        (cube(meshes, Vec3::new(0.28, 0.2, 0.08)), light.clone(), Transform::from_xyz(-0.85, 0.7, -2.58)),
        (cube(meshes, Vec3::new(0.28, 0.2, 0.08)), light.clone(), Transform::from_xyz(0.85, 0.7, -2.58)),
    ];
    // Wheels: cylinders laid on their side (axis along X).
    let wheel = meshes.add(Cylinder { radius: 0.45, half_height: 0.15 });
    for (x, z) in [(-1.3, -1.75), (1.3, -1.75), (-1.3, 1.75), (1.3, 1.75)] {
        parts.push((
            wheel.clone(),
            rubber.clone(),
            Transform { translation: Vec3::new(x, 0.45, z), rotation: Quat::from_rotation_z(FRAC_PI_2), scale: Vec3::ONE },
        ));
    }

    commands
        .spawn((
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            Vehicle { yaw, vel: Vec3::ZERO, yaw_rate: 0.0, collider, last_delta: Vec3::ZERO, last_dyaw: 0.0 },
            LevelEntity,
            Name::new("Truck"),
        ))
        .with_children(|c| {
            for (mesh, mat, tf) in &parts {
                c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *tf));
            }
        })
        .id()
}

// ----------------------------------------------------------------------------
// Systems
// ----------------------------------------------------------------------------
/// Press **E**: mount the vehicle whose driver spot you're standing on, or
/// dismount the one you're driving.
pub(crate) fn vehicle_activate(
    keys: Res<ButtonInput<KeyCode>>,
    mut active: ResMut<ActiveVehicle>,
    mount: Res<crate::mount::ActiveMount>,
    q_player: Query<&Transform, With<Player>>,
    q_veh: Query<(Entity, &Transform, &Vehicle)>,
    mut notify: MessageWriter<Notify>,
    mut sfx: MessageWriter<Sfx>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    if active.0.is_some() {
        active.0 = None;
        notify.write(Notify::new("Left the truck"));
        return;
    }
    // Don't board a truck if this same E press just mounted (or is keeping us on) an
    // Ogre. `mount_activate` runs before us (explicit ordering in MountPlugin), so a
    // mount that became active this frame is already visible here — a single tap can
    // never both mount an Ogre and board the truck.
    if mount.0.is_some() {
        return;
    }
    let Ok(ptf) = q_player.single() else { return };
    for (e, vtf, v) in &q_veh {
        let on_deck = ride(ptf.translation, PLAYER_HALF[1], vtf.translation, v.yaw, Vec3::ZERO, 0.0).is_some();
        let driver = vtf.translation + Quat::from_rotation_y(v.yaw) * DRIVER_LOCAL;
        let near = (ptf.translation.x - driver.x).hypot(ptf.translation.z - driver.z) < 2.4;
        if on_deck && near {
            active.0 = Some(e);
            notify.write(Notify::new("Driving — W/S throttle, A/D steer, E to exit"));
            sfx.write(Sfx::at(Sound::EngineStart, vtf.translation));
            break;
        }
    }
}

/// Integrate every vehicle's motion and rewrite its collider slot. Only the one
/// in [`ActiveVehicle`] reads WASD; the rest coast to a stop and settle.
fn vehicle_drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveVehicle>,
    mut colliders: ResMut<WorldColliders>,
    mut q: Query<(Entity, &mut Transform, &mut Vehicle)>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (e, mut tf, mut v) in &mut q {
        let driving = active.0 == Some(e);

        // --- driving in the heading frame ----------------------------------
        // Split the planar velocity into forward (along heading) and lateral
        // (sideways) parts. Throttle/brake/drag act on the forward part; the
        // lateral part is the slide/drift, which tire grip bleeds away so the
        // truck normally tracks where it points — but a fresh crash impulse
        // survives a moment as a recoil/skid before grip pulls it back in line.
        let fwd0 = forward(v.yaw);
        let planar0 = Vec3::new(v.vel.x, 0.0, v.vel.z);
        let mut fs = planar0.dot(fwd0);
        let lateral = (planar0 - fwd0 * fs) * (-GRIP * dt).exp();

        let mut yaw = v.yaw;
        if driving {
            let mut throttle = false;
            if keys.pressed(KeyCode::KeyW) {
                fs += ACCEL * dt;
                throttle = true;
            }
            if keys.pressed(KeyCode::KeyS) {
                fs -= BRAKE * dt;
                throttle = true;
            }
            if !throttle {
                fs = coast(fs, dt);
            }
            // Steering authority scales with (signed) speed: no turning while
            // stopped, and reversing flips the turn direction like a real car.
            let mut steer = 0.0;
            if keys.pressed(KeyCode::KeyA) {
                steer += 1.0;
            }
            if keys.pressed(KeyCode::KeyD) {
                steer -= 1.0;
            }
            yaw += steer * TURN_RATE * dt * (fs / TURN_REF).clamp(-1.0, 1.0);
        } else {
            fs = coast(fs, dt);
        }
        fs = fs.clamp(-MAX_REVERSE, MAX_SPEED);

        // Integrate the collision spin onto the heading, then wind it down.
        yaw += v.yaw_rate * dt;
        let mut yaw_rate = v.yaw_rate * (-SPIN_DECAY * dt).exp();

        // Recompose the world velocity around the (now steered + spun) heading:
        // forward thrust along the new heading plus whatever lateral slide grip
        // left behind. Gravity rides in y.
        let fwd = forward(yaw);
        let drive_planar = fwd * fs + lateral;
        let vy = v.vel.y - GRAVITY * dt;
        let vel = Vec3::new(drive_planar.x, vy, drive_planar.z);

        // --- sweep the footprint (excluding our own slot) ------------------
        let old_t = tf.translation;
        let h = footprint_half(yaw);
        let half = Vec3::new(h.x, BODY_H * 0.5, h.y);
        let center = Vec3::new(old_t.x, old_t.y + BODY_H * 0.5, old_t.z);
        if let Some(slot) = colliders.solids.get_mut(v.collider) {
            *slot = degenerate();
        }
        let res = move_and_slide(center, half, vel, dt, &colliders.solids, STEP);
        let new_t = Vec3::new(res.pos.x, res.pos.y - BODY_H * 0.5, res.pos.z);

        // The slide already stripped the into-wall component, leaving the
        // tangential slide. Add the rebound + spin of a crash on top.
        let slid_planar = Vec3::new(res.vel.x, 0.0, res.vel.z);
        let (out_planar, dyaw_rate, impact) = if res.hit_wall {
            crash_response(drive_planar, fwd, slid_planar, res.wall_normal)
        } else {
            (slid_planar, 0.0, 0.0)
        };
        yaw_rate = (yaw_rate + dyaw_rate).clamp(-MAX_SPIN, MAX_SPIN);
        if impact > BONK_SPEED {
            // Louder + a touch deeper the harder the clang (capped at full blast).
            let over = (impact - BONK_SPEED) / 24.0;
            sfx.write(Sfx {
                sound: Sound::Crash,
                pos: Some(new_t),
                volume: (0.45 + over).clamp(0.45, 1.0),
                pitch: (1.05 - over * 0.3).clamp(0.8, 1.05),
            });
        }

        let out_vy = if res.on_ground { 0.0 } else { res.vel.y };

        tf.translation = new_t;
        tf.rotation = Quat::from_rotation_y(yaw);
        v.last_delta = new_t - old_t;
        v.last_dyaw = yaw - v.yaw;
        v.yaw = yaw;
        v.vel = Vec3::new(out_planar.x, out_vy, out_planar.z);
        v.yaw_rate = yaw_rate;

        if let Some(slot) = colliders.solids.get_mut(v.collider) {
            *slot = footprint_aabb(new_t, yaw);
        }
    }
}

/// Carry everything resting on a deck along with the truck's motion this frame —
/// the "friction" that keeps the player (and monsters) aboard. Runs after the
/// truck has moved and before the player resolves its own collision.
#[allow(clippy::type_complexity)]
fn vehicle_carry(
    q_veh: Query<(&Transform, &Vehicle), (With<Vehicle>, Without<Player>, Without<Enemy>)>,
    mut q_player: Query<(&mut Transform, &mut Player), (With<Player>, Without<Enemy>, Without<Vehicle>)>,
    mut q_enemy: Query<(&mut Transform, &Enemy), (With<Enemy>, Without<Player>, Without<Vehicle>)>,
) {
    for (vtf, v) in &q_veh {
        if v.last_delta.length_squared() < 1e-10 && v.last_dyaw.abs() < 1e-6 {
            continue;
        }
        let t = vtf.translation;
        if let Ok((mut ptf, mut p)) = q_player.single_mut() {
            if let Some(np) = ride(ptf.translation, PLAYER_HALF[1], t, v.yaw, v.last_delta, v.last_dyaw) {
                ptf.translation = np;
                // Turn the player's view with the deck so facing stays consistent.
                p.yaw += v.last_dyaw;
                ptf.rotation = Quat::from_rotation_y(p.yaw);
            }
        }
        for (mut etf, en) in &mut q_enemy {
            if let Some(np) = ride(etf.translation, en.half.y, t, v.yaw, v.last_delta, v.last_dyaw) {
                etf.translation = np;
            }
        }
    }
}

/// Ram monsters with a moving truck: a head-on impact at speed deals damage and
/// a meaty knockback (and plays a thud), so you can plough a Knight off the deck
/// or pulp a Grunt under the wheels. A per-monster cooldown makes one pass-through
/// a single hit rather than one hit per overlapping frame.
fn vehicle_ram(
    time: Res<Time>,
    q_veh: Query<(&Transform, &Vehicle)>,
    mut q_enemy: Query<(Entity, &Transform, &mut Enemy, &Health), Without<Vehicle>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    // Recover ram cooldowns first so a monster becomes rammable again after RAM_CD.
    for (_, _, mut en, _) in &mut q_enemy {
        if en.ram_cd > 0.0 {
            en.ram_cd = (en.ram_cd - dt).max(0.0);
        }
    }
    for (vtf, v) in &q_veh {
        let planar = Vec3::new(v.vel.x, 0.0, v.vel.z);
        let speed = planar.length();
        if speed < RAM_MIN_SPEED {
            continue;
        }
        let dir = planar / speed;
        let t = vtf.translation;
        for (e, etf, mut en, hp) in &mut q_enemy {
            if hp.dead || en.ram_cd > 0.0 || !rammed(etf.translation, en.half, t, v.yaw) {
                continue;
            }
            en.ram_cd = RAM_CD;
            let amount = (speed * 2.0).clamp(15.0, 90.0);
            let knockback = dir * (speed * 0.6).clamp(7.0, 24.0) + Vec3::Y * 5.0;
            dmg.write(DamageEvent::body(e, amount, None, knockback));
            sfx.write(Sfx::at(Sound::RamHit, etf.translation));
        }
    }
}

/// Drive the engine + tyre loops for the truck you're in. Spawns the two looping
/// audio entities on first frame of driving and despawns them when you step out;
/// while driving it scrubs the engine's pitch/volume by RPM and fades the tyre
/// screech in/out by lateral slip. Engine RPM is forward speed plus a bump while
/// the throttle is held (so it revs even stalled against a wall).
fn vehicle_audio(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveVehicle>,
    sounds: Res<Sounds>,
    q_veh: Query<&Vehicle>,
    // `AudioSink` is inserted a frame or two after the entity spawns, so make it
    // optional: `Err` = no sound entity yet (spawn one), `Ok(None)` = spawned but
    // the sink isn't live yet (wait), `Ok(Some)` = live (drive it).
    mut q_engine: Query<Option<&mut AudioSink>, (With<EngineSound>, Without<TireSound>)>,
    mut q_tire: Query<Option<&mut AudioSink>, (With<TireSound>, Without<EngineSound>)>,
    loops: Query<Entity, Or<(With<EngineSound>, With<TireSound>)>>,
) {
    let dt = time.delta_secs();
    let Some(ve) = active.0 else {
        // Not driving: stop both loops.
        for e in &loops {
            commands.entity(e).despawn();
        }
        return;
    };
    let Ok(v) = q_veh.get(ve) else { return };

    // --- engine RPM from forward speed (+ throttle bump) ---------------------
    let fwd = forward(v.yaw);
    let planar = Vec3::new(v.vel.x, 0.0, v.vel.z);
    let fs = planar.dot(fwd);
    let throttle = keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::KeyS);
    let mut rpm = (fs.abs() / MAX_SPEED).clamp(0.0, 1.0);
    if throttle {
        rpm = (rpm + ENGINE_THROTTLE_REV).min(1.0);
    }
    let target_pitch = ENGINE_IDLE_PITCH + (ENGINE_MAX_PITCH - ENGINE_IDLE_PITCH) * rpm;
    let target_vol = ENGINE_IDLE_VOL + (ENGINE_MAX_VOL - ENGINE_IDLE_VOL) * rpm;

    match q_engine.single_mut() {
        Ok(Some(mut sink)) => {
            let k = 1.0 - (-ENGINE_SMOOTH * dt).exp();
            let p = sink.speed();
            sink.set_speed(p + (target_pitch - p) * k);
            let cv = sink.volume().to_linear();
            sink.set_volume(Volume::Linear(cv + (target_vol - cv) * k));
        }
        Ok(None) => {} // entity spawned, sink not live yet
        Err(_) => {
            commands.spawn((
                AudioPlayer::new(sounds.get(Sound::EngineLoop)),
                PlaybackSettings::LOOP
                    .with_volume(Volume::Linear(ENGINE_IDLE_VOL))
                    .with_speed(ENGINE_IDLE_PITCH),
                EngineSound,
                Name::new("EngineSound"),
            ));
        }
    }

    // --- tyre screech from lateral slip -------------------------------------
    let lateral = (planar - fwd * fs).length();
    let tire_target = if planar.length() > TIRE_MIN_SPEED {
        ((lateral - TIRE_SLIP_ON) / (TIRE_SLIP_FULL - TIRE_SLIP_ON)).clamp(0.0, 1.0) * TIRE_MAX_VOL
    } else {
        0.0
    };
    match q_tire.single_mut() {
        Ok(Some(mut sink)) => {
            let cv = sink.volume().to_linear();
            let rate = if tire_target > cv { TIRE_ATTACK } else { TIRE_RELEASE };
            let k = 1.0 - (-rate * dt).exp();
            sink.set_volume(Volume::Linear(cv + (tire_target - cv) * k));
        }
        Ok(None) => {} // entity spawned, sink not live yet
        Err(_) => {
            commands.spawn((
                AudioPlayer::new(sounds.get(Sound::TireScreech)),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
                TireSound,
                Name::new("TireSound"),
            ));
        }
    }
}

/// On leaving `Playing` (death/victory/level change): clear the driving handle so
/// a fresh level never starts in a phantom truck, and despawn the looping engine/
/// tyre sounds so they don't drone on over the death/victory screen.
fn cleanup_vehicle_audio(
    mut active: ResMut<ActiveVehicle>,
    mut commands: Commands,
    loops: Query<Entity, Or<(With<EngineSound>, With<TireSound>)>>,
) {
    active.0 = None;
    for e in &loops {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The swept solver reports an OUTWARD wall normal (points from the wall back
    // toward the truck). A front wall the truck drives -Z into therefore reports
    // +Z; a wall on the truck's right (+X) reports -X.

    /// A head-on crash rebounds the truck back out of the wall and barely spins it.
    #[test]
    fn head_on_bounces_back_without_spinning() {
        let fwd = forward(0.0); // (0,0,-1)
        let drive = fwd * 20.0; // driving straight into a front wall
        let wall_n = Vec3::Z; // front wall's outward normal
        let slid = Vec3::ZERO; // solver clipped all forward motion
        let (out, dyaw, impact) = crash_response(drive, fwd, slid, wall_n);
        assert!((impact - 20.0).abs() < 1e-3, "impact = closing speed");
        assert!(out.z > 0.0, "velocity must rebound back out of the wall (+Z), got {out:?}");
        assert!((out.z - 20.0 * REST).abs() < 1e-3, "rebound = impact * restitution");
        assert!(dyaw.abs() < 1e-3, "a square head-on hit has no preferred spin, got {dyaw}");
    }

    /// A glancing hit spins the truck toward the direction it scrapes along the
    /// wall: nosing forward-and-right into a front wall turns it right (yaw < 0).
    #[test]
    fn glancing_hit_spins_toward_the_slide() {
        let fwd = forward(0.0); // (0,0,-1)
        let drive = Vec3::new(4.0, 0.0, -12.0); // forward and to the right (+X)
        let wall_n = Vec3::Z; // front wall
        let slid = Vec3::new(4.0, 0.0, 0.0); // solver keeps the +X tangential slide
        let (out, dyaw, impact) = crash_response(drive, fwd, slid, wall_n);
        assert!(impact > 0.0);
        assert!(out.x > 0.0, "keeps sliding to the right along the wall");
        assert!(dyaw < 0.0, "right-ward scrape turns the nose right (yaw decreases), got {dyaw}");
    }

    /// Driving parallel to / away from a wall the solver still flags is not a
    /// crash: no rebound, no spin.
    #[test]
    fn no_response_when_not_driving_into_the_wall() {
        let fwd = forward(0.0);
        let drive = Vec3::new(0.0, 0.0, -10.0); // moving along the wall, not into it
        let wall_n = Vec3::X; // wall on the left, normal points right toward us
        let slid = drive;
        let (out, dyaw, impact) = crash_response(drive, fwd, slid, wall_n);
        assert_eq!(impact, 0.0);
        assert_eq!(dyaw, 0.0);
        assert!((out - drive).length() < 1e-6);
    }

    // --- ram overlap (vehicle_ram) -----------------------------------------
    const EHALF: Vec3 = Vec3::new(0.5, 0.9, 0.5); // a typical grounded monster

    /// A grounded monster squarely in front of the truck is rammed.
    #[test]
    fn rams_monster_in_front() {
        let truck = Vec3::ZERO; // yaw 0 → forward -Z
        let enemy = Vec3::new(0.0, 0.9, -2.0); // ahead, on the deck-height band
        assert!(rammed(enemy, EHALF, truck, 0.0));
    }

    /// A monster off to the side, clear of the footprint, is not rammed.
    #[test]
    fn no_ram_when_clear_to_the_side() {
        let truck = Vec3::ZERO;
        let enemy = Vec3::new(5.0, 0.9, 0.0); // well outside HALF_W + plan radius
        assert!(!rammed(enemy, EHALF, truck, 0.0));
    }

    /// A Scrag hovering well above the truck body passes over it, not rammed.
    #[test]
    fn no_ram_for_flyer_above_the_body() {
        let truck = Vec3::ZERO;
        let enemy = Vec3::new(0.0, 3.0, -2.0); // centred above, feet at 2.1 > body top
        assert!(!rammed(enemy, EHALF, truck, 0.0));
    }

    /// The footprint rotates with the truck: a monster off the +X axis is rammed
    /// once the truck is yawed 90° to face it.
    #[test]
    fn ram_footprint_follows_yaw() {
        let truck = Vec3::ZERO;
        let enemy = Vec3::new(-2.0, 0.9, 0.0); // beside the unturned truck (along its short axis)
        assert!(!rammed(enemy, EHALF, truck, 0.0), "beyond the short half-width when unturned");
        // Yaw +90°: the long axis now lies along world X, so the same monster is in reach.
        assert!(rammed(enemy, EHALF, truck, FRAC_PI_2));
    }
}
