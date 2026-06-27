//! Persistent carnage decals (feature 55): the world keeps a ledger of violence.
//!
//! Where a monster dies, blood pools the floor (and splats the wall behind it);
//! where a rocket/grenade detonates against a brush, a black scorch ring sears the
//! face it kissed; where a hitscan pellet/nail misses into the world, a small pock
//! chips the surface; and where a shoved/rammed corpse scrapes along the floor, a wet
//! drag-smear streaks behind it. Unlike monster wound-decay these mark the WORLD — the
//! geometry stays whole but a cleared arena reads the fight when you backtrack.
//!
//! Every surface normal the world hands back is CARDINAL (the whole map is
//! axis-aligned AABB brushes), so a decal is just a thin flat quad laid flush to
//! that normal a few mm off the face to beat z-fighting — no arbitrary-angle
//! projection needed. Decals live for the life of the level (they're `LevelEntity`,
//! swept on the exit-slipgate transition) and are pooled in a fixed RING BUFFER:
//! spawning past the configurable cap recycles the OLDEST decal, so a marathon
//! Death Knight brawl on the level-8 dam stays O(cap), never O(kills).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::common::*;
use crate::physics::{raycast_world, Aabb};

/// How far (m) off the brush face a decal sits along its cardinal normal. A few mm
/// — enough to clear the `physics::SKIN` resting gap and beat z-fighting, small
/// enough that the stain still reads as flush to the surface. Backed up by the
/// material's `depth_bias`.
const DECAL_OFFSET: f32 = 0.012;

/// Base radii (m) of each decal kind before per-instance jitter.
const BLOOD_POOL_RADIUS: f32 = 0.7; // floor pool under a corpse
const BLOOD_SPLAT_RADIUS: f32 = 0.45; // wall splat behind a corpse
const POCK_RADIUS: f32 = 0.14; // a single chewed pellet/nail pock
/// How close (m) a fresh pock may land to the previous one before it's dropped as a
/// duplicate. The Lightning Gun is continuous hitscan (cooldown 0.06s ≈ 16 fire
/// ticks/sec) and dwelling its beam on a wall — an explicitly intended action (the
/// feature-29 resonance shortcut) — writes a near-zero-spread world impact every
/// tick, which would otherwise stamp ~16 stacked pocks/sec on one point: a z-fighting
/// blob that churns the whole ring and evicts the blood/scorch this feature exists to
/// keep. This guard collapses that stack to one pock while staying well under a
/// shotgun's pellet spread (≈0.18m across at typical range), so a missed blast still
/// reads as a tight cluster.
const POCK_DEDUP_DIST: f32 = 0.1;
/// Base half-extents (m) of a corpse drag-smear: long along travel, narrow across.
const SMEAR_LEN: f32 = 0.55;
const SMEAR_WIDTH: f32 = 0.22;
/// How far (m) a wall splat probe reaches from the death point before giving up.
const WALL_SPLAT_REACH: f32 = 1.6;
/// Cap (m) on the radius a scorch ring is laid at, so a huge blast doesn't paint a
/// dinner-plate; and the max distance the scorch face-probe reaches from the blast.
const SCORCH_MAX_RADIUS: f32 = 2.5;
const SCORCH_PROBE_REACH: f32 = 3.0;

pub struct DecalsPlugin;
impl Plugin for DecalsPlugin {
    fn build(&self, app: &mut App) {
        // Blood-on-death is laid from `combat::check_deaths` directly (it owns the
        // death point); the scorch + pock decals hang off the explosion/impact
        // messages those weapons already fire.
        app.add_systems(
            Update,
            (decal_on_explosion, decal_on_impact).run_if(in_state(GameState::Playing)),
        );
    }
}

/// Marker for a carnage decal quad (also tagged `LevelEntity`, so the level
/// teardown despawns it wholesale on the exit-slipgate transition).
#[derive(Component)]
pub struct Decal;

