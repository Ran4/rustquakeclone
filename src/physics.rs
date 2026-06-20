//! Axis-aligned collision: swept AABB move-and-slide with step-up, ground
//! probing, and raycasts (for hitscan weapons and enemy line-of-sight).
//!
//! The whole level is built from axis-aligned boxes (brushes), which lets us
//! use exact Minkowski-expanded swept-AABB collision — crisp and Quake-precise,
//! with no physics-engine dependency.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_center_half(center: Vec3, half: Vec3) -> Self {
        Self { min: center - half, max: center + half }
    }
    /// Build from two opposite corners (any order).
    pub fn from_corners(a: Vec3, b: Vec3) -> Self {
        Self { min: a.min(b), max: a.max(b) }
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    /// Minkowski expansion by half-extents `h` (turns box-vs-box into point-vs-box).
    pub fn expand(&self, h: Vec3) -> Aabb {
        Aabb { min: self.min - h, max: self.max + h }
    }
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x && self.max.x >= o.min.x
            && self.min.y <= o.max.y && self.max.y >= o.min.y
            && self.min.z <= o.max.z && self.max.z >= o.min.z
    }
}

/// Ray (segment) vs AABB slab test. `dir` is the full displacement (not unit).
/// Returns entry time t in [0,1] and the surface normal, if the segment from
/// `origin` to `origin+dir` first enters the box within that range.
pub fn segment_aabb(origin: Vec3, dir: Vec3, b: &Aabb) -> Option<(f32, Vec3)> {
    let mut tmin = 0.0f32;
    let mut tmax = 1.0f32;
    let mut normal = Vec3::ZERO;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        let lo = b.min[axis];
        let hi = b.max[axis];
        if d.abs() < 1e-8 {
            // Parallel to slab: must already be within it.
            if o < lo || o > hi {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let mut t1 = (lo - o) * inv;
            let mut t2 = (hi - o) * inv;
            let mut n = -1.0f32; // normal sign for the entry face on this axis
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = 1.0;
            }
            if t1 > tmin {
                tmin = t1;
                normal = Vec3::ZERO;
                normal[axis] = n;
            }
            if t2 < tmax {
                tmax = t2;
            }
            if tmin > tmax {
                return None;
            }
        }
    }
    Some((tmin, normal))
}

