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

/// Which vehicle the player is currently driving (`None` = on foot). A resource
/// rather than a marker component so `player_move` and `vehicle_drive` both see
/// the change the same frame, with no command-buffer latency.
#[derive(Resource, Default)]
pub struct ActiveVehicle(pub Option<Entity>);

/// A drivable vehicle. The entity's `Transform.translation` is its ground point
/// (centre of the footprint, at floor level); `yaw` is its heading.
#[derive(Component)]
pub struct Vehicle {
    pub yaw: f32,
    /// Signed forward speed (negative = reversing).
    pub speed: f32,
    /// Vertical velocity (gravity / settling).
    pub vy: f32,
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
                (vehicle_activate, vehicle_drive, vehicle_carry)
                    .chain()
                    // After look so the carry's yaw add lands on the freshly
                    // mouse-updated heading; before move so the player's own
                    // collision resolves on the already-carried position.
                    .after(crate::player::player_look)
                    .before(crate::player::player_move)
                    .run_if(in_state(GameState::Playing)),
            )
            // The driven vehicle is a LevelEntity (despawned on rebuild); drop the
            // dangling handle so a fresh level never starts in a phantom "driving".
            .add_systems(OnExit(GameState::Playing), |mut a: ResMut<ActiveVehicle>| a.0 = None);
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

fn coast(speed: f32, dt: f32) -> f32 {
    let d = DRAG * dt;
    if speed > 0.0 { (speed - d).max(0.0) } else { (speed + d).min(0.0) }
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
            Vehicle { yaw, speed: 0.0, vy: 0.0, collider, last_delta: Vec3::ZERO, last_dyaw: 0.0 },
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
fn vehicle_activate(
    keys: Res<ButtonInput<KeyCode>>,
    mut active: ResMut<ActiveVehicle>,
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
    let Ok(ptf) = q_player.single() else { return };
    for (e, vtf, v) in &q_veh {
        let on_deck = ride(ptf.translation, PLAYER_HALF[1], vtf.translation, v.yaw, Vec3::ZERO, 0.0).is_some();
        let driver = vtf.translation + Quat::from_rotation_y(v.yaw) * DRIVER_LOCAL;
        let near = (ptf.translation.x - driver.x).hypot(ptf.translation.z - driver.z) < 2.4;
        if on_deck && near {
            active.0 = Some(e);
            notify.write(Notify::new("Driving — W/S throttle, A/D steer, E to exit"));
            sfx.write(Sfx::at(Sound::Door, vtf.translation));
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
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (e, mut tf, mut v) in &mut q {
        let driving = active.0 == Some(e);
        let mut speed = v.speed;
        let mut yaw = v.yaw;

        if driving {
            let mut throttle = false;
            if keys.pressed(KeyCode::KeyW) {
                speed += ACCEL * dt;
                throttle = true;
            }
            if keys.pressed(KeyCode::KeyS) {
                speed -= BRAKE * dt;
                throttle = true;
            }
            if !throttle {
                speed = coast(speed, dt);
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
            yaw += steer * TURN_RATE * dt * (speed / TURN_REF).clamp(-1.0, 1.0);
        } else {
            speed = coast(speed, dt);
        }
        speed = speed.clamp(-MAX_REVERSE, MAX_SPEED);

        let vy = v.vy - GRAVITY * dt;
        let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let vel = fwd * speed + Vec3::Y * vy;

        // Sweep the footprint, excluding this truck's own slot so it can't
        // collide with itself.
        let old_t = tf.translation;
        let h = footprint_half(yaw);
        let half = Vec3::new(h.x, BODY_H * 0.5, h.y);
        let center = Vec3::new(old_t.x, old_t.y + BODY_H * 0.5, old_t.z);
        if let Some(slot) = colliders.solids.get_mut(v.collider) {
            *slot = degenerate();
        }
        let res = move_and_slide(center, half, vel, dt, &colliders.solids, STEP);
        let new_t = Vec3::new(res.pos.x, res.pos.y - BODY_H * 0.5, res.pos.z);

        // Bleed off speed when a wall stops the truck; zero vy on the ground.
        let resolved_speed = Vec3::new(res.vel.x, 0.0, res.vel.z).dot(fwd);
        let resolved_vy = if res.on_ground { 0.0 } else { res.vel.y };

        tf.translation = new_t;
        tf.rotation = Quat::from_rotation_y(yaw);
        v.last_delta = new_t - old_t;
        v.last_dyaw = yaw - v.yaw;
        v.yaw = yaw;
        v.speed = resolved_speed;
        v.vy = resolved_vy;

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
