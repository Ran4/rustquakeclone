//! Rideable **mine cart** on a fixed rail spline (feature 45).
//!
//! The cart is almost pure recombination of [`crate::vehicle`]: it's the same
//! moving-collider body the truck and the sliding doors pioneered — it reserves
//! ONE slot in [`WorldColliders`] at build time and rewrites it every frame from
//! its transform, so player collision, monster pathing, hitscan and line-of-sight
//! treat it as solid for free, and the SAME per-frame [`vehicle_carry`] step
//! slides whoever's on the deck along with it. It even carries a [`Vehicle`]
//! component (at its own smaller footprint) so it reuses `vehicle_carry`,
//! `vehicle_ram`, `vehicle_audio` and `vehicle_activate` (board/leave with **E**)
//! completely unchanged.
//!
//! The ONE thing it does differently is its motion: instead of a free velocity
//! vector it's advanced along an authored polyline — a scalar arc-length `s` plus
//! a `speed` integrated against the local grade and the passenger's brake/run
//! input — by [`rail_follow`], which [`vehicle_drive`] explicitly skips while the
//! cart is `attached`. The follow step still *moves* the cart with
//! [`move_and_slide`] (toward the sampled spline target, never a teleport) so it
//! can't tunnel a wall mid-curve and still shoves/bounces off geometry.
//!
//! ## Shoot the world to steer (the headline verb)
//!
//! The track is fixed — you don't steer it — but you can break it. Junction
//! switches and weak rail joints are small shootable collider boxes; a player
//! hitscan shot that lands on one emits a [`BrushStrike`] (the very same message
//! [`crate::resonance`] reads), which [`rail_shoot`] maps back to the cart:
//!   - a **junction switch** toggles which branch tail the cart takes past that
//!     node (only while it hasn't passed the junction yet), or
//!   - a **weak rail joint** *derails* the cart: it clears the spline binding,
//!     seeds the [`Vehicle`] velocity from the last track tangent × speed and
//!     drops straight into the free velocity + angular physics of `vehicle_drive`
//!     — it tumbles, rams monsters and bounces, with the rider kept aboard across
//!     the transition frame (`last_delta` stays continuous; nothing zeroes it).

use bevy::prelude::*;

use crate::common::tune::GRAVITY;
use crate::common::*;
use crate::physics::{move_and_slide, Aabb};
use crate::vehicle::{degenerate, footprint_aabb, footprint_half, ActiveVehicle, Vehicle};

// ----------------------------------------------------------------------------
// Cart dimensions + feel (meters / seconds). The footprint is smaller than the
// truck's; everything is stored on the shared [`Vehicle`] component so the truck
// systems serve the cart at its own size.
// ----------------------------------------------------------------------------
const CART_HALF_W: f32 = 0.7;
const CART_HALF_L: f32 = 1.0;
const CART_BODY_H: f32 = 0.7;
const CART_STEP: f32 = 0.2; // small step-up so the follow sweep crosses tie seams

const CART_MAX: f32 = 18.0; // top speed along the track (m/s, either direction)
const CART_THRUST: f32 = 5.0; // gentle push while W lets it run
const CART_BRAKE: f32 = 30.0; // S drags the brakes hard
const CART_COAST_BRAKE: f32 = 8.0; // light hold while coasting (no key held)
const CART_PARK_BRAKE: f32 = 60.0; // strong hold while un-boarded so it waits for you
const CART_DERAIL_MIN_SPEED: f32 = 3.0; // floor on the momentum a derail hands off

