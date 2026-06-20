//! The mission map: "Dimension of the Doomed".
//!
//! Built entirely from axis-aligned brushes (boxes). A small builder accumulates
//! collider AABBs and spawns visual cuboids (one shared unit-cube mesh, scaled per
//! brush). It also produces a SpawnPlan describing where enemies, items, the key,
//! the locked door and the exit go — consumed by the enemy/pickup modules.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::common::*;
use crate::physics::Aabb;

pub struct LevelPlugin;
impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerStart>()
            .init_resource::<SpawnPlan>()
            .init_resource::<LavaVolumes>();
    }
}

/// System wrapper that builds the level on entering Playing.
#[allow(clippy::too_many_arguments)]
pub fn setup_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut colliders: ResMut<WorldColliders>,
    mut start: ResMut<PlayerStart>,
    mut plan: ResMut<SpawnPlan>,
    mut lava: ResMut<LavaVolumes>,
    mut gfx: ResMut<GfxAssets>,
    mut mission: ResMut<Mission>,
    asset_server: Res<AssetServer>,
) {
    // Reset mission state for a fresh run.
    mission.has_key = false;
    mission.kills = 0;
    mission.total_enemies = 0;
    mission.objective = "Find the Silver Key".into();

    build_level(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut colliders,
        &mut start,
        &mut plan,
        &mut lava,
        &mut gfx,
        &asset_server,
    );
}

/// Where the player (re)spawns, with initial yaw.
#[derive(Resource, Default)]
pub struct PlayerStart {
    pub pos: Vec3,
    pub yaw: f32,
}

/// Lava/hazard volumes that hurt anything standing in them.
#[derive(Resource, Default)]
pub struct LavaVolumes {
    pub volumes: Vec<Aabb>,
}

// What kind of monster to place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MonsterKind {
    Grunt,
    Enforcer,
    Knight,
    Scrag,
    Ogre,
    DeathKnight,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Health(u32),
    MegaHealth,
    ArmorGreen,
    ArmorYellow,
    Shells(u32),
    Nails(u32),
    Rockets(u32),
    Cells(u32),
    WeaponSuperShotgun,
    WeaponNailgun,
    WeaponGrenade,
    WeaponRocket,
    WeaponLightning,
    SilverKey,
}

#[derive(Clone, Copy)]
pub struct MonsterSpawn {
    pub kind: MonsterKind,
    pub pos: Vec3,
}
#[derive(Clone, Copy)]
pub struct ItemSpawn {
    pub kind: ItemKind,
    pub pos: Vec3,
}

#[derive(Resource, Default)]
pub struct SpawnPlan {
    pub monsters: Vec<MonsterSpawn>,
    pub items: Vec<ItemSpawn>,
    pub exit: Option<Vec3>,
}

// ----------------------------------------------------------------------------
// Materials palette
// ----------------------------------------------------------------------------
struct Mats {
    floor: Handle<StandardMaterial>,
    wall: Handle<StandardMaterial>,
    trim: Handle<StandardMaterial>,
    ceiling: Handle<StandardMaterial>,
    metal: Handle<StandardMaterial>,
    lava: Handle<StandardMaterial>,
    slipgate: Handle<StandardMaterial>,
    door: Handle<StandardMaterial>,
}

/// Loader setting that makes a texture wrap (tile) instead of clamping at the
/// edges — required because brushes carry world-scaled UVs that exceed 0..1.
fn repeat_sampler(s: &mut ImageLoaderSettings) {
    s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
}

/// A surface material skinned with a seamless, tiling albedo texture.
fn tex_mat(
    m: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
    path: &str,
    rough: f32,
    metal: f32,
) -> Handle<StandardMaterial> {
    let img = assets.load_builder().with_settings(repeat_sampler).load(path.to_string());
    m.add(StandardMaterial {
        base_color_texture: Some(img),
        perceptual_roughness: rough,
        metallic: metal,
        ..default()
    })
}