/// The decal pool: a fixed ring buffer of quad entities plus the shared mesh and
/// the three theme-tinted materials every decal draws with. Spawning past `cap`
/// recycles the oldest entity (despawn + overwrite its ring slot), so the live
/// decal count is hard-capped regardless of how long a fight drags on.
#[derive(Resource)]
pub struct Decals {
    /// Live decal entities, oldest-first until the ring fills, then round-robined.
    ring: Vec<Entity>,
    /// Next ring slot to overwrite once `ring.len() == cap` (the oldest decal).
    next: usize,
    /// Hard ceiling on live decals (config `decal_limit`, default 2000). `0`
    /// disables decals entirely (the spawn helpers early-out).
    cap: usize,
    /// Shared unit disc mesh (radius 0.5, XY plane, +Z normal), oriented + scaled
    /// per instance. Rebuilt each level alongside the tinted materials.
    disc: Handle<Mesh>,
    blood_mat: Handle<StandardMaterial>,
    scorch_mat: Handle<StandardMaterial>,
    pock_mat: Handle<StandardMaterial>,
    /// `false` until `setup_decals` has built the mesh/materials for this level —
    /// guards spawns that might fire before the pool is dressed.
    ready: bool,
    /// Tiny xorshift state for per-instance spin + size jitter (so decals don't
    /// visibly tile). Kept here so the pool is self-contained and deterministic.
    rng: u32,
    /// World position of the last pock laid, for the spatial dedup that stops a held
    /// Lightning beam from stacking pocks on one point. Reset per level.
    last_pock: Option<Vec3>,
}

impl Decals {
    /// A pool with the configured cap and undressed handles — `setup_decals` fills
    /// the mesh/materials on entering a level.
    pub fn new(cap: usize) -> Self {
        Self {
            ring: Vec::new(),
            next: 0,
            cap,
            disc: Handle::default(),
            blood_mat: Handle::default(),
            scorch_mat: Handle::default(),
            pock_mat: Handle::default(),
            ready: false,
            rng: 0x1234_5678,
            last_pock: None,
        }
    }

    /// Spatial dedup gate for pocks: admit a pock only if it lands farther than
    /// [`POCK_DEDUP_DIST`] from the previous one, remembering it when admitted. This
    /// is what keeps a continuous beam (or a tight nail stream) on one wall point from
    /// stamping a churning stack of overlapping pocks. Split out + `Commands`-free so
    /// it's unit-testable.
    fn admit_pock(&mut self, pos: Vec3) -> bool {
        if let Some(last) = self.last_pock {
            if pos.distance_squared(last) < POCK_DEDUP_DIST * POCK_DEDUP_DIST {
                return false;
            }
        }
        self.last_pock = Some(pos);
        true
    }

    /// xorshift32 → [0,1).
    fn rand(&mut self) -> f32 {
        let mut x = self.rng | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32
    }

    /// Record a freshly-spawned decal in the ring, returning the OLD entity this
    /// push evicted (the caller despawns it) or `None` while the ring is still
    /// filling below the cap. This IS the recycle policy: below `cap` the ring just
    /// grows; at `cap` it round-robins from `next`, overwriting the oldest slot —
    /// so the live count is pinned at `cap` and a marathon fight stays O(cap).
    /// Split out and `Commands`-free so the logic is unit-testable.
    fn push(&mut self, e: Entity) -> Option<Entity> {
        if self.cap == 0 {
            return Some(e); // decals disabled: caller despawns immediately
        }
        if self.ring.len() < self.cap {
            self.ring.push(e);
            None
        } else {
            let old = std::mem::replace(&mut self.ring[self.next], e);
            self.next = (self.next + 1) % self.cap;
            Some(old)
        }
    }

    /// Lay one decal quad flush to `normal` at `pos`, with a random spin about the
    /// normal and a small size jitter, then file it in the ring (despawning
    /// whatever it recycles). `radius` is the pre-jitter half-width; `jitter` is the
    /// fractional size wobble (0.3 = ±30%).
    fn lay(&mut self, commands: &mut Commands, mat: Handle<StandardMaterial>, pos: Vec3, normal: Vec3, radius: f32, jitter: f32) {
        if !self.ready || self.cap == 0 {
            return;
        }
        let n = normal.normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        let spin = self.rand() * std::f32::consts::TAU;
        let scale = (radius * 2.0) * (1.0 + (self.rand() * 2.0 - 1.0) * jitter);
        let diameter = scale.max(0.05);
        // Orient the disc's +Z normal onto the cardinal face normal, then spin it
        // about that normal so successive stains don't share an alignment.
        let rot = Quat::from_axis_angle(n, spin) * quat_from_arc(Vec3::Z, n);
        let tf = Transform {
            translation: pos + n * DECAL_OFFSET,
            rotation: rot,
            scale: Vec3::new(diameter, diameter, 1.0),
        };
        let e = commands
            .spawn((Mesh3d(self.disc.clone()), MeshMaterial3d(mat), tf, Decal, LevelEntity))
            .id();
        if let Some(old) = self.push(e) {
            commands.entity(old).despawn();
        }
    }