/// A cart bolted to a rail spline. Holds the two possible polylines (the main
/// line and, past a junction, a branch line — identical when there's no branch),
/// the live arc-length `s` and `speed`, and the `attached` flag that hands the
/// body between [`rail_follow`] (on the rails) and [`vehicle_drive`] (derailed).
#[derive(Component)]
pub struct RailCart {
    /// The main line — the full polyline from start to finish.
    pub track_a: Vec<Vec3>,
    /// The branch line: the shared head up to the junction node, then the branch
    /// tail. Equals `track_a` when the cart has no junction.
    pub track_b: Vec<Vec3>,
    /// Arc-length (m) of the junction node along the shared head — the point past
    /// which a/b diverge. `INFINITY` when there's no branch. The switch can only be
    /// thrown while `s < junction_s` (you must redirect before you reach the fork).
    pub junction_s: f32,
    /// Which line is active (false = main `track_a`, true = branch `track_b`).
    pub on_branch: bool,
    /// Arc-length position along the active line (m).
    pub s: f32,
    /// Scalar speed along the active line (m/s; +s direction, may go slightly
    /// negative when gravity rolls it back up a grade).
    pub speed: f32,
    /// While true, [`rail_follow`] drives it on the spline and [`vehicle_drive`]
    /// skips it; a derail flips it false and the free physics takes over.
    pub attached: bool,
    /// `WorldColliders.solids` slot of the shootable junction switch box (if any).
    pub switch_slot: Option<usize>,
    /// `WorldColliders.solids` slot of the shootable weak rail joint (if any).
    pub derail_slot: Option<usize>,
}

impl RailCart {
    /// The polyline the cart is currently following.
    fn active_track(&self) -> &[Vec3] {
        if self.on_branch { &self.track_b } else { &self.track_a }
    }
}

pub struct RailCartPlugin;
impl Plugin for RailCartPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // Read this frame's shots BEFORE the free-physics drive: a derail
                // flips `attached` off so `vehicle_drive` (which runs after us)
                // picks the cart up the SAME frame, keeping the rider's carry
                // continuous across the swap.
                rail_shoot.before(crate::vehicle::vehicle_drive),
                // Spline-follow the still-attached carts AFTER the free drive (which
                // skips them) and BEFORE the rider carry, so the carry reads the
                // cart's fresh `last_delta`.
                rail_follow
                    .after(crate::vehicle::vehicle_drive)
                    .before(crate::vehicle::vehicle_carry),
            )
                // Same window as the vehicle systems: after look, before the
                // player resolves its own collision.
                .after(crate::player::player_look)
                .before(crate::player::player_move)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

// ----------------------------------------------------------------------------
// Pure spline math (unit-tested below)
// ----------------------------------------------------------------------------
/// Total length (m) of a polyline.
fn polyline_len(track: &[Vec3]) -> f32 {
    track.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Sample a polyline at arc-length `s` (clamped to `[0, len]`): returns the point
/// on the line and the unit tangent (direction of increasing `s`) there. Handles
/// degenerate (empty / single-point) tracks by falling back to the -Z forward.
pub(crate) fn sample_polyline(track: &[Vec3], s: f32) -> (Vec3, Vec3) {
    if track.is_empty() {
        return (Vec3::ZERO, Vec3::NEG_Z);
    }
    if track.len() == 1 {
        return (track[0], Vec3::NEG_Z);
    }
    let len = polyline_len(track);
    let s = s.clamp(0.0, len);
    let mut acc = 0.0;
    for w in track.windows(2) {
        let seg = w[1] - w[0];
        let l = seg.length();
        if l < 1e-6 {
            continue;
        }
        if s <= acc + l {
            let f = (s - acc) / l;
            return (w[0] + seg * f, seg / l);
        }
        acc += l;
    }
    // `s` at/past the end: the last vertex, with the final segment's tangent.
    let n = track.len();
    let seg = track[n - 1] - track[n - 2];
    let tan = seg.try_normalize().unwrap_or(Vec3::NEG_Z);
    (track[n - 1], tan)
}

/// Acceleration (m/s²) gravity imparts ALONG the direction of travel for a unit
/// `tangent`. Gravity is `(0, -GRAVITY, 0)`, so its component along the tangent is
/// `-GRAVITY * tangent.y`: downhill (`tangent.y < 0`) speeds the cart up
/// (positive), uphill slows it (negative), level is zero.
fn grade_accel(tangent: Vec3) -> f32 {
    -GRAVITY * tangent.y
}

/// Heading (yaw) whose [`forward`](crate::vehicle) `(-sin, 0, -cos)` points along
/// the planar part of `tangent` — i.e. heading = -tangent on the plane, matching
/// the -Z forward convention the player and truck share.
fn yaw_from_tangent(tangent: Vec3) -> f32 {
    (-tangent.x).atan2(-tangent.z)
}

/// Wrap an angle delta into `[-π, π]` so a carry/turn never spins the long way
/// round when the tangent's atan2 wraps.
fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut a = a % TAU;
    if a > PI {
        a -= TAU;
    } else if a < -PI {
        a += TAU;
    }
    a
}