fn make_mats(m: &mut Assets<StandardMaterial>, assets: &AssetServer) -> Mats {
    Mats {
        floor: tex_mat(m, assets, "textures/world/floor.png", 0.95, 0.0),
        wall: tex_mat(m, assets, "textures/world/wall.png", 0.9, 0.05),
        trim: tex_mat(m, assets, "textures/world/trim.png", 0.6, 0.3),
        ceiling: tex_mat(m, assets, "textures/world/ceiling.png", 1.0, 0.0),
        metal: tex_mat(m, assets, "textures/world/metal.png", 0.4, 0.75),
        lava: {
            // Textured for the molten-crust detail, but still self-lit so it glows.
            let img = assets.load_builder().with_settings(repeat_sampler).load("textures/world/lava.png");
            m.add(StandardMaterial {
                base_color_texture: Some(img),
                emissive: LinearRgba::rgb(5.0, 1.2, 0.1),
                perceptual_roughness: 0.6,
                ..default()
            })
        },
        slipgate: m.add(StandardMaterial {
            base_color: rgb(0.5, 0.2, 0.9),
            emissive: LinearRgba::rgb(1.2, 0.4, 3.0),
            ..default()
        }),
        door: tex_mat(m, assets, "textures/world/door.png", 0.6, 0.4),
    }
}

// ----------------------------------------------------------------------------
// Brush builder
// ----------------------------------------------------------------------------
const WALL_T: f32 = 0.5;

/// World-space size (in meters) that one texture tile covers. Brush faces get
/// UVs scaled by their world dimensions / this, so the texel density is uniform
/// across the whole level (the classic Quake world-aligned-texture look) and
/// adjacent brushes line up.
const TEXEL: f32 = 2.5;

