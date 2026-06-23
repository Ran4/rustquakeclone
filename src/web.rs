//! The Weaver's silk strands: author-placed WALKABLE bridges that exist only as
//! long as the spider anchoring them lives. Kill the Weaver and its strand drops,
//! letting the swept solver pull anything riding it (you, a chasing monster, the
//! spider) into the void for free.
//!
//! ## How it hooks into the rest of the engine — "the truck trick, generalised"
//!
//! The whole world collides against the flat list of axis-aligned brushes in
//! [`WorldColliders`]. A strand reserves a small RUN of slots in that list at
//! BUILD time (one per segment box) and the runtime system keeps them filled
//! while the Weaver lives — the exact moving-brush trick `Door`/`vehicle` use. So
//! the player's swept solver, the monsters' `move_and_slide`, hitscan and
//! line-of-sight all treat the strand as solid floor with no special-casing.
//! Severing writes a degenerate (far-away) AABB into every reserved slot, the
//! same disable a door does — never a `Vec::remove`, so indices stay stable.
//!
//! A near-horizontal strand is one thin box; a sloped run is approximated by a
//! short stair of stacked boxes whose centres step in Y along the line. The slot
//! count is reserved up front (`spawn_strand` pushes each box), so the indices
//! are fixed for the life of the built level — and the whole `solids` Vec is
//! rebuilt on the next level anyway, so the reservation only needs intra-level
//! stability.
//!
//! NOTE: shooting the strand bar itself to sever it is DEFERRED — killing the
//! anchoring Weaver is the primary, clean sever and the core of the mechanic.

use bevy::prelude::*;

use crate::common::*;
use crate::enemies::Enemy;
use crate::level::MonsterKind;
use crate::monster_model::Dying;
use crate::physics::Aabb;

// ----------------------------------------------------------------------------
// Strand tuning (meters).
// ----------------------------------------------------------------------------
/// Walkable width of the strand (cross-axis half is `STRAND_WIDTH / 2`).
const STRAND_WIDTH: f32 = 0.7;
/// Vertical thickness of each segment box (top sits at the anchor surface).
const STRAND_THICK: f32 = 0.25;
/// Target length of each segment box along the run; the count scales with this.
const STRAND_SEG_LEN: f32 = 1.2;
/// A Weaver within this radius of a strand's `owner_origin` adopts it.
const ADOPT_RADIUS: f32 = 2.5;
/// Cap on segment boxes per strand — keeps the flat `solids` slice small even if
/// a very long/steep run is authored.
const MAX_SEGMENTS: usize = 24;

// ----------------------------------------------------------------------------
// Components
// ----------------------------------------------------------------------------
/// A walkable silk strand: a row of reserved [`WorldColliders`] slots bridging
/// two anchors. Solid while its owner Weaver lives; cleared (riders drop) when
/// the Weaver dies.
#[derive(Component)]
pub struct Strand {
    /// First reserved collider slot (the run is `slot0 .. slot0 + count`).
    pub slot0: usize,
    /// Number of reserved slots (segment boxes).
    pub count: usize,
    /// Anchor A (top-surface point) — kept for visuals / a future bar-sever.
    pub a: Vec3,
    /// Anchor B.
    pub b: Vec3,
    /// The Weaver's spawn position, used by the one-shot position adoption.
    pub owner_origin: Vec3,
    /// The live Weaver entity once adopted (`None` until found).
    pub owner: Option<Entity>,
    /// Whether `owner` has been resolved yet.
    pub adopted: bool,
    /// Whether the strand has been cut (slots degenerate, visual gone).
    pub severed: bool,
    /// The silk-visual child entity, despawned on sever.
    pub visual: Entity,
}