// ----------------------------------------------------------------------------
// Spawning the cart model + reserving its collider slots
// ----------------------------------------------------------------------------
/// Spawn a mine cart following `track` (a low-poly open ore bin on 4 wheels) at
/// `track[0]`, reserving its moving footprint collider slot. `branch` optionally
/// adds a shootable junction switch at node index `j` whose alternate tail
/// polyline `tail` (which MUST start at `track[j]`) the cart takes when thrown;
/// `derail_node` optionally marks a shootable weak rail joint at that node. Both
/// markers reserve their own collider slot and spawn a glowing box you can shoot.
/// Returns the cart entity. Call during level build so the slot indices stay
/// stable (see [`crate::level::Build::rail`]).
pub fn spawn_cart(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    colliders: &mut Vec<Aabb>,
    track: &[Vec3],
    branch: Option<(usize, &[Vec3])>,
    derail_node: Option<usize>,
) -> Entity {
    let track_a = track.to_vec();
    let start = track_a.first().copied().unwrap_or(Vec3::ZERO);

    // Build the branch line (shared head + branch tail) and the junction arc-length.
    let (track_b, junction_s, switch_node) = match branch {
        Some((j, tail)) if !tail.is_empty() && j < track_a.len() => {
            let mut tb: Vec<Vec3> = track_a[..=j].to_vec();
            tb.extend_from_slice(&tail[1..]); // tail[0] duplicates the junction node
            (tb, polyline_len(&track_a[..=j]), Some(track_a[j]))
        }
        _ => (track_a.clone(), f32::INFINITY, None),
    };

    let (_, tan0) = sample_polyline(&track_a, 0.0);
    let yaw = yaw_from_tangent(tan0);

    // Reserve the moving footprint slot (rewritten every frame by rail_follow).
    let collider = colliders.len();
    colliders.push(footprint_aabb(start, yaw, CART_HALF_W, CART_HALF_L, CART_BODY_H));

    // Shootable junction switch: a glowing lever box set just off the rail beside
    // the junction node (clear of the cart's footprint so it never bumps it).
    let switch_slot = switch_node.map(|node| {
        let (_, tan) = sample_polyline(&track_a, junction_s);
        let perp = Vec3::new(tan.z, 0.0, -tan.x).try_normalize().unwrap_or(Vec3::X);
        let center = node + perp * (CART_HALF_W + 1.0) + Vec3::Y * 0.9;
        let slot = colliders.len();
        colliders.push(Aabb::from_center_half(center, Vec3::splat(0.45)));
        spawn_marker(commands, meshes, materials, center, 0.45, rgb(1.0, 0.85, 0.2), LinearRgba::rgb(3.2, 2.4, 0.3));
        slot
    });

    // Shootable weak rail joint: a red clamp box beside the derail node.
    let derail_slot = derail_node.filter(|&n| n < track_a.len()).map(|n| {
        let node = track_a[n];
        let (_, tan) = sample_polyline(&track_a, polyline_len(&track_a[..=n]));
        let perp = Vec3::new(tan.z, 0.0, -tan.x).try_normalize().unwrap_or(Vec3::X);
        let center = node + perp * (CART_HALF_W + 1.0) + Vec3::Y * 0.6;
        let slot = colliders.len();
        colliders.push(Aabb::from_center_half(center, Vec3::splat(0.4)));
        spawn_marker(commands, meshes, materials, center, 0.4, rgb(1.0, 0.25, 0.15), LinearRgba::rgb(3.4, 0.4, 0.2));
        slot
    });

    // --- the cart model: an open iron ore bin on four wheels -----------------
    let iron = materials.add(StandardMaterial {
        base_color: rgb(0.30, 0.22, 0.18),
        perceptual_roughness: 0.7,
        metallic: 0.6,
        emissive: LinearRgba::rgb(0.02, 0.01, 0.005),
        ..default()
    });
    let plate = materials.add(StandardMaterial {
        base_color: rgb(0.34, 0.27, 0.17),
        perceptual_roughness: 0.9,
        ..default()
    });
    let rubber = materials.add(StandardMaterial {
        base_color: rgb(0.05, 0.05, 0.06),
        perceptual_roughness: 0.95,
        ..default()
    });

    let cube = |m: &mut Assets<Mesh>, sz: Vec3| m.add(Cuboid::from_size(sz));
    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> = vec![
        // under-frame, then the floor plate you stand on (its top ≈ body height)
        (cube(meshes, Vec3::new(1.5, 0.5, 2.1)), iron.clone(), Transform::from_xyz(0.0, 0.45, 0.0)),
        (cube(meshes, Vec3::new(1.3, 0.16, 1.9)), plate.clone(), Transform::from_xyz(0.0, 0.62, 0.0)),
        // open bin walls (low, wall-less top — a flatbed of sorts)
        (cube(meshes, Vec3::new(1.5, 0.5, 0.16)), iron.clone(), Transform::from_xyz(0.0, 0.95, -1.0)),
        (cube(meshes, Vec3::new(1.5, 0.5, 0.16)), iron.clone(), Transform::from_xyz(0.0, 0.95, 1.0)),
        (cube(meshes, Vec3::new(0.16, 0.5, 2.1)), iron.clone(), Transform::from_xyz(-0.75, 0.95, 0.0)),
        (cube(meshes, Vec3::new(0.16, 0.5, 2.1)), iron.clone(), Transform::from_xyz(0.75, 0.95, 0.0)),
    ];
    // Wheels: cylinders laid on their side (axis along X).
    let wheel = meshes.add(Cylinder { radius: 0.35, half_height: 0.12 });
    for (x, z) in [(-0.75, -0.75), (0.75, -0.75), (-0.75, 0.75), (0.75, 0.75)] {
        parts.push((
            wheel.clone(),
            rubber.clone(),
            Transform {
                translation: Vec3::new(x, 0.35, z),
                rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                scale: Vec3::ONE,
            },
        ));
    }

    commands
        .spawn((
            Transform::from_translation(start).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            Vehicle {
                yaw,
                vel: Vec3::ZERO,
                yaw_rate: 0.0,
                collider,
                last_delta: Vec3::ZERO,
                last_dyaw: 0.0,
                half_w: CART_HALF_W,
                half_l: CART_HALF_L,
                body_h: CART_BODY_H,
                driver_local: Vec3::new(0.0, CART_BODY_H, 0.0),
                // The mine cart mounts no pintle gun; park the gun seat far out of
                // any deck reach so `gun_activate`'s near-test can never crew it.
                gun_local: Vec3::splat(1.0e6),
            },
            RailCart {
                track_a,
                track_b,
                junction_s,
                on_branch: false,
                s: 0.0,
                speed: 0.0,
                attached: true,
                switch_slot,
                derail_slot,
            },
            LevelEntity,
            Name::new("MineCart"),
        ))
        .with_children(|c| {
            for (mesh, mat, tf) in &parts {
                c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *tf));
            }
        })
        .id()
}

