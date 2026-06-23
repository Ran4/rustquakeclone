//! Procedural skeletal monster models.
//!
//! Each monster is a small hierarchy of parented "bone" entities (pelvis -> torso
//! -> head/jaw, articulated arms & legs, plus
//! per-kind bits like the Ogre's chainsaw or the Scrag's wings & tail). Every part
//! is a low-poly mesh — tapered limbs, claws, faceted muscle masses — that is
//! flat-shaded and vertex-jittered for the angular, hand-modeled Quake look, then
//! skinned with an AI-generated texture. A procedural animation system drives the
//! bones every frame: a walk cycle, idle breathing, attack swings, pain flinch and
//! a death topple.
//!
//! The skeleton is the entity hierarchy itself — rotating a joint swings everything
//! parented to it, exactly like a real bone rig. No binary mesh assets required.

use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};

use crate::common::{tune::GRAVITY, *};
use crate::effects::{spawn_blood, spawn_gibs};
use crate::enemies::{kind_pitch, Enemy};
use crate::level::MonsterKind;
use crate::physics::{ray_aabb, Aabb};
use crate::player::PlayerCamera;

/// Limb world-AABBs are only rebuilt for monsters within this distance of the
/// camera (beyond it the broad body Hurtbox still lands hits — you just can't
/// surgically dismember a monster too far to aim a bone on).
const LIMB_BOX_MAX_DIST: f32 = 60.0;

/// The 8 corners of the unit cube, for transforming a local AABB to world.
const CORNERS: [Vec3; 8] = [
    Vec3::new(-1.0, -1.0, -1.0),
    Vec3::new(1.0, -1.0, -1.0),
    Vec3::new(-1.0, 1.0, -1.0),
    Vec3::new(1.0, 1.0, -1.0),
    Vec3::new(-1.0, -1.0, 1.0),
    Vec3::new(1.0, -1.0, 1.0),
    Vec3::new(-1.0, 1.0, 1.0),
    Vec3::new(1.0, 1.0, 1.0),
];

pub struct MonsterModelPlugin;
impl Plugin for MonsterModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonsterTextures>()
            .add_systems(Startup, load_monster_textures)
            .add_systems(Update, (animate_monsters, animate_death))
            // Rebuild the shared limb hitboxes once per frame before any weapon
            // reads them. (Runs in Update using last frame's propagated bone
            // GlobalTransforms — a cosmetically-irrelevant 1-frame pose lag; there
            // is no way to read same-frame propagation from Update.)
            .add_systems(
                Update,
                rebuild_limb_boxes
                    .before(crate::weapons::fire_weapon)
                    .before(crate::projectiles::projectile_move)
                    .before(crate::combat::handle_explosions)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, update_limb_gibs.run_if(in_state(GameState::Playing)));
    }
}

// ----------------------------------------------------------------------------
// Textures: shared albedo images, one set per monster + two shared (bone/metal).
// ----------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TexFile {
    GruntSkin,
    GruntArmor,
    EnforcerSkin,
    EnforcerArmor,
    KnightSteel,
    KnightMail,
    ScragFlesh,
    ScragMembrane,
    OgreHide,
    OgreApron,
    DkArmor,
    DkCloth,
    Bone,
    DemonMetal,
}
impl TexFile {
    pub fn file(self) -> &'static str {
        use TexFile::*;
        match self {
            GruntSkin => "textures/monsters/grunt_skin.png",
            GruntArmor => "textures/monsters/grunt_armor.png",
            EnforcerSkin => "textures/monsters/enforcer_skin.png",
            EnforcerArmor => "textures/monsters/enforcer_armor.png",
            KnightSteel => "textures/monsters/knight_steel.png",
            KnightMail => "textures/monsters/knight_mail.png",
            ScragFlesh => "textures/monsters/scrag_flesh.png",
            ScragMembrane => "textures/monsters/scrag_membrane.png",
            OgreHide => "textures/monsters/ogre_hide.png",
            OgreApron => "textures/monsters/ogre_apron.png",
            DkArmor => "textures/monsters/dk_armor.png",
            DkCloth => "textures/monsters/dk_cloth.png",
            Bone => "textures/monsters/bone.png",
            DemonMetal => "textures/monsters/demon_metal.png",
        }
    }
    pub fn all() -> [TexFile; 14] {
        use TexFile::*;
        [
            GruntSkin, GruntArmor, EnforcerSkin, EnforcerArmor, KnightSteel, KnightMail,
            ScragFlesh, ScragMembrane, OgreHide, OgreApron, DkArmor, DkCloth, Bone, DemonMetal,
        ]
    }
}

#[derive(Resource, Default)]
pub struct MonsterTextures {
    pub map: HashMap<TexFile, Handle<Image>>,
}
impl MonsterTextures {
    fn get(&self, f: TexFile) -> Option<Handle<Image>> {
        self.map.get(&f).cloned()
    }
}

fn load_monster_textures(asset_server: Res<AssetServer>, mut tex: ResMut<MonsterTextures>) {
    for f in TexFile::all() {
        tex.map.insert(f, asset_server.load(f.file()));
    }
}

/// The skin texture used for the slot-0 "SkinPrimary" role of each kind.
fn skin_of(kind: MonsterKind) -> TexFile {
    use MonsterKind::*;
    match kind {
        Grunt => TexFile::GruntSkin,
        Enforcer => TexFile::EnforcerSkin,
        Knight => TexFile::KnightSteel,
        Scrag => TexFile::ScragFlesh,
        Ogre => TexFile::OgreHide,
        DeathKnight => TexFile::DkArmor,
        Weaver => TexFile::OgreHide, // dark chitinous hide (reused, no new image)
    }
}
/// The detail/armor texture used for the slot-1 "ArmorSecondary" role of each kind.
fn armor_of(kind: MonsterKind) -> TexFile {
    use MonsterKind::*;
    match kind {
        Grunt => TexFile::GruntArmor,
        Enforcer => TexFile::EnforcerArmor,
        Knight => TexFile::KnightMail,
        Scrag => TexFile::ScragMembrane,
        Ogre => TexFile::OgreApron,
        DeathKnight => TexFile::DkCloth,
        Weaver => TexFile::ScragMembrane, // translucent web/membrane sheen (reused)
    }
}

// ----------------------------------------------------------------------------
// Rig description (data) — assembled into entities by `build_monster_visual`.
// ----------------------------------------------------------------------------
#[derive(Clone, Copy)]
pub enum Shape {
    /// Cuboid given as half-extents.
    Box(Vec3),
    /// Capsule: radius + cylinder length. Rendered as a faceted tapered limb.
    Capsule { r: f32, len: f32 },
    Sphere(f32),
    /// Cone: base radius + height (points toward +Y by default).
    Cone { r: f32, h: f32 },
    /// Tapered faceted limb (frustum): radius `r0` at the top joint -> `r1` at the
    /// far end, total length `len` along local Y. The Quake-style muscular limb.
    Limb { r0: f32, r1: f32, len: f32 },
    /// A claw / horn / fang: a curved tapering spike of length `len`, base radius
    /// `r`, curling by `bend` meters toward +Z along its length.
    Claw { r: f32, len: f32, bend: f32 },
}

/// Which texture a part is skinned with (relative to the monster).
#[derive(Clone, Copy)]
pub enum TexId {
    SkinPrimary,
    ArmorSecondary,
    Bone,
    DemonMetal,
    /// No texture — an unlit, emissive part (e.g. glowing eyes).
    None,
}

/// How a bone moves under the procedural animator.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AnimRole {
    Static,
    Pelvis,
    Torso,
    Chest,
    Head,
    Jaw,
    ThighL,
    ThighR,
    ShinL,
    ShinR,
    FootL,
    FootR,
    UpperArmL,
    UpperArmR,
    ForearmL,
    ForearmR,
    WeaponArm,
    WingL,
    WingR,
    Tail,
    Cape,
}

/// One bone in a monster rig. `pos`/`rot_deg` are local to the parent joint.
#[derive(Clone, Copy)]
pub struct BoneSpec {
    pub name: &'static str,
    pub parent: Option<&'static str>,
    pub shape: Shape,
    pub pos: Vec3,
    pub mesh_off: Vec3,
    pub rot_deg: Vec3,
    pub tex: TexId,
    pub tint: [f32; 3],
    pub emissive: [f32; 3],
    pub anim: AnimRole,
}

/// Component on every bone joint; drives the procedural animation.
#[derive(Component)]
pub struct Bone {
    pub owner: Entity,
    pub role: AnimRole,
    pub base_rot: Quat,
    pub base_pos: Vec3,
}

/// All per-instance part materials of a monster (with their base emissive), so a
/// hit-flash can pop every part and restore it afterwards.
#[derive(Component)]
pub struct MonsterMats(pub Vec<(Handle<StandardMaterial>, LinearRgba)>);

/// Marks a monster that is dying — it topples over, then despawns.
#[derive(Component)]
pub struct Dying {
    pub t: f32,
    pub yaw: f32,
}

// ----------------------------------------------------------------------------
// Dismemberment: per-bone hurtboxes, limb pools, sever + gibs.
// ----------------------------------------------------------------------------

/// On every bone joint (alongside `Bone`). Carries the bone's limb group and its
/// LOCAL-space hit half-extents + center, so the per-frame world-AABB build is
/// pure transform math (no mesh access at runtime).
#[derive(Component, Clone, Copy)]
pub struct BoneHurt {
    pub group: LimbGroup,
    pub local_center: Vec3,
    pub local_half: Vec3,
}

/// On the subtree-root bone of each SEVERABLE group (one per present group).
/// Lets `do_sever` hide the whole subtree and clone a gib in one op.
#[derive(Component)]
pub struct LimbRoot {
    pub owner: Entity,
    pub group: LimbGroup,
    pub mesh: Option<Handle<Mesh>>,
}

/// Marker on a severed bone (and its descendants): excluded from `animate_monsters`
/// and the limb-box rebuild.
#[derive(Component)]
pub struct Severed;