/// Pure ray vs AABB (infinite ray, `dir` unit length, up to `max` distance).
/// Returns (distance, normal) of the nearest entry.
pub fn ray_aabb(origin: Vec3, dir: Vec3, max: f32, b: &Aabb) -> Option<(f32, Vec3)> {
    let mut tmin = 0.0f32;
    let mut tmax = max;
    let mut normal = Vec3::ZERO;
    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        if d.abs() < 1e-8 {
            if o < b.min[axis] || o > b.max[axis] {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let mut t1 = (b.min[axis] - o) * inv;
            let mut t2 = (b.max[axis] - o) * inv;
            let mut n = -1.0f32;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = 1.0;
            }
            if t1 > tmin {
                tmin = t1;
                normal = Vec3::ZERO;
                normal[axis] = n;
            }
            if t2 < tmax {
                tmax = t2;
            }
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmin <= tmax && tmin >= 0.0 {
        Some((tmin, normal))
    } else {
        None
    }
}

/// Nearest world hit for a ray, returns (distance, point, normal).
pub fn raycast_world(
    origin: Vec3,
    dir: Vec3,
    max: f32,
    solids: &[Aabb],
) -> Option<(f32, Vec3, Vec3)> {
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for b in solids {
        if let Some((t, n)) = ray_aabb(origin, dir, max, b) {
            if best.map_or(true, |(bt, _, _)| t < bt) {
                best = Some((t, origin + dir * t, n));
            }
        }
    }
    best
}

/// Is the straight segment from `a` to `b` unobstructed by world geometry?
pub fn line_of_sight(a: Vec3, b: Vec3, solids: &[Aabb]) -> bool {
    let delta = b - a;
    let dist = delta.length();
    if dist < 1e-5 {
        return true;
    }
    let dir = delta / dist;
    match raycast_world(a, dir, dist, solids) {
        Some((t, _, _)) => t >= dist - 1e-3,
        None => true,
    }
}

const SKIN: f32 = 0.0015;

/// Result of a swept move.
pub struct MoveResult {
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub hit_wall: bool,
}

/// Sweep an AABB (center `pos`, half-extents `half`) through `vel` over `dt`,
/// sliding along surfaces (clipping the velocity vector, Quake-style, so speed
/// is preserved tangentially). Performs step-up so the mover climbs stairs and
/// ledges up to `step_height`. Returns resolved position and velocity.
pub fn move_and_slide(
    pos: Vec3,
    half: Vec3,
    vel: Vec3,
    dt: f32,
    solids: &[Aabb],
    step_height: f32,
) -> MoveResult {
    // Plain velocity-clipping slide of the full motion.
    let (p1, v1, wall, _floor) = slide_move(pos, half, vel, dt, solids);

    let mut out_pos = p1;
    let mut out_vel = v1;

    // If we bumped a wall while trying to move horizontally, attempt a step-up
    // (up -> forward -> settle) so stairs and low ledges are walkable.
    let horiz = Vec3::new(vel.x, 0.0, vel.z);
    if wall && horiz.length_squared() > 1e-4 {
        let plain_gain = (p1 - pos) * Vec3::new(1.0, 0.0, 1.0);
        let (pu, mu, _) = slide(pos, half, Vec3::new(0.0, step_height, 0.0), solids);
        if mu.y > step_height * 0.5 {
            let (pf, mf, _) = slide(pu, half, horiz * dt, solids);
            if mf.length_squared() > plain_gain.length_squared() + 1e-6 {
                let (pd, _, _) =
                    slide(pf, half, Vec3::new(0.0, -(step_height + 0.05), 0.0), solids);
                out_pos = pd;
                // Keep horizontal speed so climbing stairs doesn't sap momentum.
                out_vel = Vec3::new(vel.x, v1.y.min(0.0), vel.z);
            }
        }
    }

    let on_ground = ground_check(out_pos, half, solids) && out_vel.y <= 0.05;
    if on_ground && out_vel.y < 0.0 {
        out_vel.y = 0.0;
    }

    MoveResult { pos: out_pos, vel: out_vel, on_ground, hit_wall: wall }
}

/// Velocity-threading slide: integrates `vel` over `dt`, clipping the velocity
/// against each surface it hits. Returns (pos, clipped_vel, hit_wall, hit_floor).
fn slide_move(
    pos: Vec3,
    half: Vec3,
    vel: Vec3,
    dt: f32,
    solids: &[Aabb],
) -> (Vec3, Vec3, bool, bool) {
    let mut cur = pos;
    let mut velocity = vel;
    let mut time_left = dt;
    let mut hit_wall = false;
    let mut hit_floor = false;

    for _ in 0..4 {
        if time_left <= 1e-6 {
            break;
        }
        let disp = velocity * time_left;
        if disp.length_squared() < 1e-12 {
            break;
        }
        let mut best_t = 1.0f32;
        let mut best_n = Vec3::ZERO;
        let mut found = false;
        for b in solids {
            let eb = b.expand(half);
            if let Some((t, n)) = segment_aabb(cur, disp, &eb) {
                if t < best_t {
                    best_t = t;
                    best_n = n;
                    found = true;
                }
            }
        }
        if !found {
            cur += disp;
            break;
        }
        // Convert the world-space SKIN gap into a fraction of this displacement
        // so we leave a constant small gap regardless of speed.
        let disp_len = disp.length();
        let move_t = if disp_len > 1e-9 { ((best_t * disp_len - SKIN) / disp_len).max(0.0) } else { best_t };
        cur += disp * move_t;
        time_left *= 1.0 - move_t;
        if best_n.y > 0.7 {
            hit_floor = true;
        } else if best_n.y < -0.7 {
            // ceiling
        } else {
            hit_wall = true;
        }
        // Clip velocity onto the surface plane (project out the normal component).
        let into = velocity.dot(best_n);
        if into < 0.0 {
            velocity -= best_n * into;
        }
    }
    (cur, velocity, hit_wall, hit_floor)
}

/// Pure positional slide of a displacement `disp` (used for step probes and the
/// ground check). Returns (final_pos, total_moved, hit_any).
fn slide(pos: Vec3, half: Vec3, disp: Vec3, solids: &[Aabb]) -> (Vec3, Vec3, bool) {
    let mut cur = pos;
    let mut remaining = disp;
    let mut total = Vec3::ZERO;
    let mut hit_any = false;

    for _ in 0..4 {
        if remaining.length_squared() < 1e-10 {
            break;
        }
        let mut best_t = 1.0f32;
        let mut best_n = Vec3::ZERO;
        let mut found = false;
        for b in solids {
            let eb = b.expand(half);
            if let Some((t, n)) = segment_aabb(cur, remaining, &eb) {
                if t < best_t {
                    best_t = t;
                    best_n = n;
                    found = true;
                }
            }
        }
        if !found {
            cur += remaining;
            total += remaining;
            break;
        }
        hit_any = true;
        let rem_len = remaining.length();
        let move_t = if rem_len > 1e-9 { ((best_t * rem_len - SKIN) / rem_len).max(0.0) } else { best_t };
        let step = remaining * move_t;
        cur += step;
        total += step;
        let leftover = remaining * (1.0 - move_t);
        let into = leftover.dot(best_n);
        remaining = leftover - best_n * into;
    }
    (cur, total, hit_any)
}

/// True if there's solid ground within a small distance below the AABB.
pub fn ground_check(pos: Vec3, half: Vec3, solids: &[Aabb]) -> bool {
    let probe = 0.14;
    let (_, moved, hit) = slide(pos, half, Vec3::new(0.0, -probe, 0.0), solids);
    // Grounded if the downward probe was blocked before completing.
    hit && moved.y > -probe + 1e-4
}