/// Build a box brush as its own mesh: positions are local (centered on the
/// brush), but UVs are derived from the *world* coordinates so the texture
/// tiles consistently and seams between brushes align. Each of the 6 faces maps
/// the two in-plane world axes to (u, v).
fn box_mesh(min: Vec3, max: Vec3) -> Mesh {
    let center = (min + max) * 0.5;
    let s = 1.0 / TEXEL;
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);

    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut nor: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut uv: Vec<[f32; 2]> = Vec::with_capacity(24);
    let mut idx: Vec<u32> = Vec::with_capacity(36);

    let mut quad = |p: [Vec3; 4], n: [f32; 3], u: [[f32; 2]; 4]| {
        let base = pos.len() as u32;
        for k in 0..4 {
            let w = p[k] - center;
            pos.push([w.x, w.y, w.z]);
            nor.push(n);
            uv.push(u[k]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // +X / -X faces: u <- z, v <- y
    quad(
        [Vec3::new(x1, y0, z1), Vec3::new(x1, y0, z0), Vec3::new(x1, y1, z0), Vec3::new(x1, y1, z1)],
        [1.0, 0.0, 0.0],
        [[z1 * s, y0 * s], [z0 * s, y0 * s], [z0 * s, y1 * s], [z1 * s, y1 * s]],
    );
    quad(
        [Vec3::new(x0, y0, z0), Vec3::new(x0, y0, z1), Vec3::new(x0, y1, z1), Vec3::new(x0, y1, z0)],
        [-1.0, 0.0, 0.0],
        [[z0 * s, y0 * s], [z1 * s, y0 * s], [z1 * s, y1 * s], [z0 * s, y1 * s]],
    );
    // +Y / -Y faces (top/bottom): u <- x, v <- z
    quad(
        [Vec3::new(x0, y1, z1), Vec3::new(x1, y1, z1), Vec3::new(x1, y1, z0), Vec3::new(x0, y1, z0)],
        [0.0, 1.0, 0.0],
        [[x0 * s, z1 * s], [x1 * s, z1 * s], [x1 * s, z0 * s], [x0 * s, z0 * s]],
    );
    quad(
        [Vec3::new(x0, y0, z0), Vec3::new(x1, y0, z0), Vec3::new(x1, y0, z1), Vec3::new(x0, y0, z1)],
        [0.0, -1.0, 0.0],
        [[x0 * s, z0 * s], [x1 * s, z0 * s], [x1 * s, z1 * s], [x0 * s, z1 * s]],
    );
    // +Z / -Z faces: u <- x, v <- y
    quad(
        [Vec3::new(x0, y0, z1), Vec3::new(x1, y0, z1), Vec3::new(x1, y1, z1), Vec3::new(x0, y1, z1)],
        [0.0, 0.0, 1.0],
        [[x0 * s, y0 * s], [x1 * s, y0 * s], [x1 * s, y1 * s], [x0 * s, y1 * s]],
    );
    quad(
        [Vec3::new(x1, y0, z0), Vec3::new(x0, y0, z0), Vec3::new(x0, y1, z0), Vec3::new(x1, y1, z0)],
        [0.0, 0.0, -1.0],
        [[x1 * s, y0 * s], [x0 * s, y0 * s], [x0 * s, y1 * s], [x1 * s, y1 * s]],
    );

    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m
}

struct Build<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    colliders: &'a mut Vec<Aabb>,
}

impl<'a, 'w, 's> Build<'a, 'w, 's> {
    fn visual(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
        let center = (min + max) * 0.5;
        let mesh = self.meshes.add(box_mesh(min, max));
        self.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(center),
            LevelEntity,
        ));
    }

    /// Solid brush: visual + collider.
    fn solid(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
        self.visual(min, max, mat);
        self.colliders.push(Aabb::from_corners(min, max));
    }

    /// Decorative brush: visual only (no collision), e.g. lava surface.
    fn deco(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
        self.visual(min, max, mat);
    }

    /// Wall running along X at depth `z`, spanning [x0,x1], height [y0,y1],
    /// with door-gaps (intervals on the X axis) carved out.
    fn wall_x(
        &mut self,
        x0: f32,
        x1: f32,
        z: f32,
        y0: f32,
        y1: f32,
        mat: Handle<StandardMaterial>,
        gaps: &[(f32, f32)],
    ) {
        for (a, b) in subtract(x0, x1, gaps) {
            self.solid(
                Vec3::new(a, y0, z - WALL_T * 0.5),
                Vec3::new(b, y1, z + WALL_T * 0.5),
                mat.clone(),
            );
        }
    }

    /// Wall running along Z at position `x`, spanning [z0,z1], height [y0,y1].
    fn wall_z(
        &mut self,
        z0: f32,
        z1: f32,
        x: f32,
        y0: f32,
        y1: f32,
        mat: Handle<StandardMaterial>,
        gaps: &[(f32, f32)],
    ) {
        for (a, b) in subtract(z0, z1, gaps) {
            self.solid(
                Vec3::new(x - WALL_T * 0.5, y0, a),
                Vec3::new(x + WALL_T * 0.5, y1, b),
                mat.clone(),
            );
        }
    }

    fn floor(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, mat: Handle<StandardMaterial>) {
        self.solid(Vec3::new(x0, y - 0.5, z0), Vec3::new(x1, y, z1), mat);
    }
    fn ceiling(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, mat: Handle<StandardMaterial>) {
        self.solid(Vec3::new(x0, y, z0), Vec3::new(x1, y + 0.5, z1), mat);
    }
}

/// Subtract a set of intervals (gaps) from [lo,hi], returning the remaining segments.
fn subtract(lo: f32, hi: f32, gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut segs = vec![(lo, hi)];
    for &(ga, gb) in gaps {
        let mut next = Vec::new();
        for (a, b) in segs {
            if gb <= a || ga >= b {
                next.push((a, b)); // gap doesn't overlap this segment
            } else {
                if ga > a {
                    next.push((a, ga));
                }
                if gb < b {
                    next.push((gb, b));
                }
            }
        }
        segs = next;
    }
    segs.into_iter().filter(|(a, b)| b - a > 0.05).collect()
}