/// Per-group sever pool, on the Enemy root. `max[g]==0` => the kind has no such
/// group (or it's the never-severing torso).
#[derive(Component)]
pub struct Limbs {
    pub max: [f32; LimbGroup::COUNT],
    pub taken: [f32; LimbGroup::COUNT],
    pub severed: [bool; LimbGroup::COUNT],
}
impl Limbs {
    pub fn is_severed(&self, g: LimbGroup) -> bool {
        self.severed[g.idx()]
    }
}

/// Cripple flags the AI/anim read, on the Enemy root. Derived at sever time so the
/// AI never scans the pool array.
#[derive(Component, Default, Clone, Copy)]
pub struct Crippled {
    pub disarmed: bool, // weapon (right) arm gone
    pub legs_lost: u8,  // 0 / 1 / 2
    pub head_gone: bool, // boss only (normals die outright on head sever)
    pub grounded: bool, // Scrag wing gone
}

/// A detached limb tumbling as a gib (separate, unparented world entity).
#[derive(Component)]
pub struct LimbGib {
    pub vel: Vec3,
    pub spin: Vec3,
    pub life: f32,
}

/// Conservative local-space half-extents of a bone shape (encloses the mesh).
fn shape_half(s: &Shape) -> Vec3 {
    match *s {
        Shape::Box(h) => h,
        Shape::Sphere(r) => Vec3::splat(r),
        Shape::Cone { r, h } => Vec3::new(r, h * 0.5, r),
        Shape::Capsule { r, len } => Vec3::new(r, len * 0.5 + r, r),
        Shape::Limb { r0, r1, len } => {
            let r = r0.max(r1);
            Vec3::new(r, len * 0.5, r)
        }
        Shape::Claw { r, len, bend } => Vec3::new(r, len * 0.5, r + bend.abs()),
    }
}

/// The group a bone *seeds* from its own role. `Torso` here means "inherit from
/// my parent" (resolved by the structural walk in `build_monster_visual`), since
/// most weapon-arm subtree bones are tagged `Static`.
fn group_of(role: AnimRole) -> LimbGroup {
    use AnimRole::*;
    use LimbGroup as G;
    match role {
        Head | Jaw => G::Head,
        ThighL | ShinL | FootL => G::LegL,
        ThighR | ShinR | FootR => G::LegR,
        UpperArmL | ForearmL => G::ArmL,
        UpperArmR | ForearmR | WeaponArm => G::ArmR,
        WingL | WingR => G::WingL,
        Pelvis | Torso | Chest | Tail | Cape | Static => G::Torso,
    }
}

/// Damage required to sever a group, per kind (tuned ~0.45-0.6x body HP so chip
/// damage won't dismember but focused fire will). 0 = no such group / not severable.
fn limb_max(kind: MonsterKind, g: LimbGroup) -> f32 {
    use LimbGroup::*;
    use MonsterKind::*;
    match (kind, g) {
        (Grunt, Head) => 16.0,
        (Grunt, ArmL) => 16.0,
        (Grunt, ArmR) => 14.0,
        (Grunt, LegL | LegR) => 18.0,
        (Enforcer, Head) => 28.0,
        (Enforcer, ArmL) => 26.0,
        (Enforcer, ArmR) => 24.0,
        (Enforcer, LegL | LegR) => 30.0,
        (Knight, Head) => 30.0,
        (Knight, ArmL) => 26.0,
        (Knight, ArmR) => 28.0,
        (Knight, LegL | LegR) => 32.0,
        (Scrag, Head) => 24.0,
        (Scrag, ArmL) => 22.0,
        (Scrag, ArmR) => 22.0,
        (Scrag, WingL) => 20.0,
        (Ogre, Head) => 110.0,
        (Ogre, ArmL) => 90.0,
        (Ogre, ArmR) => 80.0,
        (Ogre, LegL | LegR) => 100.0,
        (DeathKnight, Head) => 180.0,
        (DeathKnight, ArmL) => 140.0,
        (DeathKnight, ArmR) => 130.0,
        (DeathKnight, LegL | LegR) => 150.0,
        _ => 0.0,
    }
}

/// Pour `dmg` into a limb pool. Returns true EXACTLY once — the hit that empties
/// it (so a sever fires a single time). Pure + headless-testable.
pub fn accrue(taken: &mut f32, max: f32, severed: &mut bool, dmg: f32) -> bool {
    if *severed || max <= 0.0 {
        return false;
    }
    *taken += dmg.max(0.0);
    if *taken >= max {
        *severed = true;
        true
    } else {
        false
    }
}

/// Move-speed multiplier from how many legs are gone (0 -> full, 1 -> half, 2 -> crawl).
pub fn cripple_speed(legs_lost: u8) -> f32 {
    match legs_lost {
        0 => 1.0,
        1 => 0.5,
        _ => 0.15,
    }
}

/// Nearest limb-box hit along a ray within `max_t`. Pure (no ECS). `pad` grows
/// each box (used by the melee whip's forgiving reach).
pub fn nearest_limb_hit(
    origin: Vec3,
    dir: Vec3,
    max_t: f32,
    boxes: &[(Entity, LimbGroup, Aabb)],
    pad: f32,
) -> Option<(Entity, LimbGroup, f32, Vec3)> {
    let mut best_t = max_t;
    let mut best = None;
    for &(e, g, bb) in boxes {
        let b = if pad > 0.0 { bb.expand(Vec3::splat(pad)) } else { bb };
        if let Some((t, n)) = ray_aabb(origin, dir, best_t, &b) {
            best_t = t;
            best = Some((e, g, t, n));
        }
    }
    best
}

// ----------------------------------------------------------------------------
// Building the entity hierarchy from a rig.
// ----------------------------------------------------------------------------
fn make_mesh(meshes: &mut Assets<Mesh>, shape: &Shape) -> Handle<Mesh> {
    // Build a low-poly base, then flat-shade it (per-face normals) and jitter the
    // vertices a touch — that's the angular, faceted, hand-modeled Quake look.
    let (base, amp) = match *shape {
        Shape::Box(h) => (Mesh::from(Cuboid::from_size(h * 2.0)), 0.12 * h.min_element()),
        // A capsule becomes a tapered, faceted limb (thick at the joint, lean at the end).
        Shape::Capsule { r, len } => (tapered_prism(r * 1.05, r * 0.6, len + 2.0 * r, 7), 0.18 * r),
        Shape::Sphere(r) => (uv_sphere(r, 9, 6), 0.13 * r),
        Shape::Cone { r, h } => (low_cone(r, h, 7), 0.1 * r),
        Shape::Limb { r0, r1, len } => (tapered_prism(r0, r1, len, 7), 0.18 * r0),
        Shape::Claw { r, len, bend } => (claw_mesh(r, len, bend, 6), 0.08 * r),
    };
    let mut m = faceted(base);
    jitter(&mut m, amp);
    m.compute_flat_normals();
    meshes.add(m)
}

/// Flat-shade a mesh: give every triangle its own vertices and per-face normals,
/// so each facet reads as a hard low-poly plane (no smooth balloon shading).
fn faceted(mut m: Mesh) -> Mesh {
    m.duplicate_vertices();
    m.compute_flat_normals();
    m
}

