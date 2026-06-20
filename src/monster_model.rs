//! Procedural skeletal monster models.
//!
//! Each monster is no longer a single capsule: it's a small hierarchy of parented
//! "bone" entities (pelvis -> torso -> head/jaw, articulated arms & legs, plus
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
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use crate::enemies::Enemy;
use crate::level::MonsterKind;

pub struct MonsterModelPlugin;
impl Plugin for MonsterModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MonsterTextures>()
            .add_systems(Startup, load_monster_textures)
            .add_systems(Update, (animate_monsters, animate_death));
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
) -> Vec<(Handle<StandardMaterial>, LinearRgba)> {
    let rig = rig_for(kind);
    let mut name_to_e: HashMap<&'static str, Entity> = HashMap::new();
    let mut mats = Vec::new();
    for spec in &rig {
        let parent_e = spec
            .parent
            .and_then(|n| name_to_e.get(n).copied())
            .unwrap_or(root);
        let base_rot = Quat::from_euler(
            EulerRot::XYZ,
            spec.rot_deg.x.to_radians(),
            spec.rot_deg.y.to_radians(),
            spec.rot_deg.z.to_radians(),
        );
        let joint = commands
            .spawn((
                ChildOf(parent_e),
                Transform::from_translation(spec.pos).with_rotation(base_rot),
                Visibility::default(),
                Bone { owner: root, role: spec.anim, base_rot, base_pos: spec.pos },
            ))
            .id();
        name_to_e.insert(spec.name, joint);

        let mesh = make_mesh(meshes, &spec.shape);
        let (mat, base_em) = make_material(materials, tex, kind, spec);
        mats.push((mat.clone(), base_em));
        commands.spawn((
            ChildOf(joint),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(spec.mesh_off),
        ));
    }
    mats
}

// ----------------------------------------------------------------------------
// Procedural animation.
// ----------------------------------------------------------------------------
fn animate_monsters(
    time: Res<Time>,
    q_owner: Query<(&Enemy, Option<&Dying>)>,
    mut q_bone: Query<(&Bone, &mut Transform)>,
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