    /// Pellet/nail pock where a hitscan/projectile chewed the WORLD (feature 55,
    /// source (c)). One small chip per world impact — they cluster naturally into a
    /// spread where a shotgun blast missed.
    pub fn pock(&mut self, commands: &mut Commands, pos: Vec3, normal: Vec3) {
        if !self.admit_pock(pos) {
            return;
        }
        let mat = self.pock_mat.clone();
        self.lay(commands, mat, pos, normal, POCK_RADIUS, 0.4);
    }

    /// Wet drag-smear a shoved/rammed corpse leaves as it scrapes along the floor
    /// (feature 55, source (d)): a short blood streak elongated ALONG `dir` (the
    /// body's horizontal slide this frame), laid flush on the floor at `pos`. Driven
    /// from the ragdoll scrape cue in `corpse.rs`, so a truck-ram (which dumps a big
    /// knockback impulse into the body) leaves the long ugly smears the spec promises.
    pub fn smear(&mut self, commands: &mut Commands, pos: Vec3, dir: Vec3) {
        if !self.ready || self.cap == 0 {
            return;
        }
        let n = Vec3::Y;
        let horiz = Vec3::new(dir.x, 0.0, dir.z);
        if horiz.length_squared() < 1.0e-6 {
            return;
        }
        let horiz = horiz.normalize();
        // Lay the disc flat on the floor, then spin it about the floor normal so its
        // local +X (the long axis once we scale) runs along the slide direction.
        let flat = quat_from_arc(Vec3::Z, n);
        let local_x = flat * Vec3::X;
        let mut angle = local_x.angle_between(horiz);
        if local_x.cross(horiz).dot(n) < 0.0 {
            angle = -angle;
        }
        let rot = Quat::from_axis_angle(n, angle) * flat;
        let len = SMEAR_LEN * (1.0 + (self.rand() * 2.0 - 1.0) * 0.3);
        let wid = SMEAR_WIDTH * (1.0 + (self.rand() * 2.0 - 1.0) * 0.3);
        let tf = Transform {
            translation: pos + n * DECAL_OFFSET,
            rotation: rot,
            scale: Vec3::new(len.max(0.05), wid.max(0.05), 1.0),
        };
        let mat = self.blood_mat.clone();
        let e = commands.spawn((Mesh3d(self.disc.clone()), MeshMaterial3d(mat), tf, Decal, LevelEntity)).id();
        if let Some(old) = self.push(e) {
            commands.entity(old).despawn();
        }
    }

    /// Scorch ring where a blast kissed a brush face (feature 55, source (b)).
    pub fn scorch(&mut self, commands: &mut Commands, pos: Vec3, normal: Vec3, radius: f32) {
        let mat = self.scorch_mat.clone();
        self.lay(commands, mat, pos, normal, radius, 0.25);
    }

    /// Blood stain on a monster death (feature 55, source (a)): a floor pool under
    /// the body (raycast straight down to the floor brush it falls onto) plus, if a
    /// wall is close behind, one smaller splat flung onto it. `solids` is
    /// `WorldColliders.solids`.
    pub fn death_stain(&mut self, commands: &mut Commands, solids: &[Aabb], pos: Vec3) {
        if !self.ready || self.cap == 0 {
            return;
        }
        // Floor pool: drop a ray from just above the death point onto the floor.
        if let Some((_, p, n)) = raycast_world(pos + Vec3::Y * 0.5, Vec3::NEG_Y, 6.0, solids) {
            if n.y > 0.5 {
                let mat = self.blood_mat.clone();
                self.lay(commands, mat, p, n, BLOOD_POOL_RADIUS, 0.3);
            }
        }
        // Wall splat: probe the four cardinals for a near wall; splat the first one.
        for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            if let Some((_, p, n)) = raycast_world(pos + Vec3::Y * 0.6, d, WALL_SPLAT_REACH, solids) {
                if n.y.abs() < 0.5 {
                    let mat = self.blood_mat.clone();
                    self.lay(commands, mat, p, n, BLOOD_SPLAT_RADIUS, 0.35);
                    break; // one wall splat is plenty
                }
            }
        }
    }
}