/// Deterministic per-vertex hash in [0,1) from the exact vertex position bits, so
/// coincident (duplicated) vertices get the *same* offset and the mesh stays watertight.
fn vhash(x: f32, y: f32, z: f32, c: u32) -> f32 {
    let mut h = x
        .to_bits()
        .wrapping_mul(0x9e37_79b1)
        ^ y.to_bits().wrapping_mul(0x85eb_ca77)
        ^ z.to_bits().wrapping_mul(0xc2b2_ae3d)
        ^ c.wrapping_mul(0x27d4_eb2f);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Nudge each vertex by a small hash-based offset for an organic, hand-modeled feel.
fn jitter(m: &mut Mesh, amp: f32) {
    if amp <= 0.0 {
        return;
    }
    let Some(VertexAttributeValues::Float32x3(ps)) = m.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    for p in ps.iter_mut() {
        p[0] += (vhash(p[0], p[1], p[2], 0) - 0.5) * 2.0 * amp;
        p[1] += (vhash(p[0], p[1], p[2], 1) - 0.5) * 2.0 * amp;
        p[2] += (vhash(p[0], p[1], p[2], 2) - 0.5) * 2.0 * amp;
    }
}

/// Lathe a stack of rings into a closed surface. Each ring is `[y, radius, x_off,
/// z_off]`; consecutive rings are bridged by faceted quads and the end rings are
/// capped. Used to build limbs, cones and claws. Winding is irrelevant — monster
/// materials are double-sided.
fn lathe(rings: &[[f32; 4]], sides: u32) -> Mesh {
    let sides = sides.max(3) as usize;
    let cols = sides + 1;
    let nr = rings.len();
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for (ri, r) in rings.iter().enumerate() {
        let (y, rad, xo, zo) = (r[0], r[1], r[2], r[3]);
        let v = ri as f32 / (nr.max(2) - 1) as f32;
        for j in 0..=sides {
            let a = j as f32 / sides as f32 * TAU;
            pos.push([xo + rad * a.cos(), y, zo + rad * a.sin()]);
            uv.push([j as f32 / sides as f32, v]);
        }
    }
    for ri in 0..nr - 1 {
        let b0 = ri * cols;
        let b1 = (ri + 1) * cols;
        for j in 0..sides {
            let (a, b, c, d) = ((b0 + j) as u32, (b0 + j + 1) as u32, (b1 + j) as u32, (b1 + j + 1) as u32);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // End caps (skip if the ring is effectively a point).
    if rings[0][1] > 0.03 {
        let c = pos.len() as u32;
        pos.push([rings[0][2], rings[0][0], rings[0][3]]);
        uv.push([0.5, 0.5]);
        for j in 0..sides as u32 {
            idx.extend_from_slice(&[c, j, j + 1]);
        }
    }
    if rings[nr - 1][1] > 0.03 {
        let li = ((nr - 1) * cols) as u32;
        let c = pos.len() as u32;
        pos.push([rings[nr - 1][2], rings[nr - 1][0], rings[nr - 1][3]]);
        uv.push([0.5, 0.5]);
        for j in 0..sides as u32 {
            idx.extend_from_slice(&[c, li + j, li + j + 1]);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

/// Tapered faceted limb centered on the origin, spanning local Y in [-len/2, +len/2].
fn tapered_prism(r0: f32, r1: f32, len: f32, sides: u32) -> Mesh {
    lathe(&[[len * 0.5, r0, 0.0, 0.0], [-len * 0.5, r1, 0.0, 0.0]], sides)
}

/// Low-poly cone, apex toward +Y, centered on the origin.
fn low_cone(r: f32, h: f32, sides: u32) -> Mesh {
    lathe(&[[h * 0.5, 0.0, 0.0, 0.0], [-h * 0.5, r, 0.0, 0.0]], sides)
}

/// A curling claw/horn: wide base at +Y, tapering to a point at -Y, curling +Z.
fn claw_mesh(r: f32, len: f32, bend: f32, sides: u32) -> Mesh {
    lathe(
        &[
            [len * 0.5, r, 0.0, 0.0],
            [0.0, r * 0.5, 0.0, bend * 0.45],
            [-len * 0.5, r * 0.12, 0.0, bend],
        ],
        sides,
    )
}

/// Chunky low-poly UV sphere (good UVs for texturing, faceted once flat-shaded).
fn uv_sphere(r: f32, slices: u32, stacks: u32) -> Mesh {
    let slices = slices.max(3) as usize;
    let stacks = stacks.max(2) as usize;
    let cols = slices + 1;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for i in 0..=stacks {
        let phi = i as f32 / stacks as f32 * PI;
        let (y, ring) = (r * phi.cos(), r * phi.sin());
        for j in 0..=slices {
            let a = j as f32 / slices as f32 * TAU;
            pos.push([ring * a.cos(), y, ring * a.sin()]);
            uv.push([j as f32 / slices as f32, i as f32 / stacks as f32]);
        }
    }
    for i in 0..stacks {
        for j in 0..slices {
            let (a, b, c, d) = (
                (i * cols + j) as u32,
                (i * cols + j + 1) as u32,
                ((i + 1) * cols + j) as u32,
                ((i + 1) * cols + j + 1) as u32,
            );
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

fn resolve_tex(kind: MonsterKind, tex: TexId) -> Option<TexFile> {
    match tex {
        TexId::SkinPrimary => Some(skin_of(kind)),
        TexId::ArmorSecondary => Some(armor_of(kind)),
        TexId::Bone => Some(TexFile::Bone),
        TexId::DemonMetal => Some(TexFile::DemonMetal),
        TexId::None => None,
    }
}

fn make_material(
    materials: &mut Assets<StandardMaterial>,
    tex_res: &MonsterTextures,
    kind: MonsterKind,
    spec: &BoneSpec,
) -> (Handle<StandardMaterial>, LinearRgba) {
    let tint = Color::srgb(spec.tint[0], spec.tint[1], spec.tint[2]);
    let emissive = LinearRgba::rgb(spec.emissive[0], spec.emissive[1], spec.emissive[2]);
    let unlit = matches!(spec.tex, TexId::None);
    let tex_handle = resolve_tex(kind, spec.tex).and_then(|f| tex_res.get(f));
    // Metallic-ish parts (armor/blades) look better a touch shinier.
    let (rough, metal) = match spec.tex {
        TexId::DemonMetal => (0.45, 0.7),
        TexId::ArmorSecondary => (0.6, 0.35),
        _ => (0.85, 0.05),
    };
    let h = materials.add(StandardMaterial {
        base_color: tint,
        base_color_texture: tex_handle,
        emissive,
        perceptual_roughness: rough,
        metallic: metal,
        unlit,
        // Double-sided so faceted shells render regardless of winding, and thin
        // parts (wings, capes, blades) are visible from both faces.
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    (h, emissive)
}

/// Build the articulated model for `kind` as children of the already-spawned
/// `root` (the Enemy entity). Returns the per-instance materials for hit-flashing.
pub fn build_monster_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &MonsterTextures,
    kind: MonsterKind,
    root: Entity,
) -> (Vec<(Handle<StandardMaterial>, LinearRgba)>, Limbs) {
    let rig = rig_for(kind);
    let mut name_to_e: HashMap<&'static str, Entity> = HashMap::new();
    let mut name_to_group: HashMap<&'static str, LimbGroup> = HashMap::new();
    let mut seen_groups: HashSet<LimbGroup> = HashSet::new();
    let mut present: HashSet<LimbGroup> = HashSet::new();
    let mut mats = Vec::new();
    for spec in &rig {
        let parent_e = spec
            .parent
            .and_then(|n| name_to_e.get(n).copied())
            .unwrap_or(root);
        // Structural group: a bone's own role seeds its group; a Torso seed
        // inherits the parent's group (so a Static weapon-arm bone joins ArmR,
        // and an eye/fang Static bone joins Head). Parents precede children in
        // the rig list, so the parent's group is already resolved.
        let own = group_of(spec.anim);
        let group = if own != LimbGroup::Torso {
            own
        } else {
            spec.parent.and_then(|p| name_to_group.get(p).copied()).unwrap_or(LimbGroup::Torso)
        };
        name_to_group.insert(spec.name, group);
        present.insert(group);

        let base_rot = Quat::from_euler(
            EulerRot::XYZ,
            spec.rot_deg.x.to_radians(),
            spec.rot_deg.y.to_radians(),
            spec.rot_deg.z.to_radians(),
        );
        let mesh = make_mesh(meshes, &spec.shape);
        let bonehurt = BoneHurt { group, local_center: spec.mesh_off, local_half: shape_half(&spec.shape) };
        let joint = commands
            .spawn((
                ChildOf(parent_e),
                Transform::from_translation(spec.pos).with_rotation(base_rot),
                Visibility::default(),
                Bone { owner: root, role: spec.anim, base_rot, base_pos: spec.pos },
                bonehurt,
            ))
            .id();
        // The first bone of a severable group is its subtree root (parents first).
        if group.severable() && !seen_groups.contains(&group) {
            seen_groups.insert(group);
            commands.entity(joint).insert(LimbRoot { owner: root, group, mesh: Some(mesh.clone()) });
        }
        name_to_e.insert(spec.name, joint);

        let (mat, base_em) = make_material(materials, tex, kind, spec);
        mats.push((mat.clone(), base_em));
        commands.spawn((
            ChildOf(joint),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(spec.mesh_off),
        ));
    }
    let mut max = [0.0f32; LimbGroup::COUNT];
    for g in &present {
        if g.severable() {
            max[g.idx()] = limb_max(kind, *g);
        }
    }
    let limbs = Limbs { max, taken: [0.0; LimbGroup::COUNT], severed: [false; LimbGroup::COUNT] };
    (mats, limbs)
}

// ----------------------------------------------------------------------------
// Procedural animation.
// ----------------------------------------------------------------------------
fn animate_monsters(
    time: Res<Time>,
    q_owner: Query<(&Enemy, Option<&Dying>)>,
    mut q_bone: Query<(&Bone, &mut Transform), Without<Severed>>,
) {
    let t = time.elapsed_secs();
    for (bone, mut tf) in &mut q_bone {
        let Ok((en, dying)) = q_owner.get(bone.owner) else {
            continue;
        };
        // While dying, limbs relax to their base pose; the root topples (below).
        if dying.is_some() {
            tf.rotation = bone.base_rot;
            continue;
        }

        let speed = Vec3::new(en.vel.x, 0.0, en.vel.z).length();
        let mv = (speed / en.speed.max(0.1)).clamp(0.0, 1.0);
        let g = en.gait;
        let atk = en.atk_anim.clamp(0.0, 1.0);
        let pain = (en.pain / 0.22).clamp(0.0, 1.0);
        let breathe = (t * 1.6).sin();

        let mut q = bone.base_rot;
        let mut pos = bone.base_pos;
        use AnimRole::*;
        match bone.role {
            ThighL | ThighR => {
                let ph = if bone.role == ThighR { PI } else { 0.0 };
                let swing = (g + ph).sin() * (0.12 + 0.55 * mv);
                q *= Quat::from_rotation_x(swing);
            }
            ShinL | ShinR => {
                let ph = if bone.role == ShinR { PI } else { 0.0 };
                // Knee bends most as the leg swings back.
                let bend = (-(g + ph).sin()).max(0.0) * (0.15 + 0.9 * mv);
                q *= Quat::from_rotation_x(bend);
            }
            FootL | FootR => {
                let ph = if bone.role == FootR { PI } else { 0.0 };
                q *= Quat::from_rotation_x(-(g + ph).sin().max(0.0) * 0.4 * mv);
            }
            UpperArmL | UpperArmR => {
                // Arms counter-swing the legs.
                let ph = if bone.role == UpperArmL { PI } else { 0.0 };
                let swing = (g + ph).sin() * (0.1 + 0.4 * mv);
                q *= Quat::from_rotation_x(swing);
            }
            ForearmL | ForearmR => {
                let elbow = 0.25 + 0.12 * mv;
                q *= Quat::from_rotation_x(-elbow);
            }
            WeaponArm => {
                // Raise & swing forward on attack; a tiny idle sway otherwise.
                // Melee brutes take a big overhead chop; ranged shooters a small kick.
                use MonsterKind::*;
                let amp = match en.kind {
                    Knight | Ogre | DeathKnight => 1.6,
                    _ => 0.7,
                };
                q *= Quat::from_rotation_x(amp * atk + 0.05 * breathe);
            }
            Head => {
                q *= Quat::from_rotation_x(0.05 * breathe)
                    * Quat::from_rotation_y(0.06 * (t * 0.7).sin());
            }
            Jaw => {
                // Open wide when attacking, with a faint idle.
                let open = (atk * 0.9 + 0.06 * (breathe + 1.0)).clamp(0.0, 1.0);
                q *= Quat::from_rotation_x(open * 0.6);
            }
            Torso | Chest => {
                let lean = 0.05 * breathe - 0.28 * pain + 0.12 * atk;
                q *= Quat::from_rotation_x(lean);
            }
            Pelvis => {
                // Subtle vertical walk-bob.
                pos.y += (g * 1.0).sin().abs() * 0.05 * mv;
                tf.translation = pos;
            }
            Tail => {
                q *= Quat::from_rotation_y(0.45 * (t * 2.2).sin())
                    * Quat::from_rotation_x(0.18 * (t * 1.4).sin());
            }
            WingL | WingR => {
                let s = if bone.role == WingR { -1.0 } else { 1.0 };
                let flap = (t * 7.5).sin() * 0.7;
                q *= Quat::from_rotation_z(s * (0.25 + flap));
            }
            Cape => {
                q *= Quat::from_rotation_x(0.15 * (t * 1.5).sin() + 0.1);
            }
            Static => {}
        }
        tf.rotation = q;
    }
}

fn animate_death(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Dying)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut d) in &mut q {
        d.t += dt;
        let k = (d.t / 0.7).clamp(0.0, 1.0);
        let smooth = k * k * (3.0 - 2.0 * k); // smoothstep
        let topple = smooth * (PI * 0.5) * 0.92;
        tf.rotation = Quat::from_rotation_y(d.yaw) * Quat::from_rotation_x(topple);
        if d.t > 4.0 {
            tf.translation.y -= dt * 0.5; // sink into the floor before vanishing
        }
        if d.t > 6.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Rebuild the shared per-frame limb hitbox list from live bone transforms. Only
/// awake, alive, non-dying, in-range monsters contribute; severed bones are
/// excluded by the `Without<Severed>` filter.
fn rebuild_limb_boxes(
    mut out: ResMut<LimbBoxes>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_owner: Query<(Entity, &GlobalTransform, &Enemy), (With<Health>, Without<Dying>)>,
    q_bone: Query<(&Bone, &BoneHurt, &GlobalTransform), Without<Severed>>,
) {
    out.boxes.clear();
    let cam_pos = cam.iter().next().map(|g| g.translation());
    let mut live: HashSet<Entity> = HashSet::new();
    for (e, gt, en) in &q_owner {
        if !en.awake {
            continue;
        }
        if let Some(cp) = cam_pos {
            if gt.translation().distance(cp) > LIMB_BOX_MAX_DIST {
                continue;
            }
        }
        live.insert(e);
    }
    for (bone, bh, gt) in &q_bone {
        if !live.contains(&bone.owner) {
            continue;
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for c in CORNERS {
            let p = gt.transform_point(bh.local_center + c * bh.local_half);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        out.boxes.push((bone.owner, bh.group, Aabb { min: lo, max: hi }));
    }
}

/// Recursively tag a severed bone's descendants `Severed` so the animator and box
/// rebuild skip them (the subtree is also hidden via inherited Visibility).
fn mark_descendants_severed(commands: &mut Commands, children: &Query<&Children>, e: Entity) {
    if let Ok(kids) = children.get(e) {
        for &c in kids {
            commands.entity(c).insert(Severed);
            mark_descendants_severed(commands, children, c);
        }
    }
}

/// React to a `SeverEvent`: flag the AI cripple, hide the limb subtree, spawn a
/// tumbling gib at the bone, and play blood + a crunch. Runs in the combat chain
/// after `apply_damage` and before `check_deaths`.
pub fn do_sever(
    mut commands: Commands,
    mut reader: MessageReader<SeverEvent>,
    mut q_root: Query<(&mut Crippled, &Enemy)>,
    q_limbroot: Query<(Entity, &LimbRoot, &GlobalTransform)>,
    children: Query<&Children>,
    gfx: Res<GfxAssets>,
    mut sfx: MessageWriter<Sfx>,
) {
    for ev in reader.read() {
        let Ok((mut cr, en)) = q_root.get_mut(ev.root) else {
            continue;
        };
        match ev.limb {
            LimbGroup::ArmR => cr.disarmed = true,
            LimbGroup::LegL | LimbGroup::LegR => cr.legs_lost = (cr.legs_lost + 1).min(2),
            LimbGroup::Head => cr.head_gone = true,
            LimbGroup::WingL => cr.grounded = true,
            _ => {}
        }
        let kind = en.kind;
        for (be, lr, gt) in &q_limbroot {
            if lr.owner != ev.root || lr.group != ev.limb {
                continue;
            }
            commands.entity(be).insert((Visibility::Hidden, Severed));
            mark_descendants_severed(&mut commands, &children, be);
            let wt = gt.translation();
            let mesh = lr.mesh.clone().unwrap_or_else(|| gfx.small_sphere.clone());
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(gfx.gib.clone()),
                Transform::from_translation(wt),
                LimbGib { vel: ev.dir * 6.0 + Vec3::Y * 4.0, spin: Vec3::new(7.0, 5.0, 9.0), life: 5.0 },
                LevelEntity,
            ));
            break;
        }
        spawn_blood(&mut commands, &gfx, ev.at, ev.dir);
        sfx.write(Sfx::pitched(Sound::Sever, ev.at, kind_pitch(kind)));
    }
}

/// Tumble detached limb gibs under gravity; burst into small gibs when they expire.
fn update_limb_gibs(
    mut commands: Commands,
    time: Res<Time>,
    gfx: Res<GfxAssets>,
    mut q: Query<(Entity, &mut Transform, &mut LimbGib)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut g) in &mut q {
        g.vel.y -= GRAVITY * dt;
        tf.translation += g.vel * dt;
        tf.rotation *= Quat::from_scaled_axis(g.spin * dt);
        g.life -= dt;
        if g.life <= 0.0 {
            spawn_gibs(&mut commands, &gfx, tf.translation, 4);
            commands.entity(e).despawn();
        }
    }
}

// ----------------------------------------------------------------------------
// The rigs. `rig_for` dispatches per kind. These are placeholder bipeds/flyer
// until the designed rigs are plugged in.
// ----------------------------------------------------------------------------
fn rig_for(kind: MonsterKind) -> Vec<BoneSpec> {
    use MonsterKind::*;
    match kind {
        Grunt => rig_grunt(),
        Enforcer => rig_enforcer(),
        Knight => rig_knight(),
        Scrag => rig_scrag(),
        Ogre => rig_ogre(),
        DeathKnight => rig_deathknight(),
        Weaver => rig_weaver(),
    }
}

fn rig_grunt() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.27), pos: Vec3::new(0.0, -0.16, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.56, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.3, r1: 0.34, len: 0.42 }, pos: Vec3::new(0.0, 0.16, -0.03), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(16.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.74, 0.76, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Sphere(0.36), pos: Vec3::new(0.0, 0.34, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.76, 0.78, 0.64], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "flakPlate", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.3, 0.22, 0.14)), pos: Vec3::new(0.0, -0.02, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(18.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.46, 0.52, 0.32], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "neck", parent: Some("chest"), shape: Shape::Limb { r0: 0.18, r1: 0.16, len: 0.16 }, pos: Vec3::new(0.0, 0.22, -0.08), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(34.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.72, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "head", parent: Some("neck"), shape: Shape::Sphere(0.18), pos: Vec3::new(0.0, 0.16, -0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-18.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.72, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.13, 0.07, 0.14)), pos: Vec3::new(0.0, -0.09, -0.1), mesh_off: Vec3::new(0.0, -0.02, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.66, 0.64, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangL", parent: Some("jaw"), shape: Shape::Claw { r: 0.022, len: 0.08, bend: 0.0 }, pos: Vec3::new(0.05, 0.02, -0.09), mesh_off: Vec3::new(0.0, 0.04, 0.0), rot_deg: Vec3::new(180.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.92, 0.9, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangR", parent: Some("jaw"), shape: Shape::Claw { r: 0.022, len: 0.08, bend: 0.0 }, pos: Vec3::new(-0.05, 0.02, -0.09), mesh_off: Vec3::new(0.0, 0.04, 0.0), rot_deg: Vec3::new(180.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.92, 0.9, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.03), pos: Vec3::new(0.06, 0.01, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.5, 0.12], emissive: [7.0, 0.8, 0.18], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.03), pos: Vec3::new(-0.06, 0.01, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.5, 0.12], emissive: [7.0, 0.8, 0.18], anim: AnimRole::Static },
        BoneSpec { name: "shoulderL", parent: Some("chest"), shape: Shape::Sphere(0.2), pos: Vec3::new(0.31, 0.12, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.56, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("shoulderL"), shape: Shape::Limb { r0: 0.13, r1: 0.09, len: 0.36 }, pos: Vec3::new(0.06, -0.04, 0.0), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(12.0, 0.0, 16.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Limb { r0: 0.11, r1: 0.07, len: 0.34 }, pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(44.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "handL", parent: Some("farmL"), shape: Shape::Sphere(0.11), pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.68, 0.7, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawLb", parent: Some("handL"), shape: Shape::Claw { r: 0.034, len: 0.21, bend: 0.08 }, pos: Vec3::new(0.0, -0.05, -0.05), mesh_off: Vec3::new(0.0, -0.1, 0.0), rot_deg: Vec3::new(36.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.88, 0.84, 0.72], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawLc", parent: Some("handL"), shape: Shape::Claw { r: 0.032, len: 0.18, bend: 0.07 }, pos: Vec3::new(-0.06, -0.04, -0.04), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(40.0, 0.0, 12.0), tex: TexId::Bone, tint: [0.88, 0.84, 0.72], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "shoulderR", parent: Some("chest"), shape: Shape::Sphere(0.21), pos: Vec3::new(-0.31, 0.13, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.56, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("shoulderR"), shape: Shape::Limb { r0: 0.14, r1: 0.1, len: 0.34 }, pos: Vec3::new(0.04, -0.04, -0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(-52.0, 0.0, -12.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Limb { r0: 0.12, r1: 0.085, len: 0.32 }, pos: Vec3::new(0.0, -0.32, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-46.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "handR", parent: Some("farmR"), shape: Shape::Sphere(0.12), pos: Vec3::new(0.0, -0.32, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.68, 0.7, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "shotgun", parent: Some("handR"), shape: Shape::Box(Vec3::new(0.05, 0.05, 0.27)), pos: Vec3::new(0.0, -0.02, -0.18), mesh_off: Vec3::new(0.0, 0.0, -0.14), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.42, 0.42, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.16, r1: 0.11, len: 0.36 }, pos: Vec3::new(0.13, -0.1, 0.01), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 3.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Limb { r0: 0.12, r1: 0.08, len: 0.34 }, pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.56, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.16, r1: 0.11, len: 0.36 }, pos: Vec3::new(-0.13, -0.1, 0.01), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(-4.0, 0.0, -3.0), tex: TexId::SkinPrimary, tint: [0.72, 0.74, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Limb { r0: 0.12, r1: 0.08, len: 0.34 }, pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.56, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
    ]
}

fn rig_enforcer() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.28), pos: Vec3::new(0.0, -0.16, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.66, 0.84], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.3, r1: 0.34, len: 0.42 }, pos: Vec3::new(0.0, 0.2, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(13.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.66, 0.7, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Sphere(0.38), pos: Vec3::new(0.0, 0.34, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.58, 0.64, 0.86], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "chestcore", parent: Some("chest"), shape: Shape::Sphere(0.11), pos: Vec3::new(0.0, 0.0, -0.31), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.15, 0.95, 1.0], emissive: [0.3, 6.0, 7.5], anim: AnimRole::Static },
        BoneSpec { name: "neck", parent: Some("chest"), shape: Shape::Limb { r0: 0.18, r1: 0.15, len: 0.18 }, pos: Vec3::new(0.0, 0.26, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(26.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "head", parent: Some("neck"), shape: Shape::Box(Vec3::new(0.15, 0.15, 0.18)), pos: Vec3::new(0.0, 0.17, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-14.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.6, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.14, 0.05, 0.12)), pos: Vec3::new(0.0, -0.13, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(2.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.48, 0.52, 0.68], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "visor", parent: Some("head"), shape: Shape::Box(Vec3::new(0.14, 0.03, 0.025)), pos: Vec3::new(0.0, 0.0, -0.17), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.15, 0.95, 1.0], emissive: [0.3, 6.5, 8.0], anim: AnimRole::Static },
        BoneSpec { name: "pauldronL", parent: Some("chest"), shape: Shape::Sphere(0.22), pos: Vec3::new(0.33, 0.18, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.48, 0.54, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("pauldronL"), shape: Shape::Limb { r0: 0.14, r1: 0.1, len: 0.36 }, pos: Vec3::new(0.07, -0.08, 0.0), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(6.0, 0.0, 12.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "elbowL", parent: Some("uarmL"), shape: Shape::Sphere(0.11), pos: Vec3::new(0.0, -0.34, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "farmL", parent: Some("elbowL"), shape: Shape::Limb { r0: 0.12, r1: 0.09, len: 0.34 }, pos: Vec3::new(0.0, -0.04, -0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(34.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.6, 0.8], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "fistL", parent: Some("farmL"), shape: Shape::Sphere(0.12), pos: Vec3::new(0.0, -0.32, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.54, 0.72], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL", parent: Some("fistL"), shape: Shape::Claw { r: 0.04, len: 0.18, bend: 0.1 }, pos: Vec3::new(0.0, -0.07, -0.06), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(40.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.85, 0.86, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "pauldronR", parent: Some("chest"), shape: Shape::Sphere(0.24), pos: Vec3::new(-0.34, 0.18, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.48, 0.54, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("pauldronR"), shape: Shape::Limb { r0: 0.16, r1: 0.13, len: 0.34 }, pos: Vec3::new(-0.05, -0.09, 0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(30.0, 0.0, -8.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "gunbreech", parent: Some("uarmR"), shape: Shape::Sphere(0.16), pos: Vec3::new(0.0, -0.32, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.42, 0.46, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "gunbarrel", parent: Some("gunbreech"), shape: Shape::Limb { r0: 0.14, r1: 0.1, len: 0.4 }, pos: Vec3::new(0.0, -0.02, -0.04), mesh_off: Vec3::new(0.0, -0.04, -0.18), rot_deg: Vec3::new(82.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.46, 0.5, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "gunmuzzle", parent: Some("gunbarrel"), shape: Shape::Cone { r: 0.1, h: 0.16 }, pos: Vec3::new(0.0, -0.38, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(180.0, 0.0, 0.0), tex: TexId::None, tint: [0.15, 0.95, 1.0], emissive: [0.3, 6.5, 8.5], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.16, r1: 0.12, len: 0.4 }, pos: Vec3::new(0.15, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(-3.0, 0.0, 3.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "kneeL", parent: Some("thighL"), shape: Shape::Sphere(0.12), pos: Vec3::new(0.0, -0.4, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.6, 0.8], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "shinL", parent: Some("kneeL"), shape: Shape::Limb { r0: 0.13, r1: 0.09, len: 0.36 }, pos: Vec3::new(0.0, -0.04, 0.02), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(5.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.6, 0.8], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.12, 0.07, 0.19)), pos: Vec3::new(0.0, -0.36, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.4, 0.44, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.16, r1: 0.12, len: 0.4 }, pos: Vec3::new(-0.15, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(-3.0, 0.0, -3.0), tex: TexId::SkinPrimary, tint: [0.5, 0.54, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Limb { r0: 0.13, r1: 0.09, len: 0.36 }, pos: Vec3::new(0.0, -0.42, 0.02), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(5.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.6, 0.8], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.12, 0.07, 0.19)), pos: Vec3::new(0.0, -0.36, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.4, 0.44, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_knight() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.2), pos: Vec3::new(0.0, -0.12, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.46, 0.48, 0.52], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.24, r1: 0.27, len: 0.34 }, pos: Vec3::new(0.0, 0.18, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(24.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.52, 0.54, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Sphere(0.3), pos: Vec3::new(0.0, 0.3, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.58, 0.6, 0.64], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "cape", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.24, 0.42, 0.025)), pos: Vec3::new(0.0, -0.02, 0.2), mesh_off: Vec3::new(0.0, -0.42, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.18, 0.18, 0.21], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Cape },
        BoneSpec { name: "neck", parent: Some("chest"), shape: Shape::Limb { r0: 0.13, r1: 0.12, len: 0.13 }, pos: Vec3::new(0.0, 0.2, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(34.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.4, 0.41, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "head", parent: Some("neck"), shape: Shape::Box(Vec3::new(0.11, 0.13, 0.15)), pos: Vec3::new(0.0, 0.14, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-12.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.52, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "visor", parent: Some("head"), shape: Shape::Cone { r: 0.09, h: 0.18 }, pos: Vec3::new(0.0, -0.02, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-90.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.42, 0.43, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "hornL", parent: Some("head"), shape: Shape::Claw { r: 0.03, len: 0.18, bend: 0.18 }, pos: Vec3::new(0.07, 0.07, 0.02), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(-30.0, 0.0, -12.0), tex: TexId::DemonMetal, tint: [0.34, 0.35, 0.38], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "hornR", parent: Some("head"), shape: Shape::Claw { r: 0.03, len: 0.18, bend: 0.18 }, pos: Vec3::new(-0.07, 0.07, 0.02), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(-30.0, 0.0, 12.0), tex: TexId::DemonMetal, tint: [0.34, 0.35, 0.38], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "eyeSlit", parent: Some("head"), shape: Shape::Box(Vec3::new(0.075, 0.014, 0.02)), pos: Vec3::new(0.0, 0.0, -0.13), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.15, 0.04], emissive: [9.0, 0.5, 0.08], anim: AnimRole::Head },
        BoneSpec { name: "shoulderL", parent: Some("chest"), shape: Shape::Sphere(0.14), pos: Vec3::new(0.28, 0.12, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.52, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("shoulderL"), shape: Shape::Limb { r0: 0.1, r1: 0.07, len: 0.34 }, pos: Vec3::new(0.04, -0.04, 0.0), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(18.0, 0.0, 12.0), tex: TexId::SkinPrimary, tint: [0.42, 0.43, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Limb { r0: 0.085, r1: 0.06, len: 0.32 }, pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(34.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.48, 0.5, 0.54], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL1", parent: Some("farmL"), shape: Shape::Claw { r: 0.028, len: 0.17, bend: 0.11 }, pos: Vec3::new(0.05, -0.32, -0.04), mesh_off: Vec3::new(0.0, -0.085, 0.0), rot_deg: Vec3::new(60.0, 0.0, -8.0), tex: TexId::Bone, tint: [0.82, 0.8, 0.74], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL2", parent: Some("farmL"), shape: Shape::Claw { r: 0.028, len: 0.18, bend: 0.12 }, pos: Vec3::new(-0.03, -0.33, -0.05), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(60.0, 0.0, 8.0), tex: TexId::Bone, tint: [0.82, 0.8, 0.74], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "shoulderR", parent: Some("chest"), shape: Shape::Sphere(0.15), pos: Vec3::new(-0.29, 0.13, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.52, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("shoulderR"), shape: Shape::Limb { r0: 0.11, r1: 0.075, len: 0.34 }, pos: Vec3::new(-0.04, -0.04, 0.04), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(40.0, 0.0, -10.0), tex: TexId::SkinPrimary, tint: [0.42, 0.43, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Limb { r0: 0.09, r1: 0.07, len: 0.32 }, pos: Vec3::new(0.0, -0.34, 0.02), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(40.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.48, 0.5, 0.54], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "handR", parent: Some("farmR"), shape: Shape::Sphere(0.085), pos: Vec3::new(0.0, -0.32, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.46, 0.48, 0.52], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sword", parent: Some("handR"), shape: Shape::Box(Vec3::new(0.022, 0.48, 0.05)), pos: Vec3::new(0.0, -0.04, -0.02), mesh_off: Vec3::new(0.0, -0.48, 0.0), rot_deg: Vec3::new(-26.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.66, 0.68, 0.74], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.13, r1: 0.09, len: 0.4 }, pos: Vec3::new(0.11, -0.12, 0.01), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(-8.0, 0.0, 2.0), tex: TexId::SkinPrimary, tint: [0.42, 0.43, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Limb { r0: 0.09, r1: 0.055, len: 0.38 }, pos: Vec3::new(0.0, -0.4, 0.04), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(14.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.52, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.07, 0.045, 0.15)), pos: Vec3::new(0.0, -0.38, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.4, 0.41, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.13, r1: 0.09, len: 0.4 }, pos: Vec3::new(-0.11, -0.12, 0.01), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(-8.0, 0.0, -2.0), tex: TexId::SkinPrimary, tint: [0.42, 0.43, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Limb { r0: 0.09, r1: 0.055, len: 0.38 }, pos: Vec3::new(0.0, -0.4, 0.04), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(14.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.5, 0.52, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.07, 0.045, 0.15)), pos: Vec3::new(0.0, -0.38, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.4, 0.41, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_scrag() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "chest", parent: None, shape: Shape::Sphere(0.26), pos: Vec3::new(0.0, 0.18, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.62, 0.85, 0.45], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "torso", parent: Some("chest"), shape: Shape::Limb { r0: 0.24, r1: 0.16, len: 0.32 }, pos: Vec3::new(0.0, -0.16, 0.03), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.58, 0.82, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "tail1", parent: Some("torso"), shape: Shape::Limb { r0: 0.16, r1: 0.11, len: 0.34 }, pos: Vec3::new(0.0, -0.3, 0.02), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(-14.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.54, 0.78, 0.4], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
        BoneSpec { name: "tail2", parent: Some("tail1"), shape: Shape::Limb { r0: 0.11, r1: 0.07, len: 0.32 }, pos: Vec3::new(0.0, -0.33, 0.01), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-16.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.5, 0.74, 0.37], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
        BoneSpec { name: "tail3", parent: Some("tail2"), shape: Shape::Limb { r0: 0.07, r1: 0.015, len: 0.34 }, pos: Vec3::new(0.0, -0.31, 0.0), mesh_off: Vec3::new(0.0, -0.17, 0.0), rot_deg: Vec3::new(-20.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.46, 0.7, 0.34], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
        BoneSpec { name: "neck", parent: Some("chest"), shape: Shape::Limb { r0: 0.17, r1: 0.16, len: 0.14 }, pos: Vec3::new(0.0, 0.2, -0.07), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(26.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.83, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "head", parent: Some("neck"), shape: Shape::Sphere(0.19), pos: Vec3::new(0.0, 0.13, -0.07), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-14.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.64, 0.86, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "snout", parent: Some("head"), shape: Shape::Limb { r0: 0.13, r1: 0.07, len: 0.22 }, pos: Vec3::new(0.0, 0.0, -0.14), mesh_off: Vec3::new(0.0, 0.0, -0.11), rot_deg: Vec3::new(90.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.58, 0.8, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.13, 0.05, 0.19)), pos: Vec3::new(0.0, -0.1, -0.11), mesh_off: Vec3::new(0.0, -0.02, -0.07), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.5, 0.72, 0.36], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangUL", parent: Some("head"), shape: Shape::Claw { r: 0.028, len: 0.12, bend: 0.04 }, pos: Vec3::new(0.07, -0.08, -0.26), mesh_off: Vec3::new(0.0, -0.06, 0.0), rot_deg: Vec3::new(182.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.93, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "fangUR", parent: Some("head"), shape: Shape::Claw { r: 0.028, len: 0.12, bend: 0.04 }, pos: Vec3::new(-0.07, -0.08, -0.26), mesh_off: Vec3::new(0.0, -0.06, 0.0), rot_deg: Vec3::new(182.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.93, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "fangLL", parent: Some("jaw"), shape: Shape::Claw { r: 0.024, len: 0.09, bend: 0.04 }, pos: Vec3::new(0.06, 0.03, -0.17), mesh_off: Vec3::new(0.0, -0.045, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.93, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangLR", parent: Some("jaw"), shape: Shape::Claw { r: 0.024, len: 0.09, bend: 0.04 }, pos: Vec3::new(-0.06, 0.03, -0.17), mesh_off: Vec3::new(0.0, -0.045, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.93, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.05), pos: Vec3::new(0.1, 0.06, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.9, 0.2], emissive: [7.0, 4.5, 0.3], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.05), pos: Vec3::new(-0.1, 0.06, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.9, 0.2], emissive: [7.0, 4.5, 0.3], anim: AnimRole::Static },
        BoneSpec { name: "shoulderL", parent: Some("chest"), shape: Shape::Sphere(0.12), pos: Vec3::new(0.22, 0.02, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.83, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("shoulderL"), shape: Shape::Limb { r0: 0.08, r1: 0.055, len: 0.18 }, pos: Vec3::new(0.04, -0.04, -0.02), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(40.0, 0.0, -34.0), tex: TexId::SkinPrimary, tint: [0.58, 0.82, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Limb { r0: 0.055, r1: 0.04, len: 0.18 }, pos: Vec3::new(0.0, -0.18, -0.01), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(-60.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.83, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "handL", parent: Some("farmL"), shape: Shape::Sphere(0.055), pos: Vec3::new(0.0, -0.18, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.56, 0.8, 0.4], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL1", parent: Some("handL"), shape: Shape::Claw { r: 0.022, len: 0.15, bend: 0.06 }, pos: Vec3::new(0.04, -0.02, -0.04), mesh_off: Vec3::new(0.0, -0.075, 0.0), rot_deg: Vec3::new(200.0, 0.0, 10.0), tex: TexId::Bone, tint: [0.9, 0.88, 0.76], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL2", parent: Some("handL"), shape: Shape::Claw { r: 0.022, len: 0.16, bend: 0.07 }, pos: Vec3::new(0.0, -0.03, -0.05), mesh_off: Vec3::new(0.0, -0.08, 0.0), rot_deg: Vec3::new(205.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.9, 0.88, 0.76], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL3", parent: Some("handL"), shape: Shape::Claw { r: 0.022, len: 0.15, bend: 0.06 }, pos: Vec3::new(-0.04, -0.02, -0.04), mesh_off: Vec3::new(0.0, -0.075, 0.0), rot_deg: Vec3::new(200.0, 0.0, -10.0), tex: TexId::Bone, tint: [0.9, 0.88, 0.76], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "shoulderR", parent: Some("chest"), shape: Shape::Sphere(0.12), pos: Vec3::new(-0.22, 0.02, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.83, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmR },
        BoneSpec { name: "uarmR", parent: Some("shoulderR"), shape: Shape::Limb { r0: 0.08, r1: 0.055, len: 0.18 }, pos: Vec3::new(-0.04, -0.04, -0.02), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(40.0, 0.0, 34.0), tex: TexId::SkinPrimary, tint: [0.58, 0.82, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmR },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Limb { r0: 0.055, r1: 0.04, len: 0.18 }, pos: Vec3::new(0.0, -0.18, -0.01), mesh_off: Vec3::new(0.0, -0.09, 0.0), rot_deg: Vec3::new(-60.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.83, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmR },
        BoneSpec { name: "wingL", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.34, 0.26, 0.012)), pos: Vec3::new(0.16, 0.12, 0.07), mesh_off: Vec3::new(0.3, -0.04, 0.0), rot_deg: Vec3::new(10.0, -28.0, 30.0), tex: TexId::ArmorSecondary, tint: [0.4, 0.62, 0.32], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WingL },
    ]
}

fn rig_ogre() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.36), pos: Vec3::new(0.0, -0.36, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.62, 0.54], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "gut", parent: Some("pelvis"), shape: Shape::Sphere(0.47), pos: Vec3::new(0.0, 0.14, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.76, 0.68, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "apron", parent: Some("gut"), shape: Shape::Box(Vec3::new(0.4, 0.5, 0.07)), pos: Vec3::new(0.0, -0.28, -0.42), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.56, 0.32, 0.28], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("gut"), shape: Shape::Limb { r0: 0.42, r1: 0.46, len: 0.36 }, pos: Vec3::new(0.0, 0.34, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(20.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "neck", parent: Some("chest"), shape: Shape::Limb { r0: 0.2, r1: 0.18, len: 0.1 }, pos: Vec3::new(0.0, 0.34, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(28.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.76, 0.68, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "head", parent: Some("neck"), shape: Shape::Sphere(0.16), pos: Vec3::new(0.0, 0.12, -0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-14.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.72, 0.64], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.14, 0.06, 0.14)), pos: Vec3::new(0.0, -0.09, -0.1), mesh_off: Vec3::new(0.0, -0.02, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.62, 0.54], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fang", parent: Some("head"), shape: Shape::Claw { r: 0.022, len: 0.09, bend: 0.0 }, pos: Vec3::new(0.06, -0.11, -0.14), mesh_off: Vec3::new(0.0, -0.045, 0.0), rot_deg: Vec3::new(178.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.92, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.026), pos: Vec3::new(0.06, 0.03, -0.14), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.45, 0.08], emissive: [8.0, 0.5, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.026), pos: Vec3::new(-0.06, 0.03, -0.14), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.45, 0.08], emissive: [8.0, 0.5, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "shoulderL", parent: Some("chest"), shape: Shape::Sphere(0.24), pos: Vec3::new(0.42, 0.18, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("shoulderL"), shape: Shape::Limb { r0: 0.18, r1: 0.14, len: 0.38 }, pos: Vec3::new(0.06, -0.06, 0.0), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(10.0, 0.0, 16.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Limb { r0: 0.15, r1: 0.11, len: 0.38 }, pos: Vec3::new(0.0, -0.38, 0.02), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(44.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.71, 0.63], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "fistL", parent: Some("farmL"), shape: Shape::Sphere(0.16), pos: Vec3::new(0.0, -0.38, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL1", parent: Some("fistL"), shape: Shape::Claw { r: 0.045, len: 0.22, bend: 0.11 }, pos: Vec3::new(0.05, -0.07, -0.07), mesh_off: Vec3::new(0.0, -0.11, 0.0), rot_deg: Vec3::new(44.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.9, 0.86, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL2", parent: Some("fistL"), shape: Shape::Claw { r: 0.045, len: 0.22, bend: 0.11 }, pos: Vec3::new(-0.05, -0.07, -0.07), mesh_off: Vec3::new(0.0, -0.11, 0.0), rot_deg: Vec3::new(44.0, 0.0, -6.0), tex: TexId::Bone, tint: [0.9, 0.86, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "shoulderR", parent: Some("chest"), shape: Shape::Sphere(0.25), pos: Vec3::new(-0.42, 0.2, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("shoulderR"), shape: Shape::Limb { r0: 0.19, r1: 0.15, len: 0.38 }, pos: Vec3::new(-0.04, -0.04, -0.04), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(-50.0, 0.0, -14.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Limb { r0: 0.16, r1: 0.12, len: 0.38 }, pos: Vec3::new(0.0, -0.38, 0.02), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(74.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.71, 0.63], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sawBody", parent: Some("farmR"), shape: Shape::Box(Vec3::new(0.1, 0.14, 0.24)), pos: Vec3::new(0.0, -0.4, -0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.42, 0.4, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sawBar", parent: Some("sawBody"), shape: Shape::Box(Vec3::new(0.04, 0.07, 0.5)), pos: Vec3::new(0.0, 0.0, -0.66), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.78, 0.76, 0.72], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.22, r1: 0.16, len: 0.42 }, pos: Vec3::new(0.2, -0.1, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(-4.0, 0.0, 4.0), tex: TexId::SkinPrimary, tint: [0.74, 0.66, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Limb { r0: 0.17, r1: 0.11, len: 0.38 }, pos: Vec3::new(0.0, -0.42, 0.03), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.64, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.15, 0.07, 0.27)), pos: Vec3::new(0.0, -0.38, -0.08), mesh_off: Vec3::new(0.0, 0.0, -0.06), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.66, 0.58, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.22, r1: 0.16, len: 0.42 }, pos: Vec3::new(-0.2, -0.1, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(-4.0, 0.0, -4.0), tex: TexId::SkinPrimary, tint: [0.74, 0.66, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Limb { r0: 0.17, r1: 0.11, len: 0.38 }, pos: Vec3::new(0.0, -0.42, 0.03), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.64, 0.56], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
    ]
}

fn rig_deathknight() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.33), pos: Vec3::new(0.0, -0.16, 0.03), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.18, 0.16, 0.18], emissive: [0.6, 0.05, 0.02], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.4, r1: 0.46, len: 0.52 }, pos: Vec3::new(0.0, 0.36, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(16.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.16, 0.14, 0.16], emissive: [0.5, 0.04, 0.02], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Sphere(0.48), pos: Vec3::new(0.0, 0.44, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.17, 0.15, 0.17], emissive: [1.8, 0.16, 0.04], anim: AnimRole::Chest },
        BoneSpec { name: "pauldronL", parent: Some("chest"), shape: Shape::Sphere(0.3), pos: Vec3::new(0.44, 0.18, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.7, 0.05, 0.02], anim: AnimRole::UpperArmL },
        BoneSpec { name: "uarmL", parent: Some("pauldronL"), shape: Shape::Limb { r0: 0.17, r1: 0.12, len: 0.46 }, pos: Vec3::new(0.1, -0.07, 0.0), mesh_off: Vec3::new(0.0, -0.23, 0.0), rot_deg: Vec3::new(12.0, 0.0, 14.0), tex: TexId::ArmorSecondary, tint: [0.16, 0.14, 0.16], emissive: [0.6, 0.05, 0.02], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Limb { r0: 0.15, r1: 0.12, len: 0.48 }, pos: Vec3::new(0.0, -0.44, 0.02), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(42.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.17, 0.15, 0.17], emissive: [0.8, 0.06, 0.02], anim: AnimRole::ForearmL },
        BoneSpec { name: "handL", parent: Some("farmL"), shape: Shape::Sphere(0.16), pos: Vec3::new(0.0, -0.48, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.5, 0.04, 0.02], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL1", parent: Some("handL"), shape: Shape::Claw { r: 0.06, len: 0.4, bend: 0.18 }, pos: Vec3::new(0.0, -0.08, -0.1), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(38.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.42, 0.36, 0.34], emissive: [0.3, 0.02, 0.01], anim: AnimRole::ForearmL },
        BoneSpec { name: "pauldronR", parent: Some("chest"), shape: Shape::Sphere(0.32), pos: Vec3::new(-0.44, 0.18, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.7, 0.05, 0.02], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("pauldronR"), shape: Shape::Limb { r0: 0.18, r1: 0.13, len: 0.48 }, pos: Vec3::new(-0.1, -0.07, 0.0), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(-34.0, 0.0, -14.0), tex: TexId::ArmorSecondary, tint: [0.16, 0.14, 0.16], emissive: [0.6, 0.05, 0.02], anim: AnimRole::Static },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Limb { r0: 0.16, r1: 0.13, len: 0.48 }, pos: Vec3::new(0.0, -0.48, 0.0), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(-74.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.17, 0.15, 0.17], emissive: [0.8, 0.06, 0.02], anim: AnimRole::Static },
        BoneSpec { name: "handR", parent: Some("farmR"), shape: Shape::Sphere(0.18), pos: Vec3::new(0.0, -0.48, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.5, 0.04, 0.02], anim: AnimRole::Static },
        BoneSpec { name: "sword", parent: Some("handR"), shape: Shape::Box(Vec3::new(0.08, 1.05, 0.03)), pos: Vec3::new(0.0, 0.08, 0.0), mesh_off: Vec3::new(0.0, 0.95, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [1.0, 0.45, 0.25], emissive: [6.0, 0.9, 0.18], anim: AnimRole::Static },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Sphere(0.25), pos: Vec3::new(0.0, 0.4, -0.13), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(12.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.14, 0.12, 0.14], emissive: [0.9, 0.07, 0.02], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.16, 0.07, 0.18)), pos: Vec3::new(0.0, -0.11, -0.12), mesh_off: Vec3::new(0.0, -0.03, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.45, 0.4, 0.38], emissive: [0.4, 0.03, 0.01], anim: AnimRole::Jaw },
        BoneSpec { name: "hornL", parent: Some("head"), shape: Shape::Claw { r: 0.085, len: 0.64, bend: 0.46 }, pos: Vec3::new(0.14, 0.14, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-22.0, 0.0, 34.0), tex: TexId::Bone, tint: [0.42, 0.36, 0.34], emissive: [0.3, 0.02, 0.01], anim: AnimRole::Head },
        BoneSpec { name: "hornR", parent: Some("head"), shape: Shape::Claw { r: 0.085, len: 0.64, bend: 0.46 }, pos: Vec3::new(-0.14, 0.14, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-22.0, 0.0, -34.0), tex: TexId::Bone, tint: [0.42, 0.36, 0.34], emissive: [0.3, 0.02, 0.01], anim: AnimRole::Head },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.05), pos: Vec3::new(0.09, 0.03, -0.21), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.3, 0.06], emissive: [9.0, 0.7, 0.12], anim: AnimRole::Head },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.05), pos: Vec3::new(-0.09, 0.03, -0.21), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.3, 0.06], emissive: [9.0, 0.7, 0.12], anim: AnimRole::Head },
        BoneSpec { name: "cape", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.44, 0.8, 0.04)), pos: Vec3::new(0.0, 0.0, 0.28), mesh_off: Vec3::new(0.0, -0.8, 0.04), rot_deg: Vec3::new(12.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.55, 0.08, 0.08], emissive: [1.0, 0.05, 0.02], anim: AnimRole::Cape },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.21, r1: 0.15, len: 0.52 }, pos: Vec3::new(0.18, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.26, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 3.0), tex: TexId::ArmorSecondary, tint: [0.16, 0.14, 0.16], emissive: [0.6, 0.05, 0.02], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Limb { r0: 0.16, r1: 0.1, len: 0.5 }, pos: Vec3::new(0.0, -0.52, 0.02), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.5, 0.04, 0.02], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.13, 0.07, 0.25)), pos: Vec3::new(0.0, -0.52, -0.06), mesh_off: Vec3::new(0.0, 0.0, -0.08), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.14, 0.12, 0.14], emissive: [0.4, 0.03, 0.01], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Limb { r0: 0.21, r1: 0.15, len: 0.52 }, pos: Vec3::new(-0.18, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.26, 0.0), rot_deg: Vec3::new(-6.0, 0.0, -3.0), tex: TexId::ArmorSecondary, tint: [0.16, 0.14, 0.16], emissive: [0.6, 0.05, 0.02], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Limb { r0: 0.16, r1: 0.1, len: 0.5 }, pos: Vec3::new(0.0, -0.52, 0.02), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.15, 0.13, 0.15], emissive: [0.5, 0.04, 0.02], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.13, 0.07, 0.25)), pos: Vec3::new(0.0, -0.52, -0.06), mesh_off: Vec3::new(0.0, 0.0, -0.08), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.14, 0.12, 0.14], emissive: [0.4, 0.03, 0.01], anim: AnimRole::FootR },
    ]
}

/// The Weaver: a low, wide spider. A bulbous abdomen (Pelvis) trailing behind a
/// cephalothorax (Torso/Chest) that carries the head, glowing eye cluster and
/// chelicerae fangs. Eight two-segment legs splay out — four per side — animated
/// by reusing the biped leg/arm roles so the walk-cycle animator wiggles them.
/// Sits low (`half.y ≈ 0.5`) and wide. Skinned with the reused dark-chitin hide.
fn rig_weaver() -> Vec<BoneSpec> {
    // Tints: oily black chitin body, paler joint highlights, sickly green accents.
    let chitin = [0.16, 0.15, 0.18];
    let chitin_lo = [0.12, 0.11, 0.14];
    let limb = [0.14, 0.13, 0.16];
    // A splayed leg = thigh (out & down) + shin (down to a foot tip on the floor).
    // `side`: +1 = left (+X), -1 = right (-X). `zoff`: front..back placement.
    // `out`/`fwd` aim the thigh; the shin drops near-vertical to plant the foot.
    // Names are passed as static literals (thigh/shin/foot) so no allocation/leak.
    let leg = |thigh: &'static str, shin: &'static str, foot: &'static str,
               side: f32, zoff: f32, out: f32, fwd: f32,
               thigh_role: AnimRole, shin_role: AnimRole| {
        vec![
            BoneSpec { name: thigh, parent: Some("torso"), shape: Shape::Limb { r0: 0.075, r1: 0.05, len: 0.42 }, pos: Vec3::new(side * 0.22, 0.04, zoff), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(fwd, 0.0, side * out), tex: TexId::SkinPrimary, tint: limb, emissive: [0.0, 0.0, 0.0], anim: thigh_role },
            BoneSpec { name: shin, parent: Some(thigh), shape: Shape::Limb { r0: 0.05, r1: 0.022, len: 0.44 }, pos: Vec3::new(0.0, -0.42, 0.0), mesh_off: Vec3::new(0.0, -0.22, 0.0), rot_deg: Vec3::new(0.0, 0.0, side * -(out + 36.0)), tex: TexId::SkinPrimary, tint: limb, emissive: [0.0, 0.0, 0.0], anim: shin_role },
            BoneSpec { name: foot, parent: Some(shin), shape: Shape::Claw { r: 0.022, len: 0.12, bend: 0.04 }, pos: Vec3::new(0.0, -0.42, 0.0), mesh_off: Vec3::new(0.0, -0.06, 0.0), rot_deg: Vec3::new(170.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.5, 0.48, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        ]
    };
    let mut v = vec![
        // Abdomen: a fat low sphere trailing behind, the visual hub of the spider.
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Sphere(0.42), pos: Vec3::new(0.0, 0.0, 0.34), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: chitin, emissive: [0.02, 0.06, 0.02], anim: AnimRole::Pelvis },
        // A web-membrane saddle over the abdomen (reused membrane texture).
        BoneSpec { name: "abdomenPlate", parent: Some("pelvis"), shape: Shape::Box(Vec3::new(0.3, 0.16, 0.32)), pos: Vec3::new(0.0, 0.22, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-10.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: chitin_lo, emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        // Cephalothorax: the front body block the legs and head mount on.
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Sphere(0.3), pos: Vec3::new(0.0, -0.02, -0.36), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: chitin, emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "head", parent: Some("torso"), shape: Shape::Sphere(0.18), pos: Vec3::new(0.0, -0.02, -0.26), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: chitin_lo, emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        // Chelicerae fangs jutting down/forward from the head.
        BoneSpec { name: "fangL", parent: Some("head"), shape: Shape::Claw { r: 0.03, len: 0.16, bend: 0.05 }, pos: Vec3::new(0.06, -0.08, -0.12), mesh_off: Vec3::new(0.0, -0.08, 0.0), rot_deg: Vec3::new(150.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.55, 0.52, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangR", parent: Some("head"), shape: Shape::Claw { r: 0.03, len: 0.16, bend: 0.05 }, pos: Vec3::new(-0.06, -0.08, -0.12), mesh_off: Vec3::new(0.0, -0.08, 0.0), rot_deg: Vec3::new(150.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.55, 0.52, 0.44], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        // A cluster of glowing eyes — the unmistakable spider read.
        BoneSpec { name: "eyeL1", parent: Some("head"), shape: Shape::Sphere(0.035), pos: Vec3::new(0.07, 0.05, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.4, 1.0, 0.4], emissive: [0.6, 7.0, 0.6], anim: AnimRole::Static },
        BoneSpec { name: "eyeR1", parent: Some("head"), shape: Shape::Sphere(0.035), pos: Vec3::new(-0.07, 0.05, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.4, 1.0, 0.4], emissive: [0.6, 7.0, 0.6], anim: AnimRole::Static },
        BoneSpec { name: "eyeL2", parent: Some("head"), shape: Shape::Sphere(0.022), pos: Vec3::new(0.12, 0.02, -0.14), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.4, 1.0, 0.4], emissive: [0.4, 5.0, 0.4], anim: AnimRole::Static },
        BoneSpec { name: "eyeR2", parent: Some("head"), shape: Shape::Sphere(0.022), pos: Vec3::new(-0.12, 0.02, -0.14), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.4, 1.0, 0.4], emissive: [0.4, 5.0, 0.4], anim: AnimRole::Static },
    ];
    // Eight legs: four per side, front pair on arm roles (counter-swing), the rest
    // on leg roles (walk cycle). Front legs reach forward, rear legs trail back.
    use AnimRole::*;
    v.extend(leg("thL1", "shL1", "ftL1", 1.0, -0.16, 58.0, -34.0, UpperArmL, ForearmL));
    v.extend(leg("thL2", "shL2", "ftL2", 1.0, -0.02, 70.0, -10.0, ThighL, ShinL));
    v.extend(leg("thL3", "shL3", "ftL3", 1.0, 0.12, 70.0, 14.0, ThighL, ShinL));
    v.extend(leg("thL4", "shL4", "ftL4", 1.0, 0.26, 58.0, 36.0, ThighL, ShinL));
    v.extend(leg("thR1", "shR1", "ftR1", -1.0, -0.16, 58.0, -34.0, UpperArmR, ForearmR));
    v.extend(leg("thR2", "shR2", "ftR2", -1.0, -0.02, 70.0, -10.0, ThighR, ShinR));
    v.extend(leg("thR3", "shR3", "ftR3", -1.0, 0.12, 70.0, 14.0, ThighR, ShinR));
    v.extend(leg("thR4", "shR4", "ftR4", -1.0, 0.26, 58.0, 36.0, ThighR, ShinR));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The weapon arm (and its Static sub-bones) must map to ArmR, so severing it
    /// disarms — this is the structural-walk's whole reason to exist.
    #[test]
    fn weapon_arm_maps_to_armr() {
        assert_eq!(group_of(AnimRole::WeaponArm), LimbGroup::ArmR);
        assert_eq!(group_of(AnimRole::UpperArmR), LimbGroup::ArmR);
        assert_eq!(group_of(AnimRole::ForearmR), LimbGroup::ArmR);
    }

    /// The torso is the only never-severing group.
    #[test]
    fn torso_is_never_severable() {
        assert!(!LimbGroup::Torso.severable());
        assert!(LimbGroup::ArmR.severable() && LimbGroup::Head.severable());
        assert_eq!(group_of(AnimRole::Pelvis), LimbGroup::Torso);
        assert_eq!(group_of(AnimRole::Static), LimbGroup::Torso); // seed; inherits via the walk
    }

    /// A pool severs on exactly the hit that empties it — never twice.
    #[test]
    fn accrue_severs_exactly_once() {
        let (mut taken, mut sev) = (0.0f32, false);
        assert!(!accrue(&mut taken, 20.0, &mut sev, 12.0)); // 12/20
        assert!(accrue(&mut taken, 20.0, &mut sev, 12.0)); // 24/20 -> sever
        assert!(!accrue(&mut taken, 20.0, &mut sev, 99.0)); // already severed
        // A group the kind doesn't have (max 0) never severs.
        let (mut t2, mut s2) = (0.0f32, false);
        assert!(!accrue(&mut t2, 0.0, &mut s2, 100.0));
    }

    /// Nearest-hit picks the closer of two stacked boxes and the pad rescues a near miss.
    #[test]
    fn nearest_limb_hit_picks_closest_and_respects_pad() {
        let near = Aabb::from_center_half(Vec3::new(0.0, 0.0, 2.0), Vec3::splat(0.5));
        let far = Aabb::from_center_half(Vec3::new(0.0, 0.0, 6.0), Vec3::splat(0.5));
        let e = Entity::PLACEHOLDER;
        let boxes = [(e, LimbGroup::Head, near), (e, LimbGroup::Torso, far)];
        let hit = nearest_limb_hit(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), 20.0, &boxes, 0.0);
        assert_eq!(hit.map(|h| h.1), Some(LimbGroup::Head));
        // A ray that just misses (0.6 off-axis, box half 0.5) connects only with pad.
        let off = [(e, LimbGroup::Head, Aabb::from_center_half(Vec3::new(0.6, 0.0, 3.0), Vec3::splat(0.5)))];
        assert!(nearest_limb_hit(Vec3::ZERO, Vec3::Z, 20.0, &off, 0.0).is_none());
        assert!(nearest_limb_hit(Vec3::ZERO, Vec3::Z, 20.0, &off, 0.2).is_some());
    }

    /// shape_half encloses the extreme local vertices of the awkward shapes.
    #[test]
    fn shape_half_encloses_limb_and_claw() {
        let limb = shape_half(&Shape::Limb { r0: 0.2, r1: 0.1, len: 0.5 });
        assert!(limb.x >= 0.2 && limb.y >= 0.25); // widest radius + half length
        let claw = shape_half(&Shape::Claw { r: 0.08, len: 0.6, bend: 0.4 });
        assert!(claw.z >= 0.08 + 0.4); // base radius + full bend reach
    }

    /// Leg loss halves then crawls.
    #[test]
    fn cripple_speed_halves_then_crawls() {
        assert_eq!(cripple_speed(0), 1.0);
        assert_eq!(cripple_speed(1), 0.5);
        assert!(cripple_speed(2) < 0.2);
    }
}