/// Spawn one shootable, self-lit marker box (a junction lever / rail-joint clamp).
fn spawn_marker(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    center: Vec3,
    half: f32,
    color: Color,
    emissive: LinearRgba,
) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::splat(half * 2.0)));
    let mat = materials.add(StandardMaterial { base_color: color, emissive, perceptual_roughness: 0.4, metallic: 0.5, ..default() });
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_translation(center),
        LevelEntity,
    ));
}

// ----------------------------------------------------------------------------
// Systems
// ----------------------------------------------------------------------------
/// Drive every *attached* cart along its spline: integrate `speed` against the
/// grade + brake input, advance `s`, sample the target, and MOVE toward it with
/// the swept solver (so it can't tunnel a wall and still bounces off geometry).
/// Rewrites the reserved collider slot and sets `last_delta`/`last_dyaw` for the
/// shared rider carry. [`vehicle_drive`] skips attached carts, so this owns them.
fn rail_follow(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveVehicle>,
    mut colliders: ResMut<WorldColliders>,
    mut q: Query<(Entity, &mut Transform, &mut Vehicle, &mut RailCart)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (e, mut tf, mut v, mut cart) in &mut q {
        if !cart.attached {
            continue;
        }
        let driving = active.0 == Some(e);

        // --- integrate speed (read the active line, then write the cart) -----
        let target;
        let tan;
        let yaw;
        let sp;
        let len;
        {
            let track = cart.active_track();
            len = polyline_len(track);
            let (_, tan0) = sample_polyline(track, cart.s);

            let mut s_speed = cart.speed + grade_accel(tan0) * dt;
            // Brake regime: parked (held) until boarded; then W lets it run (a
            // small thrust on top of gravity), S drags the brakes hard, and merely
            // coasting holds it gently on the grade.
            let brake = if driving {
                if keys.pressed(KeyCode::KeyW) {
                    s_speed += CART_THRUST * dt;
                    0.0
                } else if keys.pressed(KeyCode::KeyS) {
                    CART_BRAKE
                } else {
                    CART_COAST_BRAKE
                }
            } else {
                CART_PARK_BRAKE
            };
            let b = brake * dt;
            s_speed = if s_speed > 0.0 { (s_speed - b).max(0.0) } else { (s_speed + b).min(0.0) };
            s_speed = s_speed.clamp(-CART_MAX, CART_MAX);

            // Advance the arc-length and sample the new target + tangent.
            let mut s_next = (cart.s + s_speed * dt).clamp(0.0, len);
            if s_next <= 0.0 || s_next >= len {
                s_speed = 0.0; // come to rest at either end of the line
                s_next = s_next.clamp(0.0, len);
            }
            let (p, t) = sample_polyline(track, s_next);
            target = p;
            tan = t;
            yaw = yaw_from_tangent(t);
            sp = s_speed;
        }

        // --- move toward the target via the swept solver ---------------------
        // delta is the step the spline asks for; handing it (as a velocity) to
        // move_and_slide rather than teleporting means a wall mid-curve shoves and
        // bounces the cart instead of letting it tunnel through.
        let old_t = tf.translation;
        let bh = v.body_h;
        let h = footprint_half(yaw, v.half_w, v.half_l);
        let half = Vec3::new(h.x, bh * 0.5, h.y);
        let center = old_t + Vec3::Y * (bh * 0.5);
        let delta = target - old_t;
        if let Some(slot) = colliders.solids.get_mut(v.collider) {
            *slot = degenerate();
        }
        let res = move_and_slide(center, half, delta / dt, dt, &colliders.solids, CART_STEP);
        let new_t = res.pos - Vec3::Y * (bh * 0.5);

        // Feed the shared rider-carry step.
        tf.translation = new_t;
        tf.rotation = Quat::from_rotation_y(yaw);
        v.last_delta = new_t - old_t;
        v.last_dyaw = wrap_angle(yaw - v.yaw);
        v.yaw = yaw;
        // Keep a live world velocity around the tangent so a derail this frame (and
        // vehicle_ram) read a sane momentum even before the handoff seeds it.
        v.vel = tan * sp;
        v.yaw_rate = 0.0;
        // Commit arc-length by the distance the body *actually* swept (projected
        // onto the tangent), not the requested step. If geometry clips the cart for
        // a frame, `s` then holds with the body instead of running ahead — so when
        // the obstruction clears the next `delta` stays small and can't present a
        // tunneling-scale velocity to the swept solver (nor visibly jump to catch up).
        let moved = (new_t - old_t).dot(tan);
        cart.s = (cart.s + moved).clamp(0.0, len);
        cart.speed = sp;

        if let Some(slot) = colliders.solids.get_mut(v.collider) {
            *slot = footprint_aabb(new_t, yaw, v.half_w, v.half_l, bh);
        }
    }
}

