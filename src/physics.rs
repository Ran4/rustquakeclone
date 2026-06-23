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
    raycast_world_indexed(origin, dir, max, solids).map(|(_, t, p, n)| (t, p, n))
}

/// Like [`raycast_world`] but also hands back which `solids` slot the nearest hit
/// belongs to, so callers can map a beam back to a specific brush (feature 29:
/// the Lightning Gun needs the hit slot to look up a resonant brush's profile).
pub fn raycast_world_indexed(
    origin: Vec3,
    dir: Vec3,
    max: f32,
    solids: &[Aabb],
) -> Option<(usize, f32, Vec3, Vec3)> {
    let mut best: Option<(usize, f32, Vec3, Vec3)> = None;
    for (i, b) in solids.iter().enumerate() {
        if let Some((t, n)) = ray_aabb(origin, dir, max, b) {
            if best.map_or(true, |(_, bt, _, _)| t < bt) {
                best = Some((i, t, origin + dir * t, n));
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
    /// Summed outward normal of the wall(s) hit during the slide (points from the
    /// wall back toward the mover), or zero if no wall was hit. Normalize before
    /// use. Lets a caller bounce/spin off a surface instead of just sliding.
    pub wall_normal: Vec3,
}

/// Push `pos` (centre of an AABB with half-extents `half`) out of any solid it
/// is *inside*, along the axis of least penetration. Iterated so an overlap with
/// several brushes at once (a corner) resolves.
///
/// This is the safety net for the swept solver. `segment_aabb` returns
/// `(t=0, normal=ZERO)` when its origin is already inside a box, which makes
/// `slide_move`/`slide` stall completely — `move_t` clamps to 0 (no motion) and
/// the zero normal never clips the velocity, so a mover that ends up embedded in
/// a brush (a spawn placed too close, a moving brush pushed into it, a fast
/// knock-back into a corner) is frozen for good. Healing the penetration before
/// the sweep keeps that degenerate case unreachable.
///
/// Only a *strictly* inside centre is moved: a mover resting flush against a
/// face (separated by the `SKIN` gap the slide always leaves) is not inside, so
/// normal resting contact, sliding and step-up are untouched.
pub fn depenetrate(mut pos: Vec3, half: Vec3, solids: &[Aabb]) -> Vec3 {
    for _ in 0..4 {
        let mut moved = false;
        for b in solids {
            let eb = b.expand(half);
            if pos.x <= eb.min.x
                || pos.x >= eb.max.x
                || pos.y <= eb.min.y
                || pos.y >= eb.max.y
                || pos.z <= eb.min.z
                || pos.z >= eb.max.z
            {
                continue; // not strictly inside this expanded box
            }
            // Penetration depth toward each of the six faces; escape via the
            // shallowest (minimum-translation vector).
            let pen = [
                (pos.x - eb.min.x, Vec3::NEG_X),
                (eb.max.x - pos.x, Vec3::X),
                (pos.y - eb.min.y, Vec3::NEG_Y),
                (eb.max.y - pos.y, Vec3::Y),
                (pos.z - eb.min.z, Vec3::NEG_Z),
                (eb.max.z - pos.z, Vec3::Z),
            ];
            let (depth, dir) = pen
                .iter()
                .copied()
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                .unwrap();
            // Push fully out plus the SKIN gap, so the next sweep starts clear.
            pos += dir * (depth + SKIN);
            moved = true;
        }
        if !moved {
            break;
        }
    }
    pos
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
    // Heal any pre-existing penetration first, so the inside-origin degenerate
    // case in `segment_aabb` (t=0, ZERO normal -> zero progress) is never fed
    // into the sweep. Without this a mover that ever gets embedded in a brush
    // stays frozen forever.
    let pos = depenetrate(pos, half, solids);

    // Plain velocity-clipping slide of the full motion.
    let (p1, v1, wall, _floor, wall_n) = slide_move(pos, half, vel, dt, solids);

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

    MoveResult { pos: out_pos, vel: out_vel, on_ground, hit_wall: wall, wall_normal: wall_n }
}

/// Velocity-threading slide: integrates `vel` over `dt`, clipping the velocity
/// against each surface it hits. Returns
/// (pos, clipped_vel, hit_wall, hit_floor, summed_wall_normal).
fn slide_move(
    pos: Vec3,
    half: Vec3,
    vel: Vec3,
    dt: f32,
    solids: &[Aabb],
) -> (Vec3, Vec3, bool, bool, Vec3) {
    let mut cur = pos;
    let mut velocity = vel;
    let mut time_left = dt;
    let mut hit_wall = false;
    let mut hit_floor = false;
    let mut wall_normal = Vec3::ZERO;

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
            wall_normal += best_n;
        }
        // Clip velocity onto the surface plane (project out the normal component).
        let into = velocity.dot(best_n);
        if into < 0.0 {
            velocity -= best_n * into;
        }
    }
    (cur, velocity, hit_wall, hit_floor, wall_normal)
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

/// Find a vertical wall the AABB is pressed up against, within `reach` of its
/// surface horizontally. Returns the wall's outward normal (unit, horizontal,
/// pointing from the wall back toward the mover) — used for wall jumps. Probes
/// the four cardinal horizontal directions and returns the nearest wall's
/// normal; floors and ceilings are ignored.
pub fn nearby_wall_normal(pos: Vec3, half: Vec3, solids: &[Aabb], reach: f32) -> Option<Vec3> {
    let dirs = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];
    let mut best: Option<(f32, Vec3)> = None;
    for d in dirs {
        let disp = d * reach;
        for b in solids {
            let eb = b.expand(half);
            if let Some((t, n)) = segment_aabb(pos, disp, &eb) {
                // Only count near-vertical surfaces (walls), not floors/ceilings.
                if n.y.abs() < 0.7 && best.map_or(true, |(bt, _)| t < bt) {
                    // Outward normal points back toward the mover: opposite of probe.
                    best = Some((t, -d));
                }
            }
        }
    }
    best.map(|(_, n)| n)
}

/// True if there's solid ground within a small distance below the AABB.
pub fn ground_check(pos: Vec3, half: Vec3, solids: &[Aabb]) -> bool {
    let probe = 0.14;
    let (_, moved, hit) = slide(pos, half, Vec3::new(0.0, -probe, 0.0), solids);
    // Grounded if the downward probe was blocked before completing.
    hit && moved.y > -probe + 1e-4
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF: Vec3 = Vec3::new(0.4, 0.9, 0.4);

    fn inside_any(pos: Vec3, half: Vec3, solids: &[Aabb]) -> bool {
        solids.iter().any(|b| {
            let eb = b.expand(half);
            pos.x > eb.min.x
                && pos.x < eb.max.x
                && pos.y > eb.min.y
                && pos.y < eb.max.y
                && pos.z > eb.min.z
                && pos.z < eb.max.z
        })
    }

    /// A mover embedded in a brush must be pushed out, and must then actually
    /// move when swept — the bug was that an embedded mover stayed frozen
    /// because `segment_aabb` returns (t=0, ZERO normal) from inside a box.
    #[test]
    fn embedded_mover_is_freed_and_can_move() {
        // A wall brush; place the mover's centre well inside its expanded box.
        let wall = Aabb::from_corners(Vec3::new(-5.0, 0.0, 9.0), Vec3::new(5.0, 4.0, 9.8));
        let solids = [wall];
        let stuck = Vec3::new(0.0, 1.0, 9.4); // centre inside wall+half on every axis
        assert!(inside_any(stuck, HALF, &solids), "test setup: should start embedded");

        let freed = depenetrate(stuck, HALF, &solids);
        assert!(!inside_any(freed, HALF, &solids), "depenetrate must free the mover");

        // And a full swept move from the embedded start must now make progress
        // instead of returning the unchanged position.
        let res = move_and_slide(stuck, HALF, Vec3::new(0.0, 0.0, 5.0), 1.0 / 60.0, &solids, 0.5);
        assert!(
            res.pos.distance(stuck) > 1e-3,
            "embedded mover stayed frozen: {:?} -> {:?}",
            stuck,
            res.pos
        );
        assert!(!inside_any(res.pos, HALF, &solids));
    }

    /// Resting flush on a floor (separated by the SKIN gap the slide leaves)
    /// must NOT be perturbed — depenetrate only touches a strictly-inside centre.
    #[test]
    fn resting_contact_is_untouched() {
        let floor = Aabb::from_corners(Vec3::new(-10.0, -0.5, -10.0), Vec3::new(10.0, 0.0, 10.0));
        let solids = [floor];
        // Box bottom = centre.y - 0.9 sits SKIN above the floor top (y=0).
        let resting = Vec3::new(0.0, 0.9 + SKIN, 0.0);
        assert!(!inside_any(resting, HALF, &solids), "test setup: resting is not embedded");
        let after = depenetrate(resting, HALF, &solids);
        assert!(after.distance(resting) < 1e-6, "resting contact moved: {:?} -> {:?}", resting, after);
    }
}