// ----------------------------------------------------------------------------
// The map
// ----------------------------------------------------------------------------
/// Build the whole level. Called on (re)start. Returns nothing; fills resources.
pub fn build_level(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    colliders: &mut WorldColliders,
    start: &mut PlayerStart,
    plan: &mut SpawnPlan,
    lava: &mut LavaVolumes,
    gfx: &mut GfxAssets,
    asset_server: &AssetServer,
) {
    let unit = meshes.add(Cuboid::from_size(Vec3::ONE));
    gfx.unit_cube = unit.clone();
    gfx.sphere = meshes.add(Sphere::new(0.5));
    gfx.small_sphere = meshes.add(Sphere::new(0.12));
    let unlit = |m: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| {
        m.add(StandardMaterial {
            base_color: c,
            emissive: e,
            unlit: false,
            ..default()
        })
    };
    gfx.white_unlit = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    gfx.blood = unlit(materials, rgb(0.45, 0.02, 0.02), LinearRgba::rgb(0.2, 0.0, 0.0));
    gfx.gib = unlit(materials, rgb(0.6, 0.08, 0.08), LinearRgba::rgb(0.15, 0.0, 0.0));
    gfx.spark = unlit(materials, rgb(1.0, 0.8, 0.3), LinearRgba::rgb(6.0, 3.5, 0.6));
    gfx.smoke = materials.add(StandardMaterial { base_color: rgb(0.18, 0.18, 0.2), ..default() });
    gfx.muzzle = unlit(materials, rgb(1.0, 0.9, 0.5), LinearRgba::rgb(8.0, 6.0, 2.0));
    gfx.explosion = unlit(materials, rgb(1.0, 0.6, 0.2), LinearRgba::rgb(9.0, 3.5, 0.6));
    gfx.rocket = unlit(materials, rgb(1.0, 0.7, 0.3), LinearRgba::rgb(5.0, 2.0, 0.4));
    gfx.grenade = unlit(materials, rgb(0.25, 0.5, 0.2), LinearRgba::rgb(0.2, 0.6, 0.1));
    gfx.nail = unlit(materials, rgb(0.8, 0.8, 0.9), LinearRgba::rgb(1.5, 1.5, 2.0));
    gfx.plasma = unlit(materials, rgb(0.5, 0.7, 1.0), LinearRgba::rgb(1.0, 3.0, 8.0));

    let mats = make_mats(materials, asset_server);
    gfx.lava_mat = mats.lava.clone();
    colliders.solids.clear();
    plan.monsters.clear();
    plan.items.clear();
    lava.volumes.clear();

    let mut b = Build {
        commands,
        meshes,
        colliders: &mut colliders.solids,
    };

    // Player starts in the slipgate room facing north (-Z).
    start.pos = Vec3::new(0.0, 1.0, 2.0);
    start.yaw = 0.0;

    // === Start room S: x[-6,6] z[-6,6], h5, opening north (z=-6) x[-2,2] ===
    room(&mut b, &mats, -6.0, 6.0, -6.0, 6.0, 0.0, 5.0,
        &[Wall::N((-2.0, 2.0))]);
    // slipgate decoration behind spawn
    b.deco(Vec3::new(-2.0, 0.1, 5.2), Vec3::new(2.0, 4.0, 5.6), mats.slipgate.clone());
    plan.items.push(ItemSpawn { kind: ItemKind::ArmorGreen, pos: Vec3::new(-4.0, 0.6, -3.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Shells(15), pos: Vec3::new(4.0, 0.6, -3.0) });

    // === Corridor C1: x[-2,2] z[-14,-6], h4 ===
    corridor_z(&mut b, &mats, -2.0, 2.0, -14.0, -6.0, 0.0, 4.0);

    // === Room A "Grunt Hall": x[-8,8] z[-28,-14], h6, open S(z=-14) x[-2,2], open E(x=8) z[-25,-21] ===
    room(&mut b, &mats, -8.0, 8.0, -28.0, -14.0, 0.0, 6.0,
        &[Wall::S((-2.0, 2.0)), Wall::E((-25.0, -21.0))]);
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Grunt, pos: Vec3::new(-4.0, 1.0, -24.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Grunt, pos: Vec3::new(4.0, 1.0, -25.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Grunt, pos: Vec3::new(0.0, 1.0, -27.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Shells(30), pos: Vec3::new(-6.0, 0.6, -26.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Health(25), pos: Vec3::new(6.0, 0.6, -26.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::WeaponSuperShotgun, pos: Vec3::new(0.0, 0.6, -18.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Shells(15), pos: Vec3::new(2.0, 0.6, -18.0) });

    // === Corridor C2 with LAVA: x[8,20] z[-25,-21], h5; lava gap in the middle ===
    corridor_x(&mut b, &mats, 8.0, 20.0, -25.0, -21.0, 0.0, 5.0);
    // carve a lava pit: remove floor x[12,16] and drop it; add lava below
    // (we already laid a floor in corridor_x; overlay a recessed lava strip)
    b.deco(Vec3::new(12.0, -0.4, -25.0), Vec3::new(16.0, -0.2, -21.0), mats.lava.clone());
    lava.volumes.push(Aabb::from_corners(
        Vec3::new(12.0, -1.0, -25.0),
        Vec3::new(16.0, 0.4, -21.0),
    ));
    // (Players jump the 4 m gap; we leave a thin lip so it's a real gap.)

    // === Room B "Ogre Ledge": x[20,38] z[-30,-14], h9; open W(x=20) z[-25,-21], open S(z=-14) x[26,30] ===
    room(&mut b, &mats, 20.0, 38.0, -30.0, -14.0, 0.0, 9.0,
        &[Wall::W((-25.0, -21.0)), Wall::S((26.0, 30.0))]);
    // two side ledges at y=3
    b.solid(Vec3::new(20.0, 2.5, -30.0), Vec3::new(25.0, 3.0, -24.0), mats.metal.clone());
    b.solid(Vec3::new(33.0, 2.5, -30.0), Vec3::new(38.0, 3.0, -24.0), mats.metal.clone());
    // stairs up to the right ledge
    stairs(&mut b, &mats, 31.0, 33.0, -16.0, 3.0, 0.0, 6, Vec3::Z * -1.0);
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Ogre, pos: Vec3::new(22.5, 3.6, -27.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Ogre, pos: Vec3::new(35.5, 3.6, -27.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Knight, pos: Vec3::new(29.0, 1.0, -20.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::WeaponNailgun, pos: Vec3::new(35.5, 3.6, -27.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Nails(75), pos: Vec3::new(22.5, 3.6, -27.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::WeaponGrenade, pos: Vec3::new(22.5, 3.6, -26.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Rockets(15), pos: Vec3::new(24.5, 3.6, -27.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Health(25), pos: Vec3::new(29.0, 0.6, -28.0) });

    // === Corridor C3: x[26,30] z[-14,-8], h5 ===
    corridor_z(&mut b, &mats, 26.0, 30.0, -14.0, -8.0, 0.0, 5.0);

    // === Atrium "The Pit": x[18,40] z[-8,14], h12; open N(z=-8) x[26,30], open E(x=40) z[0,4], door W is none; locked door on N? ===
    // Exits: N corridor back to C3 (open), E to Key Vault, and a LOCKED door on the EAST upper? Keep: E open to vault corridor; locked door to final on the SOUTH wall (z=14).
    room(&mut b, &mats, 18.0, 40.0, -8.0, 14.0, 0.0, 12.0,
        &[Wall::N((26.0, 30.0)), Wall::E((0.0, 4.0)), Wall::S((27.0, 31.0))]);
    // central pillars
    b.solid(Vec3::new(24.0, 0.0, 0.0), Vec3::new(26.0, 8.0, 2.0), mats.trim.clone());
    b.solid(Vec3::new(32.0, 0.0, 4.0), Vec3::new(34.0, 8.0, 6.0), mats.trim.clone());
    // a raised platform (balcony) with the rocket launcher, reached by stairs
    b.solid(Vec3::new(34.0, 3.0, -8.0), Vec3::new(40.0, 3.5, -2.0), mats.metal.clone());
    stairs(&mut b, &mats, 30.0, 34.0, -3.0, 3.5, 0.0, 7, Vec3::X);
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Knight, pos: Vec3::new(22.0, 1.0, 6.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Scrag, pos: Vec3::new(30.0, 5.0, 8.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Scrag, pos: Vec3::new(36.0, 6.0, 10.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Enforcer, pos: Vec3::new(20.0, 1.0, 12.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::WeaponRocket, pos: Vec3::new(37.0, 4.1, -5.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Rockets(15), pos: Vec3::new(37.0, 4.1, -3.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::ArmorYellow, pos: Vec3::new(20.0, 0.6, -5.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Shells(20), pos: Vec3::new(22.0, 0.6, 8.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Nails(30), pos: Vec3::new(20.0, 0.6, 10.0) });

    // === Corridor C4 to Key Vault: x[40,46] z[0,4], h5 ===
    corridor_x(&mut b, &mats, 40.0, 46.0, 0.0, 4.0, 0.0, 5.0);

    // === Key Vault: x[46,58] z[-4,8], h6; open W(x=46) z[0,4] ===
    room(&mut b, &mats, 46.0, 58.0, -4.0, 8.0, 0.0, 6.0, &[Wall::W((0.0, 4.0))]);
    b.deco(Vec3::new(50.0, 0.1, 1.0), Vec3::new(54.0, 0.2, 5.0), mats.trim.clone());
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::DeathKnight, pos: Vec3::new(54.0, 1.0, 2.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::SilverKey, pos: Vec3::new(52.0, 1.0, 2.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::MegaHealth, pos: Vec3::new(48.0, 0.8, 6.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Cells(50), pos: Vec3::new(56.0, 0.8, 6.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::WeaponLightning, pos: Vec3::new(48.0, 0.8, -2.0) });

    // === Locked door on the Atrium south wall (z=14), gap x[27,31] ===
    let door_aabb = Aabb::from_corners(Vec3::new(27.0, 0.0, 13.7), Vec3::new(31.0, 5.0, 14.3));
    let door_center = door_aabb.center();
    let solid_index = b.colliders.len();
    b.colliders.push(door_aabb);
    let door_mesh = b.meshes.add(box_mesh(door_aabb.min, door_aabb.max));
    b.commands.spawn((
        Mesh3d(door_mesh),
        MeshMaterial3d(mats.door.clone()),
        Transform::from_translation(door_center),
        Door {
            solid_index,
            closed_pos: door_center,
            open_offset: Vec3::new(0.0, 5.2, 0.0),
            opening: false,
            opened: false,
            t: 0.0,
        },
        LevelEntity,
    ));

    // === Corridor C5: x[27,31] z[14,22], h5 ===
    corridor_z(&mut b, &mats, 27.0, 31.0, 14.0, 22.0, 0.0, 5.0);

    // === Final Chamber: x[20,38] z[22,40], h9; open N(z=22) x[27,31] ===
    room(&mut b, &mats, 20.0, 38.0, 22.0, 40.0, 0.0, 9.0, &[Wall::N((27.0, 31.0))]);
    // exit slipgate at far end
    b.deco(Vec3::new(26.0, 0.1, 39.4), Vec3::new(32.0, 4.5, 39.8), mats.slipgate.clone());
    plan.exit = Some(Vec3::new(29.0, 1.5, 38.5));
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Ogre, pos: Vec3::new(24.0, 1.0, 34.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Knight, pos: Vec3::new(34.0, 1.0, 34.0) });
    plan.monsters.push(MonsterSpawn { kind: MonsterKind::Enforcer, pos: Vec3::new(29.0, 1.0, 36.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Health(25), pos: Vec3::new(22.0, 0.6, 24.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Rockets(15), pos: Vec3::new(36.0, 0.6, 24.0) });
    plan.items.push(ItemSpawn { kind: ItemKind::Shells(20), pos: Vec3::new(29.0, 0.6, 24.0) });

    // ---- lights ----
    spawn_lights(b.commands);
}

// Wall descriptor for `room`: which wall and the gap interval on it.
enum Wall {
    N((f32, f32)),
    S((f32, f32)),
    E((f32, f32)),
    W((f32, f32)),
}

#[allow(clippy::too_many_arguments)]
fn room(
    b: &mut Build,
    mats: &Mats,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    h: f32,
    openings: &[Wall],
) {
    let y1 = y + h;
    b.floor(x0, x1, z0, z1, y, mats.floor.clone());
    b.ceiling(x0, x1, z0, z1, y1, mats.ceiling.clone());
    let mut north = vec![];
    let mut south = vec![];
    let mut east = vec![];
    let mut west = vec![];
    for o in openings {
        match o {
            Wall::N(g) => north.push(*g),
            Wall::S(g) => south.push(*g),
            Wall::E(g) => east.push(*g),
            Wall::W(g) => west.push(*g),
        }
    }
    // North wall at z0, south at z1 (north = -Z).
    b.wall_x(x0, x1, z0, y, y1, mats.wall.clone(), &north);
    b.wall_x(x0, x1, z1, y, y1, mats.wall.clone(), &south);
    b.wall_z(z0, z1, x1, y, y1, mats.wall.clone(), &east);
    b.wall_z(z0, z1, x0, y, y1, mats.wall.clone(), &west);
}

/// Corridor running along Z (walls on x sides, floor+ceiling, open ends).
fn corridor_z(b: &mut Build, mats: &Mats, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32) {
    let y1 = y + h;
    b.floor(x0, x1, z0, z1, y, mats.floor.clone());
    b.ceiling(x0, x1, z0, z1, y1, mats.ceiling.clone());
    b.wall_z(z0, z1, x0, y, y1, mats.wall.clone(), &[]);
    b.wall_z(z0, z1, x1, y, y1, mats.wall.clone(), &[]);
}

/// Corridor running along X (walls on z sides).
fn corridor_x(b: &mut Build, mats: &Mats, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32) {
    let y1 = y + h;
    b.floor(x0, x1, z0, z1, y, mats.floor.clone());
    b.ceiling(x0, x1, z0, z1, y1, mats.ceiling.clone());
    b.wall_x(x0, x1, z0, y, y1, mats.wall.clone(), &[]);
    b.wall_x(x0, x1, z1, y, y1, mats.wall.clone(), &[]);
}

/// A simple staircase: `steps` steps rising to `top_y`, spanning x[x0,x1],
/// advancing along `dir` (unit Z or X) from `base`.
/// A staircase of `steps` rising from `base_y` to `top_y`, the treads spanning
/// the cross-axis interval [a0,a1] and marching from `base_along` along `dir`
/// (which must be unit +/-X or +/-Z). Each step is a solid box up to its height.
#[allow(clippy::too_many_arguments)]
fn stairs(
    b: &mut Build,
    mats: &Mats,
    a0: f32,
    a1: f32,
    base_along: f32,
    top_y: f32,
    base_y: f32,
    steps: u32,
    dir: Vec3,
) {
    let rise = (top_y - base_y) / steps as f32;
    let depth = 0.55;
    for i in 0..steps {
        let h = base_y + rise * (i as f32 + 1.0);
        let off = i as f32 * depth;
        if dir.z.abs() > 0.5 {
            let s = dir.z.signum();
            let z0 = base_along + s * off;
            let z1 = z0 + s * depth;
            b.solid(
                Vec3::new(a0, base_y - 0.5, z0.min(z1)),
                Vec3::new(a1, h, z0.max(z1)),
                mats.trim.clone(),
            );
        } else {
            let s = dir.x.signum();
            let x0 = base_along + s * off;
            let x1 = x0 + s * depth;
            b.solid(
                Vec3::new(x0.min(x1), base_y - 0.5, a0),
                Vec3::new(x0.max(x1), h, a1),
                mats.trim.clone(),
            );
        }
    }
}

fn spawn_lights(commands: &mut Commands) {
    // Dim ambient directional so nothing is pitch black.
    commands.spawn((
        DirectionalLight {
            color: rgb(0.6, 0.6, 0.75),
            illuminance: 2700.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(10.0, 30.0, 5.0).looking_at(Vec3::new(20.0, 0.0, 0.0), Vec3::Y),
        LevelEntity,
    ));
    // Warm point lights through the rooms.
    let spots = [
        (Vec3::new(0.0, 4.0, 0.0), rgb(1.0, 0.8, 0.6), 600_000.0),
        (Vec3::new(0.0, 4.5, -22.0), rgb(1.0, 0.7, 0.5), 800_000.0),
        (Vec3::new(14.0, 3.5, -23.0), rgb(1.0, 0.4, 0.1), 500_000.0), // lava glow
        (Vec3::new(29.0, 6.0, -22.0), rgb(0.9, 0.8, 0.7), 1_200_000.0),
        (Vec3::new(29.0, 8.0, 3.0), rgb(0.8, 0.85, 1.0), 1_500_000.0),
        (Vec3::new(52.0, 4.0, 2.0), rgb(0.9, 0.85, 0.5), 800_000.0),
        (Vec3::new(29.0, 6.0, 32.0), rgb(0.7, 0.6, 0.9), 1_000_000.0),
    ];
    for (pos, color, intensity) in spots {
        commands.spawn((
            PointLight {
                color,
                intensity,
                range: 40.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(pos),
            LevelEntity,
        ));
    }
}