/// A `StandardMaterial` for a decal quad: double-sided so the stain shows whichever
/// way its cardinal normal faces, alpha-blended so the disc reads as a soft stain
/// over the brush texture (not a hard tile), and depth-biased toward the camera to
/// back up the few-mm geometric offset against z-fighting.
fn decal_mat(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        cull_mode: None,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 8.0,
        ..default()
    }
}

/// A filled unit disc (radius 0.5) in the XY plane facing +Z, centred at origin — a
/// triangle fan of `segments` wedges. Cheap, shared, oriented + scaled per instance.
/// Reads as a round splat far better than a square quad would.
fn disc_mesh(segments: usize) -> Mesh {
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(segments + 1);
    let mut nor: Vec<[f32; 3]> = Vec::with_capacity(segments + 1);
    let mut uv: Vec<[f32; 2]> = Vec::with_capacity(segments + 1);
    let mut idx: Vec<u32> = Vec::with_capacity(segments * 3);
    pos.push([0.0, 0.0, 0.0]);
    nor.push([0.0, 0.0, 1.0]);
    uv.push([0.5, 0.5]);
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        pos.push([c * 0.5, s * 0.5, 0.0]);
        nor.push([0.0, 0.0, 1.0]);
        uv.push([0.5 + c * 0.5, 0.5 + s * 0.5]);
    }
    for i in 0..segments as u32 {
        let a = 1 + i;
        let b = 1 + (i + 1) % segments as u32;
        idx.extend_from_slice(&[0, a, b]);
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

/// Rotation taking unit `from` onto unit `to`, robust to the antiparallel (180°)
/// case `glam`'s `from_rotation_arc` leaves undefined and to degenerate inputs.
/// (Cardinal-normal decals routinely need `Z → -Z`, exactly that 180° case.)
fn quat_from_arc(from: Vec3, to: Vec3) -> Quat {
    let (from, to) = (from.normalize_or_zero(), to.normalize_or_zero());
    if from == Vec3::ZERO || to == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let d = from.dot(to);
    if d > 0.999_99 {
        return Quat::IDENTITY;
    }
    if d < -0.999_99 {
        let mut axis = from.cross(Vec3::X);
        if axis.length_squared() < 1e-6 {
            axis = from.cross(Vec3::Y);
        }
        return Quat::from_axis_angle(axis.normalize(), std::f32::consts::PI);
    }
    Quat::from_rotation_arc(from, to)
}

/// Dress the decal pool for the level just built (runs after `setup_level` in the
/// OnEnter(Playing) chain). Drops the previous ring — its quads are already gone
/// with the old `LevelEntity` sweep, so we only clear the stale ids — and rebuilds
/// the shared mesh + the three materials tinted to this theme's decal palette.
pub fn setup_decals(
    mut decals: ResMut<Decals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    style: Res<LevelStyle>,
) {
    decals.ring.clear();
    decals.next = 0;
    decals.last_pock = None;
    decals.disc = meshes.add(disc_mesh(20));
    decals.blood_mat = materials.add(decal_mat(style.decal.blood));
    decals.scorch_mat = materials.add(decal_mat(style.decal.scorch));
    decals.pock_mat = materials.add(decal_mat(style.decal.pock));
    decals.ready = true;
}

/// Sear a scorch ring where a real detonation kissed a brush. `ExplosionEvent`
/// carries no surface normal, so probe outward from the blast centre to the
/// nearest brush face it touched and lay the scorch flush there. The Lodestone's
/// per-frame implode pulls (`implode: true`) are silent gravity, not detonations —
/// skipped, same as the fireball visual.
fn decal_on_explosion(
    mut decals: ResMut<Decals>,
    mut commands: Commands,
    colliders: Res<WorldColliders>,
    mut reader: MessageReader<ExplosionEvent>,
) {
    for ex in reader.read() {
        if ex.implode {
            continue;
        }
        if let Some((p, n)) = nearest_face(&colliders.solids, ex.pos, SCORCH_PROBE_REACH) {
            let r = (ex.radius * 0.5).clamp(0.6, SCORCH_MAX_RADIUS);
            decals.scorch(&mut commands, p, n, r);
        }
    }
}

/// Chip a pock where a hitscan/projectile struck the WORLD. Monster hits
/// (`blood: true`) are left to the death stain — they'd otherwise paint the air
/// where a body once stood.
fn decal_on_impact(
    mut decals: ResMut<Decals>,
    mut commands: Commands,
    mut reader: MessageReader<ImpactEvent>,
) {
    for ev in reader.read() {
        if !ev.blood {
            decals.pock(&mut commands, ev.pos, ev.normal);
        }
    }
}

/// Nearest brush face to `pos`, probed along the six cardinal directions out to
/// `max`. Returns the hit point + its (cardinal) normal — used to anchor a scorch
/// flush on whichever floor/wall/ceiling a blast hugged.
fn nearest_face(solids: &[Aabb], pos: Vec3, max: f32) -> Option<(Vec3, Vec3)> {
    let dirs = [Vec3::NEG_Y, Vec3::Y, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for d in dirs {
        if let Some((dist, p, n)) = raycast_world(pos, d, max, solids) {
            if best.map_or(true, |(bd, _, _)| dist < bd) {
                best = Some((dist, p, n));
            }
        }
    }
    best.map(|(_, p, n)| (p, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring buffer is a hard ceiling: pushing past the cap keeps the live count
    /// pinned at `cap` and evicts the OLDEST entity each time, in oldest-first order.
    #[test]
    fn ring_buffer_recycles_oldest_past_cap() {
        let cap = 4;
        let mut d = Decals::new(cap);
        let mut evicted = Vec::new();

        // Fill exactly to the cap: the ring grows, nothing is evicted.
        for i in 0..cap as u32 {
            if let Some(old) = d.push(Entity::from_raw_u32(i).unwrap()) {
                evicted.push(old);
            }
        }
        assert_eq!(d.ring.len(), cap, "ring fills to the cap");
        assert!(evicted.is_empty(), "no eviction while below the cap");

        // Push `cap` more: every push now evicts, live count never exceeds the cap.
        for i in cap as u32..cap as u32 * 2 {
            let old = d.push(Entity::from_raw_u32(i).unwrap()).expect("a full ring evicts on every push");
            evicted.push(old);
        }
        assert_eq!(d.ring.len(), cap, "live count stays pinned at the cap");

        // The evicted ones are exactly the first `cap` entities, oldest-first.
        let expect: Vec<Entity> = (0..cap as u32).map(|i| Entity::from_raw_u32(i).unwrap()).collect();
        assert_eq!(evicted, expect, "evicted oldest-first");

        // …and the ring now holds the newest `cap` entities.
        let live: std::collections::HashSet<Entity> = d.ring.iter().copied().collect();
        for i in cap as u32..cap as u32 * 2 {
            assert!(live.contains(&Entity::from_raw_u32(i).unwrap()), "newest entities are live");
        }
    }

    /// The pock spatial dedup collapses a beam dwelling on one point to a single
    /// pock, but still admits a pock laid clearly apart (a shotgun's spread cluster).
    #[test]
    fn pock_dedup_collapses_a_dwelt_beam() {
        let mut d = Decals::new(8);
        let wall = Vec3::new(2.0, 1.5, 0.0);
        assert!(d.admit_pock(wall), "the first pock is always admitted");
        // A near-zero-spread beam re-hits essentially the same point every tick.
        for _ in 0..16 {
            let jittered = wall + Vec3::new(0.0, 0.01, 0.01);
            assert!(!d.admit_pock(jittered), "a pock within the dedup distance is dropped");
        }
        // A pellet that landed clearly apart still chips its own pock.
        let apart = wall + Vec3::new(0.0, 0.3, 0.0);
        assert!(d.admit_pock(apart), "a pock beyond the dedup distance is admitted");
        // …and now dedups against THAT one.
        assert!(!d.admit_pock(apart + Vec3::new(0.0, 0.02, 0.0)), "dedup tracks the latest pock");
    }

    /// A zero cap disables the pool: every push is handed straight back for despawn
    /// and nothing is retained.
    #[test]
    fn zero_cap_retains_nothing() {
        let mut d = Decals::new(0);
        let e = Entity::from_raw_u32(7).unwrap();
        assert_eq!(d.push(e), Some(e), "cap 0 hands the entity straight back");
        assert!(d.ring.is_empty());
    }
}
