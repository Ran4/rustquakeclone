//! Procedural skeletal monster models.
//!
//! Each monster is no longer a single capsule: it's a small hierarchy of parented
//! "bone" entities (pelvis -> torso -> head/jaw, articulated arms & legs, plus
//! per-kind bits like the Ogre's chainsaw or the Scrag's wings & tail). Every part
//! is a primitive mesh (box/capsule/sphere/cone) skinned with an AI-generated
//! texture. A procedural animation system drives the bones every frame: a walk
//! cycle, idle breathing, attack swings, pain flinch and a death topple.
//!
//! The skeleton is the entity hierarchy itself — rotating a joint swings everything
//! parented to it, exactly like a real bone rig. No binary mesh assets required.

use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::PI;

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
    /// Capsule: radius + cylinder length (total height = len + 2*r along local Y).
    Capsule { r: f32, len: f32 },
    Sphere(f32),
    /// Cone: base radius + height (points toward +Y by default).
    Cone { r: f32, h: f32 },
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
    match *shape {
        Shape::Box(h) => meshes.add(Cuboid::from_size(h * 2.0)),
        Shape::Capsule { r, len } => meshes.add(Capsule3d::new(r, len)),
        Shape::Sphere(r) => meshes.add(Sphere::new(r)),
        Shape::Cone { r, h } => meshes.add(Cone { radius: r, height: h }),
    }
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
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Box(Vec3::new(0.17, 0.15, 0.13)), pos: Vec3::new(0.0, -0.02, 0.03), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.85, 0.85, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Box(Vec3::new(0.2, 0.18, 0.14)), pos: Vec3::new(0.0, 0.3, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [1.0, 1.0, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Box(Vec3::new(0.25, 0.18, 0.16)), pos: Vec3::new(0.0, 0.32, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [1.0, 1.0, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Sphere(0.135), pos: Vec3::new(0.0, 0.26, -0.09), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-4.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.95, 1.0, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.085, 0.045, 0.075)), pos: Vec3::new(0.0, -0.07, -0.055), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.9, 0.92, 0.82], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.022), pos: Vec3::new(0.05, 0.025, -0.12), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.45, 0.12], emissive: [7.0, 0.9, 0.18], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.022), pos: Vec3::new(-0.05, 0.025, -0.12), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.45, 0.12], emissive: [7.0, 0.9, 0.18], anim: AnimRole::Static },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.075, len: 0.21 }, pos: Vec3::new(0.29, 0.11, 0.0), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(10.0, 0.0, 12.0), tex: TexId::SkinPrimary, tint: [0.95, 1.0, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.062, len: 0.2 }, pos: Vec3::new(0.0, -0.36, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-35.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.92, 0.98, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.078, len: 0.21 }, pos: Vec3::new(-0.29, 0.13, -0.02), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(-55.0, 0.0, -10.0), tex: TexId::SkinPrimary, tint: [0.95, 1.0, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Capsule { r: 0.062, len: 0.2 }, pos: Vec3::new(0.0, -0.36, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-40.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.92, 0.98, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "shotgun", parent: Some("farmR"), shape: Shape::Box(Vec3::new(0.04, 0.04, 0.3)), pos: Vec3::new(0.0, -0.34, -0.1), mesh_off: Vec3::new(0.0, 0.0, -0.18), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.5, 0.5, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.092, len: 0.26 }, pos: Vec3::new(0.11, -0.14, 0.01), mesh_off: Vec3::new(0.0, -0.22, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 3.0), tex: TexId::SkinPrimary, tint: [0.95, 1.0, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Capsule { r: 0.075, len: 0.27 }, pos: Vec3::new(0.0, -0.44, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.9, 0.9, 0.85], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.08, 0.05, 0.13)), pos: Vec3::new(0.0, -0.43, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.58, 0.52], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.092, len: 0.26 }, pos: Vec3::new(-0.11, -0.14, 0.01), mesh_off: Vec3::new(0.0, -0.22, 0.0), rot_deg: Vec3::new(-4.0, 0.0, -3.0), tex: TexId::SkinPrimary, tint: [0.95, 1.0, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Capsule { r: 0.075, len: 0.27 }, pos: Vec3::new(0.0, -0.44, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.9, 0.9, 0.85], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.08, 0.05, 0.13)), pos: Vec3::new(0.0, -0.43, -0.05), mesh_off: Vec3::new(0.0, 0.0, -0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.58, 0.52], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_enforcer() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Box(Vec3::new(0.2, 0.16, 0.15)), pos: Vec3::new(0.0, -0.05, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.85, 0.88, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Box(Vec3::new(0.24, 0.2, 0.16)), pos: Vec3::new(0.0, 0.3, 0.01), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.9, 0.9, 0.95], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Box(Vec3::new(0.3, 0.24, 0.19)), pos: Vec3::new(0.0, 0.4, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.8, 0.84, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "chestcore", parent: Some("chest"), shape: Shape::Sphere(0.07), pos: Vec3::new(0.0, 0.02, -0.19), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.1, 0.9, 1.0], emissive: [0.2, 5.0, 6.0], anim: AnimRole::Static },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.13, 0.14, 0.14)), pos: Vec3::new(0.0, 0.36, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.78, 0.82, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.1, 0.05, 0.1)), pos: Vec3::new(0.0, -0.12, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.88, 0.88, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "visor", parent: Some("head"), shape: Shape::Box(Vec3::new(0.11, 0.025, 0.02)), pos: Vec3::new(0.0, 0.0, -0.145), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [0.1, 0.9, 1.0], emissive: [0.2, 5.5, 6.5], anim: AnimRole::Static },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.08, len: 0.24 }, pos: Vec3::new(0.32, 0.16, 0.0), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(0.0, 0.0, 6.0), tex: TexId::SkinPrimary, tint: [0.9, 0.9, 0.95], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.07, len: 0.24 }, pos: Vec3::new(0.0, -0.4, 0.02), mesh_off: Vec3::new(0.0, -0.19, 0.0), rot_deg: Vec3::new(18.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.8, 0.84, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "fistL", parent: Some("farmL"), shape: Shape::Sphere(0.08), pos: Vec3::new(0.0, -0.38, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.88, 0.88, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "pauldronR", parent: Some("chest"), shape: Shape::Sphere(0.15), pos: Vec3::new(-0.34, 0.19, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.76, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.09, len: 0.24 }, pos: Vec3::new(-0.33, 0.12, 0.02), mesh_off: Vec3::new(0.0, -0.2, 0.0), rot_deg: Vec3::new(14.0, 0.0, -4.0), tex: TexId::SkinPrimary, tint: [0.9, 0.9, 0.95], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "gunR", parent: Some("uarmR"), shape: Shape::Box(Vec3::new(0.09, 0.26, 0.11)), pos: Vec3::new(0.0, -0.42, 0.04), mesh_off: Vec3::new(0.0, -0.18, 0.04), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.5, 0.55, 0.65], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "gunmuzzle", parent: Some("gunR"), shape: Shape::Cone { r: 0.07, h: 0.14 }, pos: Vec3::new(0.0, -0.34, 0.12), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(90.0, 0.0, 0.0), tex: TexId::None, tint: [0.1, 0.9, 1.0], emissive: [0.3, 6.0, 7.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.11, len: 0.28 }, pos: Vec3::new(0.13, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.9, 0.9, 0.95], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Capsule { r: 0.09, len: 0.3 }, pos: Vec3::new(0.0, -0.5, 0.0), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.8, 0.84, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.1, 0.06, 0.16)), pos: Vec3::new(0.0, -0.5, -0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.5, 0.55, 0.65], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.11, len: 0.28 }, pos: Vec3::new(-0.13, -0.12, 0.0), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.9, 0.9, 0.95], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Capsule { r: 0.09, len: 0.3 }, pos: Vec3::new(0.0, -0.5, 0.0), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.8, 0.84, 1.0], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.1, 0.06, 0.16)), pos: Vec3::new(0.0, -0.5, -0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.5, 0.55, 0.65], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_knight() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Box(Vec3::new(0.17, 0.14, 0.13)), pos: Vec3::new(0.0, -0.12, 0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.72, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Box(Vec3::new(0.2, 0.16, 0.13)), pos: Vec3::new(0.0, 0.28, -0.02), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.82, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Box(Vec3::new(0.26, 0.18, 0.16)), pos: Vec3::new(0.0, 0.3, -0.03), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.85, 0.87, 0.92], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.12, 0.13, 0.15)), pos: Vec3::new(0.0, 0.27, -0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.55, 0.57, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "visor", parent: Some("head"), shape: Shape::Cone { r: 0.1, h: 0.16 }, pos: Vec3::new(0.0, -0.02, -0.12), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-90.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.4, 0.42, 0.46], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "eyeSlit", parent: Some("head"), shape: Shape::Box(Vec3::new(0.07, 0.012, 0.02)), pos: Vec3::new(0.0, 0.0, -0.18), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.25, 0.05], emissive: [7.0, 0.7, 0.15], anim: AnimRole::Static },
        BoneSpec { name: "cape", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.22, 0.34, 0.03)), pos: Vec3::new(0.0, -0.04, 0.18), mesh_off: Vec3::new(0.0, -0.34, 0.0), rot_deg: Vec3::new(12.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.3, 0.3, 0.34], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Cape },
        BoneSpec { name: "shoulderL", parent: Some("chest"), shape: Shape::Sphere(0.1), pos: Vec3::new(0.28, 0.12, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.62, 0.68], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.07, len: 0.22 }, pos: Vec3::new(0.3, 0.06, 0.02), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(6.0, 0.0, 8.0), tex: TexId::SkinPrimary, tint: [0.8, 0.82, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.06, len: 0.2 }, pos: Vec3::new(0.0, -0.36, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(14.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.75, 0.77, 0.83], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "shoulderR", parent: Some("chest"), shape: Shape::Sphere(0.11), pos: Vec3::new(-0.29, 0.13, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.62, 0.68], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.075, len: 0.22 }, pos: Vec3::new(-0.31, 0.07, 0.04), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(24.0, 0.0, -8.0), tex: TexId::SkinPrimary, tint: [0.8, 0.82, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Capsule { r: 0.065, len: 0.2 }, pos: Vec3::new(0.0, -0.36, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(30.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.75, 0.77, 0.83], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sword", parent: Some("farmR"), shape: Shape::Box(Vec3::new(0.025, 0.5, 0.06)), pos: Vec3::new(0.0, -0.32, -0.02), mesh_off: Vec3::new(0.0, -0.5, 0.0), rot_deg: Vec3::new(-18.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.7, 0.72, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.085, len: 0.3 }, pos: Vec3::new(0.1, -0.14, 0.01), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(-4.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.82, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Capsule { r: 0.07, len: 0.28 }, pos: Vec3::new(0.0, -0.47, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.72, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.08, 0.05, 0.14)), pos: Vec3::new(0.0, -0.44, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.55, 0.57, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.085, len: 0.3 }, pos: Vec3::new(-0.1, -0.14, 0.01), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(-4.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.82, 0.88], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Capsule { r: 0.07, len: 0.28 }, pos: Vec3::new(0.0, -0.47, 0.0), mesh_off: Vec3::new(0.0, -0.21, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.72, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.08, 0.05, 0.14)), pos: Vec3::new(0.0, -0.44, 0.06), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.55, 0.57, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_scrag() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "torso", parent: None, shape: Shape::Capsule { r: 0.22, len: 0.34 }, pos: Vec3::new(0.0, 0.05, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(12.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.85, 1.0, 0.7], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Capsule { r: 0.2, len: 0.22 }, pos: Vec3::new(0.0, 0.34, -0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-18.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.95, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Sphere(0.2), pos: Vec3::new(0.0, 0.28, -0.12), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(10.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 1.0, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "snout", parent: Some("head"), shape: Shape::Cone { r: 0.12, h: 0.26 }, pos: Vec3::new(0.0, 0.0, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-100.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.75, 0.92, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.11, 0.04, 0.16)), pos: Vec3::new(0.0, -0.1, -0.1), mesh_off: Vec3::new(0.0, 0.0, -0.08), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.85, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "fangUpper", parent: Some("head"), shape: Shape::Cone { r: 0.03, h: 0.13 }, pos: Vec3::new(0.0, -0.08, -0.22), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(180.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.95, 0.85], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "fangLower", parent: Some("jaw"), shape: Shape::Cone { r: 0.025, h: 0.11 }, pos: Vec3::new(0.0, 0.04, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.95, 0.95, 0.85], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.045), pos: Vec3::new(0.1, 0.06, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.9, 0.2], emissive: [6.0, 4.5, 0.3], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.045), pos: Vec3::new(-0.1, 0.06, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.9, 0.2], emissive: [6.0, 4.5, 0.3], anim: AnimRole::Static },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.055, len: 0.16 }, pos: Vec3::new(0.2, 0.06, -0.04), mesh_off: Vec3::new(0.0, -0.13, 0.0), rot_deg: Vec3::new(30.0, 0.0, -40.0), tex: TexId::SkinPrimary, tint: [0.8, 0.95, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.045, len: 0.16 }, pos: Vec3::new(0.0, -0.27, 0.0), mesh_off: Vec3::new(0.0, -0.13, 0.0), rot_deg: Vec3::new(-55.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.82, 0.97, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "clawL", parent: Some("farmL"), shape: Shape::Cone { r: 0.025, h: 0.14 }, pos: Vec3::new(0.0, -0.25, 0.0), mesh_off: Vec3::new(0.0, -0.07, 0.0), rot_deg: Vec3::new(200.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.9, 0.9, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.055, len: 0.16 }, pos: Vec3::new(-0.2, 0.06, -0.04), mesh_off: Vec3::new(0.0, -0.13, 0.0), rot_deg: Vec3::new(30.0, 0.0, 40.0), tex: TexId::SkinPrimary, tint: [0.8, 0.95, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmR },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Capsule { r: 0.045, len: 0.16 }, pos: Vec3::new(0.0, -0.27, 0.0), mesh_off: Vec3::new(0.0, -0.13, 0.0), rot_deg: Vec3::new(-55.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.82, 0.97, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmR },
        BoneSpec { name: "clawR", parent: Some("farmR"), shape: Shape::Cone { r: 0.025, h: 0.14 }, pos: Vec3::new(0.0, -0.25, 0.0), mesh_off: Vec3::new(0.0, -0.07, 0.0), rot_deg: Vec3::new(200.0, 0.0, 0.0), tex: TexId::Bone, tint: [0.9, 0.9, 0.78], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "wingL", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.32, 0.24, 0.02)), pos: Vec3::new(0.16, 0.1, 0.06), mesh_off: Vec3::new(0.3, 0.0, 0.0), rot_deg: Vec3::new(8.0, -25.0, 28.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.92, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WingL },
        BoneSpec { name: "wingR", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.32, 0.24, 0.02)), pos: Vec3::new(-0.16, 0.1, 0.06), mesh_off: Vec3::new(-0.3, 0.0, 0.0), rot_deg: Vec3::new(8.0, 25.0, -28.0), tex: TexId::ArmorSecondary, tint: [0.7, 0.92, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WingR },
        BoneSpec { name: "tail1", parent: Some("torso"), shape: Shape::Capsule { r: 0.13, len: 0.26 }, pos: Vec3::new(0.0, -0.3, 0.04), mesh_off: Vec3::new(0.0, -0.18, 0.0), rot_deg: Vec3::new(-15.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.95, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
        BoneSpec { name: "tail2", parent: Some("tail1"), shape: Shape::Capsule { r: 0.08, len: 0.24 }, pos: Vec3::new(0.0, -0.46, 0.0), mesh_off: Vec3::new(0.0, -0.16, 0.0), rot_deg: Vec3::new(-18.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.72, 0.9, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
        BoneSpec { name: "tail3", parent: Some("tail2"), shape: Shape::Cone { r: 0.05, h: 0.3 }, pos: Vec3::new(0.0, -0.4, 0.0), mesh_off: Vec3::new(0.0, -0.15, 0.0), rot_deg: Vec3::new(160.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.65, 0.85, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Tail },
    ]
}

fn rig_ogre() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Box(Vec3::new(0.34, 0.22, 0.28)), pos: Vec3::new(0.0, -0.3, 0.05), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.75, 0.68, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "gut", parent: Some("pelvis"), shape: Shape::Sphere(0.42), pos: Vec3::new(0.0, 0.18, -0.08), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.9, 0.85, 0.8], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("gut"), shape: Shape::Box(Vec3::new(0.4, 0.26, 0.26)), pos: Vec3::new(0.0, 0.4, -0.14), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(18.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Sphere(0.17), pos: Vec3::new(0.0, 0.26, -0.18), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.82, 0.72, 0.64], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "jaw", parent: Some("head"), shape: Shape::Box(Vec3::new(0.13, 0.06, 0.11)), pos: Vec3::new(0.0, -0.1, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.85, 0.74, 0.66], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Jaw },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.028), pos: Vec3::new(0.07, 0.04, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.5, 0.1], emissive: [7.0, 0.5, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.028), pos: Vec3::new(-0.07, 0.04, -0.15), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.5, 0.1], emissive: [7.0, 0.5, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.13, len: 0.3 }, pos: Vec3::new(0.48, 0.16, 0.02), mesh_off: Vec3::new(0.0, -0.28, 0.0), rot_deg: Vec3::new(0.0, 0.0, 18.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.115, len: 0.3 }, pos: Vec3::new(0.0, -0.56, 0.0), mesh_off: Vec3::new(0.0, -0.27, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.71, 0.63], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.14, len: 0.3 }, pos: Vec3::new(-0.48, 0.18, 0.0), mesh_off: Vec3::new(0.0, -0.28, 0.0), rot_deg: Vec3::new(-35.0, 0.0, -15.0), tex: TexId::SkinPrimary, tint: [0.78, 0.7, 0.62], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Capsule { r: 0.12, len: 0.3 }, pos: Vec3::new(0.0, -0.58, 0.0), mesh_off: Vec3::new(0.0, -0.27, 0.0), rot_deg: Vec3::new(40.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.8, 0.71, 0.63], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sawBody", parent: Some("farmR"), shape: Shape::Box(Vec3::new(0.14, 0.12, 0.3)), pos: Vec3::new(0.0, -0.5, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.5, 0.45, 0.45], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sawBlade", parent: Some("sawBody"), shape: Shape::Box(Vec3::new(0.05, 0.04, 0.5)), pos: Vec3::new(0.0, 0.02, -0.78), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.85, 0.85, 0.9], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.17, len: 0.26 }, pos: Vec3::new(0.2, -0.22, 0.0), mesh_off: Vec3::new(0.0, -0.3, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.76, 0.68, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Capsule { r: 0.15, len: 0.2 }, pos: Vec3::new(0.0, -0.6, 0.0), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.74, 0.66, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "footL", parent: Some("shinL"), shape: Shape::Box(Vec3::new(0.16, 0.07, 0.24)), pos: Vec3::new(0.0, -0.5, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.62, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.17, len: 0.26 }, pos: Vec3::new(-0.2, -0.22, 0.0), mesh_off: Vec3::new(0.0, -0.3, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.76, 0.68, 0.6], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Capsule { r: 0.15, len: 0.2 }, pos: Vec3::new(0.0, -0.6, 0.0), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.74, 0.66, 0.58], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
        BoneSpec { name: "footR", parent: Some("shinR"), shape: Shape::Box(Vec3::new(0.16, 0.07, 0.24)), pos: Vec3::new(0.0, -0.5, -0.1), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.62, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::FootR },
    ]
}