/// Shoot the world to steer: read this frame's brush strikes (the same
/// [`BrushStrike`] messages the resonance system reads) and, for any that land on
/// a cart's junction switch or weak rail joint, throw the junction (swap the
/// branch the cart takes past the node) or derail it (hand it to the free physics).
fn rail_shoot(
    mut strikes: MessageReader<BrushStrike>,
    mut q: Query<(&Transform, &mut Vehicle, &mut RailCart)>,
    mut notify: MessageWriter<Notify>,
    mut sfx: MessageWriter<Sfx>,
) {
    // De-dupe this frame's discrete shots by slot: a shotgun blast lands several
    // pellets on one switch box and an even count would toggle the junction back
    // where it started. The held Lightning beam pulses every frame (it's the
    // resonance tool, not a trigger), so ignore its `beam` strikes here.
    let mut slots: Vec<usize> = Vec::new();
    for s in strikes.read() {
        if !s.beam && !slots.contains(&s.slot) {
            slots.push(s.slot);
        }
    }
    if slots.is_empty() {
        return;
    }
    for (tf, mut v, mut cart) in &mut q {
        if !cart.attached {
            continue;
        }
        for &slot in &slots {
            if cart.switch_slot == Some(slot) && cart.s < cart.junction_s {
                // Throw the junction — only valid before the cart reaches the fork.
                cart.on_branch = !cart.on_branch;
                notify.write(Notify::new(if cart.on_branch {
                    "Junction thrown — branch line"
                } else {
                    "Junction thrown — main line"
                }));
                sfx.write(Sfx::at(Sound::Crash, tf.translation));
            } else if cart.derail_slot == Some(slot) {
                // Rip the joint: seed the free velocity from the live tangent ×
                // speed so momentum is continuous, then drop the spline binding.
                // DON'T touch last_delta — the rider must stay aboard across the
                // swap, and vehicle_drive (which runs after us) writes a fresh
                // continuous last_delta this same frame.
                let (_, tan) = sample_polyline(cart.active_track(), cart.s);
                // Floor the handed-off momentum to a minimum, keeping the sign of
                // travel (+tan when rolling forward, -tan when rolled back).
                let mag = cart.speed.abs().max(CART_DERAIL_MIN_SPEED);
                v.vel = tan * if cart.speed < 0.0 { -mag } else { mag };
                v.yaw_rate = 0.0;
                cart.attached = false;
                notify.write(Notify::new("The rail tears loose — off the rails!"));
                sfx.write(Sfx::at(Sound::Crash, tf.translation));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sampling at the two ends returns the endpoints exactly.
    #[test]
    fn sample_hits_the_endpoints() {
        let track = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0)];
        let (p0, _) = sample_polyline(&track, 0.0);
        let (p1, _) = sample_polyline(&track, 10.0);
        assert!(p0.distance(track[0]) < 1e-4);
        assert!(p1.distance(track[1]) < 1e-4);
    }

    /// Arc-length sampling on a straight segment lands at the right fraction with a
    /// unit tangent along the segment.
    #[test]
    fn sample_midpoint_by_arclength() {
        let track = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0)];
        let (p, t) = sample_polyline(&track, 4.0);
        assert!(p.distance(Vec3::new(0.0, 0.0, -4.0)) < 1e-4, "got {p:?}");
        assert!(t.distance(Vec3::NEG_Z) < 1e-4, "tangent {t:?}");
        assert!((t.length() - 1.0).abs() < 1e-5);
    }

    /// Sampling carries correctly across a corner: 5m into a 3m-then-4m L lands 2m
    /// along the second (perpendicular) leg, with the second leg's tangent.
    #[test]
    fn sample_across_a_corner() {
        let track = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -3.0), // first leg: 3m along -Z
            Vec3::new(4.0, 0.0, -3.0), // second leg: 4m along +X
        ];
        assert!((polyline_len(&track) - 7.0).abs() < 1e-4);
        let (p, t) = sample_polyline(&track, 5.0);
        assert!(p.distance(Vec3::new(2.0, 0.0, -3.0)) < 1e-4, "got {p:?}");
        assert!(t.distance(Vec3::X) < 1e-4, "tangent {t:?}");
    }

    /// grade -> accel sign: downhill speeds up (+), uphill slows (-), level is ~0.
    #[test]
    fn grade_accel_sign_follows_the_slope() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let downhill = Vec3::new(0.0, -s, -s); // travelling forward and down
        let uphill = Vec3::new(0.0, s, -s); // forward and up
        let level = Vec3::new(0.0, 0.0, -1.0);
        assert!(grade_accel(downhill) > 0.0, "downhill should speed up");
        assert!(grade_accel(uphill) < 0.0, "uphill should slow down");
        assert!(grade_accel(level).abs() < 1e-5, "level grade is neutral");
    }

    /// The yaw derived from a tangent makes `forward(yaw)` point along that tangent
    /// (the -Z convention): a -Z tangent → yaw 0, a +X tangent → yaw -π/2.
    #[test]
    fn yaw_matches_the_forward_convention() {
        assert!(yaw_from_tangent(Vec3::NEG_Z).abs() < 1e-5);
        assert!((yaw_from_tangent(Vec3::X) + std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }
}