// ----------------------------------------------------------------------------
// Plugin
// ----------------------------------------------------------------------------
pub struct WebPlugin;
impl Plugin for WebPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (strand_adopt, strand_update)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// A degenerate AABB parked a million metres away — nothing can reach it, so the
/// slot stops colliding/occluding without changing the Vec's length (the same
/// disable `Door`/`vehicle` use).
fn degenerate() -> Aabb {
    Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

/// Build the segment AABBs for a strand from anchor `a` to anchor `b`. Each box
/// is `STRAND_WIDTH` wide on the cross axes and `STRAND_THICK` tall, with its TOP
/// face at the lerped anchor height (so you walk straight onto it). A sloped run
/// steps the box top in Y per segment — a stacked-AABB stair.
fn segment_boxes(a: Vec3, b: Vec3) -> Vec<Aabb> {
    let len = a.distance(b);
    let count = ((len / STRAND_SEG_LEN).ceil() as usize).clamp(1, MAX_SEGMENTS);
    // Cross-section half-extents on the horizontal axes. Orient the long axis
    // (the wider half) along whichever horizontal direction the run dominates so
    // the walkway reads as a plank, not a row of square tiles.
    let run = b - a;
    let along_x = run.x.abs() >= run.z.abs();
    let seg = len / count as f32;
    let half_long = (seg * 0.5).max(STRAND_WIDTH * 0.5);
    let half_w = STRAND_WIDTH * 0.5;
    let (hx, hz) = if along_x { (half_long, half_w) } else { (half_w, half_long) };
    let mut boxes = Vec::with_capacity(count);
    for i in 0..count {
        let t = (i as f32 + 0.5) / count as f32;
        let c = a.lerp(b, t);
        // Top face at the surface height; the box hangs `STRAND_THICK` below it.
        let center = Vec3::new(c.x, c.y - STRAND_THICK * 0.5, c.z);
        let half = Vec3::new(hx, STRAND_THICK * 0.5, hz);
        boxes.push(Aabb::from_center_half(center, half));
    }
    boxes
}

/// Reserve the strand's collider slots (build time) and spawn its silk visual +
/// the [`Strand`] entity. Call from the level `build` so the indices stay stable.
/// `owner_origin` is the Weaver's spawn position (place it at/near anchor `a`).
pub fn spawn_strand(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    colliders: &mut Vec<Aabb>,
    owner_origin: Vec3,
    a: Vec3,
    b: Vec3,
) {
    let boxes = segment_boxes(a, b);
    // Reserve a contiguous run of slots — capture the index BEFORE pushing, then
    // push exactly `count` boxes, so `slot0 .. slot0 + count` permanently address
    // this strand (mirrors Door/vehicle reservation).
    let slot0 = colliders.len();
    let count = boxes.len();
    for bx in &boxes {
        colliders.push(*bx);
    }

    // Silk visual: a faint, slightly-emissive pale cylinder spanning a -> b. A
    // child of the Strand entity so it despawns when the strand is cut.
    let silk = materials.add(StandardMaterial {
        base_color: rgb(0.82, 0.84, 0.9).with_alpha(0.85),
        emissive: LinearRgba::rgb(0.25, 0.3, 0.35),
        perceptual_roughness: 0.6,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let len = a.distance(b).max(0.05);
    let dir = (b - a).normalize_or_zero();
    // A thin walkway slab sitting just at the surface, plus the silk cable.
    let mid_top = (a + b) * 0.5;
    let walk_mat = materials.add(StandardMaterial {
        base_color: rgb(0.6, 0.62, 0.7).with_alpha(0.6),
        emissive: LinearRgba::rgb(0.12, 0.14, 0.18),
        perceptual_roughness: 0.7,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let cable = meshes.add(Cylinder { radius: 0.06, half_height: len * 0.5 });
    let plank = meshes.add(Cuboid::from_size(Vec3::new(STRAND_WIDTH, STRAND_THICK * 0.6, len)));

    let visual = commands
        .spawn((
            Transform::from_translation(mid_top)
                .looking_to(if dir == Vec3::ZERO { Vec3::NEG_Z } else { dir }, Vec3::Y),
            Visibility::Visible,
            LevelEntity,
            Name::new("Strand visual"),
        ))
        .with_children(|c| {
            // Walkway plank (oriented along the run via the parent's looking_to).
            c.spawn((
                Mesh3d(plank),
                MeshMaterial3d(walk_mat),
                Transform::from_xyz(0.0, -STRAND_THICK * 0.3, 0.0),
            ));
            // The taut silk cable: a cylinder's axis is local +Y, so lay it along
            // the run (local -Z) by rotating 90 deg about X.
            c.spawn((
                Mesh3d(cable),
                MeshMaterial3d(silk),
                Transform::from_xyz(0.0, 0.04, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
        })
        .id();

    commands.spawn((
        Strand {
            slot0,
            count,
            a,
            b,
            owner_origin,
            owner: None,
            adopted: false,
            severed: false,
            visual,
        },
        LevelEntity,
        Name::new("Strand"),
    ));
}

/// Bind each not-yet-adopted strand to the nearest live Weaver within
/// `ADOPT_RADIUS` of its `owner_origin`. Monsters spawn in the OnEnter chain, so
/// this just polls each frame until the spider exists.
fn strand_adopt(
    mut q_strand: Query<&mut Strand>,
    q_enemy: Query<(Entity, &Transform, &Enemy)>,
) {
    for mut s in &mut q_strand {
        if s.adopted || s.severed {
            continue;
        }
        let mut best: Option<(Entity, f32)> = None;
        for (e, tf, en) in &q_enemy {
            if en.kind != MonsterKind::Weaver {
                continue;
            }
            let d = tf.translation.distance(s.owner_origin);
            if d <= ADOPT_RADIUS && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((e, d));
            }
        }
        if let Some((e, _)) = best {
            s.owner = Some(e);
            s.adopted = true;
        }
    }
}

/// Keep each strand solid while its owner Weaver lives; sever it (drop the slots,
/// despawn the silk) the instant the Weaver dies or is gone.
fn strand_update(
    mut colliders: ResMut<WorldColliders>,
    mut commands: Commands,
    mut q_strand: Query<&mut Strand>,
    q_owner: Query<(&Health, Has<Dying>), With<Enemy>>,
    mut sfx: MessageWriter<Sfx>,
) {
    for mut s in &mut q_strand {
        if s.severed {
            // Belt-and-braces: keep the slots disabled.
            for i in s.slot0..s.slot0 + s.count {
                if let Some(slot) = colliders.solids.get_mut(i) {
                    *slot = degenerate();
                }
            }
            continue;
        }

        // Sever once the owner is dead/gone. An un-adopted strand stays solid
        // (acceptable per the spec) — only an *adopted* owner that has since died
        // cuts the bridge.
        let dead = if let Some(owner) = s.owner {
            match q_owner.get(owner) {
                Ok((hp, dying)) => hp.dead || dying,
                Err(_) => true, // entity despawned (overkill gib) -> gone
            }
        } else {
            false
        };

        if dead {
            let mid = (s.a + s.b) * 0.5;
            for i in s.slot0..s.slot0 + s.count {
                if let Some(slot) = colliders.solids.get_mut(i) {
                    *slot = degenerate();
                }
            }
            if let Ok(mut ec) = commands.get_entity(s.visual) {
                ec.despawn();
            }
            sfx.write(Sfx::at(Sound::Sever, mid));
            s.severed = true;
        } else {
            // Owner alive (or not yet adopted): keep the segment boxes written so
            // the bridge stays solid. Cheap and matches the door/truck pattern.
            let boxes = segment_boxes(s.a, s.b);
            for (k, bx) in boxes.iter().enumerate() {
                if let Some(slot) = colliders.solids.get_mut(s.slot0 + k) {
                    *slot = *bx;
                }
            }
        }
    }
}