fn rig_deathknight() -> Vec<BoneSpec> {
    vec![
        BoneSpec { name: "pelvis", parent: None, shape: Shape::Box(Vec3::new(0.26, 0.2, 0.2)), pos: Vec3::new(0.0, -0.15, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.75, 0.55, 0.55], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Pelvis },
        BoneSpec { name: "torso", parent: Some("pelvis"), shape: Shape::Box(Vec3::new(0.32, 0.26, 0.22)), pos: Vec3::new(0.0, 0.42, 0.0), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.5, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Torso },
        BoneSpec { name: "chest", parent: Some("torso"), shape: Shape::Box(Vec3::new(0.42, 0.28, 0.26)), pos: Vec3::new(0.0, 0.46, -0.03), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(8.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.65, 0.45, 0.45], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Chest },
        BoneSpec { name: "head", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.16, 0.18, 0.16)), pos: Vec3::new(0.0, 0.4, 0.04), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(-6.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.55, 0.4, 0.4], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Head },
        BoneSpec { name: "hornL", parent: Some("head"), shape: Shape::Cone { r: 0.05, h: 0.34 }, pos: Vec3::new(0.12, 0.14, 0.04), mesh_off: Vec3::new(0.0, 0.17, 0.0), rot_deg: Vec3::new(-22.0, 0.0, 38.0), tex: TexId::Bone, tint: [0.5, 0.42, 0.4], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "hornR", parent: Some("head"), shape: Shape::Cone { r: 0.05, h: 0.34 }, pos: Vec3::new(-0.12, 0.14, 0.04), mesh_off: Vec3::new(0.0, 0.17, 0.0), rot_deg: Vec3::new(-22.0, 0.0, -38.0), tex: TexId::Bone, tint: [0.5, 0.42, 0.4], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "eyeL", parent: Some("head"), shape: Shape::Sphere(0.035), pos: Vec3::new(0.07, 0.0, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.25, 0.08], emissive: [8.0, 0.6, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "eyeR", parent: Some("head"), shape: Shape::Sphere(0.035), pos: Vec3::new(-0.07, 0.0, -0.16), mesh_off: Vec3::new(0.0, 0.0, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::None, tint: [1.0, 0.25, 0.08], emissive: [8.0, 0.6, 0.1], anim: AnimRole::Static },
        BoneSpec { name: "cape", parent: Some("chest"), shape: Shape::Box(Vec3::new(0.34, 0.55, 0.04)), pos: Vec3::new(0.0, -0.1, 0.24), mesh_off: Vec3::new(0.0, -0.55, 0.02), rot_deg: Vec3::new(14.0, 0.0, 0.0), tex: TexId::ArmorSecondary, tint: [0.6, 0.12, 0.12], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Cape },
        BoneSpec { name: "uarmL", parent: Some("chest"), shape: Shape::Capsule { r: 0.11, len: 0.26 }, pos: Vec3::new(0.46, 0.16, 0.0), mesh_off: Vec3::new(0.0, -0.24, 0.0), rot_deg: Vec3::new(0.0, 0.0, 12.0), tex: TexId::SkinPrimary, tint: [0.7, 0.5, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::UpperArmL },
        BoneSpec { name: "farmL", parent: Some("uarmL"), shape: Shape::Capsule { r: 0.09, len: 0.26 }, pos: Vec3::new(0.0, -0.48, 0.0), mesh_off: Vec3::new(0.0, -0.22, 0.0), rot_deg: Vec3::new(18.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.65, 0.45, 0.45], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ForearmL },
        BoneSpec { name: "uarmR", parent: Some("chest"), shape: Shape::Capsule { r: 0.12, len: 0.27 }, pos: Vec3::new(-0.46, 0.16, 0.0), mesh_off: Vec3::new(0.0, -0.25, 0.0), rot_deg: Vec3::new(0.0, 0.0, -10.0), tex: TexId::SkinPrimary, tint: [0.7, 0.5, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::WeaponArm },
        BoneSpec { name: "farmR", parent: Some("uarmR"), shape: Shape::Capsule { r: 0.1, len: 0.26 }, pos: Vec3::new(0.0, -0.5, 0.0), mesh_off: Vec3::new(0.0, -0.23, 0.0), rot_deg: Vec3::new(30.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.65, 0.45, 0.45], emissive: [0.0, 0.0, 0.0], anim: AnimRole::Static },
        BoneSpec { name: "sword", parent: Some("farmR"), shape: Shape::Box(Vec3::new(0.05, 0.85, 0.02)), pos: Vec3::new(0.0, -0.46, -0.04), mesh_off: Vec3::new(0.0, -0.62, 0.0), rot_deg: Vec3::new(-30.0, 0.0, 0.0), tex: TexId::DemonMetal, tint: [0.9, 0.4, 0.25], emissive: [5.0, 0.8, 0.15], anim: AnimRole::Static },
        BoneSpec { name: "thighL", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.13, len: 0.32 }, pos: Vec3::new(0.16, -0.18, 0.0), mesh_off: Vec3::new(0.0, -0.29, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.5, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighL },
        BoneSpec { name: "shinL", parent: Some("thighL"), shape: Shape::Capsule { r: 0.11, len: 0.36 }, pos: Vec3::new(0.0, -0.58, 0.0), mesh_off: Vec3::new(0.0, -0.29, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.42, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinL },
        BoneSpec { name: "thighR", parent: Some("pelvis"), shape: Shape::Capsule { r: 0.13, len: 0.32 }, pos: Vec3::new(-0.16, -0.18, 0.0), mesh_off: Vec3::new(0.0, -0.29, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.7, 0.5, 0.5], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ThighR },
        BoneSpec { name: "shinR", parent: Some("thighR"), shape: Shape::Capsule { r: 0.11, len: 0.36 }, pos: Vec3::new(0.0, -0.58, 0.0), mesh_off: Vec3::new(0.0, -0.29, 0.0), rot_deg: Vec3::new(0.0, 0.0, 0.0), tex: TexId::SkinPrimary, tint: [0.6, 0.42, 0.42], emissive: [0.0, 0.0, 0.0], anim: AnimRole::ShinR },
    ]
}
