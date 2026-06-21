//! Level system: themed material palettes, the brush-building API every level
//! is authored against, and the per-level setup that fills the mission
//! resources. The individual maps live in `src/levels/` and are dispatched from
//! here by index.
//!
//! ## Authoring a level
//!
//! Each level is a `pub fn build(b: &mut Build)` in `src/levels/levelN.rs`. It
//! uses the `Build` helpers to lay down brushes (boxes that are both a textured
//! mesh and a collider) and to register the spawn plan. The contract every
//! level must satisfy:
//!
//! 1. set `b.start.pos` / `b.start.yaw` (player spawn),
//! 2. carve a path of `room`/`corridor` brushes from the spawn to the exit,
//! 3. place the Silver Key (`b.item(ItemKind::SilverKey, ..)`) somewhere guarded,
//! 4. gate the exit behind a locked `b.door(..)` (opens once the key is held),
//! 5. mark the exit with `b.exit(pos)` + a `b.slipgate(..)` visual,
//! 6. populate `b.monster(..)`, `b.item(..)`, `b.light(..)`, optional `b.hazard(..)`.
//!
//! Coordinates are meters; +Y is up, north is -Z. `room`/`corridor` use the
//! theme's textures; `solid`/`deco`/`slab` take an explicit material for props.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::GlobalAmbientLight;
use bevy::pbr::DistanceFog;
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

/// System wrapper that builds the active level on entering Playing. It chooses
/// the level (random on a fresh run, the next one when advancing), builds its
/// theme, applies the global styling and dispatches to the level's `build` fn.
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
    mut run: ResMut<RunState>,
    mut style: ResMut<LevelStyle>,
    mut intro: ResMut<LevelIntro>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    start_level: Res<StartLevelConfig>,
    asset_server: Res<AssetServer>,
) {
    // Reset mission state for a fresh level.
    mission.has_key = false;
    mission.kills = 0;
    mission.total_enemies = 0;
    mission.objective = "Find the Silver Key".into();

    // A fresh run (launch, death-restart, post-win restart) picks its starting
    // level; advancing between levels keeps `run.level` as already incremented.
    // Priority: `QC_LEVEL=n` (debug) → config.ron `start_level` → random.
    if !run.carry_inventory {
        run.level = std::env::var("QC_LEVEL")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.min(NUM_LEVELS - 1))
            .or(start_level.0)
            .unwrap_or_else(|| pick_random_level(run.level));
    }
    let idx = run.level.min(NUM_LEVELS - 1);
    let (name, _) = crate::levels::LEVEL_META[idx];

    // Show the intro banner for a few seconds.
    intro.text = format!("Level {}: {}", idx + 1, name);
    intro.timer = 4.5;

    apply_theme_and_build(
        idx, &mut commands, &mut meshes, &mut materials, &asset_server, &mut colliders,
        &mut start, &mut plan, &mut lava, &mut gfx, &mut style, &mut clear, &mut ambient,
    );
}

/// World-space bounds of the built geometry (union of all colliders).
pub struct BuiltBounds {
    pub min: Vec3,
    pub max: Vec3,
}

/// Resolve `idx`'s theme, apply the global styling (clear/ambient/fog/hazard),
/// clear the mission vectors and dispatch the level's `build` fn. Shared by the
/// live game and the `QC_LEVELSHOT` preview. Returns the geometry bounds.
#[allow(clippy::too_many_arguments)]
pub fn apply_theme_and_build(
    idx: usize,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
    colliders: &mut WorldColliders,
    start: &mut PlayerStart,
    plan: &mut SpawnPlan,
    lava: &mut LavaVolumes,
    gfx: &mut GfxAssets,
    style: &mut LevelStyle,
    clear: &mut ClearColor,
    ambient: &mut GlobalAmbientLight,
) -> BuiltBounds {
    let (_, theme_id) = crate::levels::LEVEL_META[idx.min(NUM_LEVELS - 1)];

    // Shared render assets + this level's theme.
    init_gfx(meshes, materials, gfx);
    let theme = build_theme(materials, assets, theme_id);
    gfx.lava_mat = theme.hazard.clone();

    // Apply global styling (fog is applied per-frame to the player camera).
    style.fog_color = theme.fog_color;
    style.fog_start = theme.fog_start;
    style.fog_end = theme.fog_end;
    style.hazard_emissive = theme.hazard_emissive;
    style.hazard_dot = theme.hazard_dot;
    style.hazard_flash = theme.hazard_flash;
    clear.0 = theme.clear_color;
    ambient.color = theme.ambient_color;
    ambient.brightness = theme.ambient_brightness;

    colliders.solids.clear();
    plan.monsters.clear();
    plan.items.clear();
    plan.ambush.clear();
    plan.exit = None;
    lava.volumes.clear();
    lava.kill_y = f32::NEG_INFINITY;

    let mut b = Build {
        commands,
        meshes,
        materials,
        assets,
        colliders: &mut colliders.solids,
        theme: &theme,
        start,
        plan,
        lava,
    };
    crate::levels::build_index(idx, &mut b);

    let mut min = Vec3::splat(1.0e9);
    let mut max = Vec3::splat(-1.0e9);
    for a in colliders.solids.iter() {
        min = min.min(a.min);
        max = max.max(a.max);
    }
    BuiltBounds { min, max }
}

/// Pick a random level index. Seeded from the wall clock so it varies per run.
fn pick_random_level(_prev: usize) -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    // splitmix64-ish hash so low-entropy nanoseconds still spread across levels.
    let mut x = (nanos as u64) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x as usize) % NUM_LEVELS
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
    /// "Fell out of the world" plane: a player whose center drops below this Y
    /// dies instantly (no footprint to miss, no matter how far a fast faller has
    /// drifted). `NEG_INFINITY` on levels with no bottomless void. See
    /// `Build::void_kill`.
    pub kill_y: f32,
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
    /// Reinforcements that teleport in when the Silver Key is grabbed.
    pub ambush: Vec<MonsterSpawn>,
    pub exit: Option<Vec3>,
}

// ----------------------------------------------------------------------------
// Themes
// ----------------------------------------------------------------------------
/// One of the campaign's visual identities. Selects textures, fog, ambient and
/// hazard styling.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeId {
    Doomed,
    Frost,
    Brass,
    Tomb,
    Hive,
    Pirate,
    Void,
}

/// The resolved material handles + styling for a level.
pub struct Theme {
    pub floor: Handle<StandardMaterial>,
    pub wall: Handle<StandardMaterial>,
    pub trim: Handle<StandardMaterial>,
    pub ceiling: Handle<StandardMaterial>,
    pub metal: Handle<StandardMaterial>,
    pub hazard: Handle<StandardMaterial>,
    pub door: Handle<StandardMaterial>,
    /// Emissive "slipgate"/portal accent material (no texture, self-lit).
    pub accent: Handle<StandardMaterial>,
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    pub clear_color: Color,
    pub hazard_emissive: LinearRgba,
    pub hazard_dot: f32,
    pub hazard_flash: Color,
}

/// Loader setting that makes a texture wrap (tile) instead of clamping — brush
/// faces carry world-scaled UVs that exceed 0..1.
fn repeat_sampler(s: &mut ImageLoaderSettings) {
    s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
}

/// A surface material skinned with a seamless, tiling albedo texture (optionally
/// tinted/emissive).
fn tex_mat(
    m: &mut Assets<StandardMaterial>,
    assets: &AssetServer,
    path: &str,
    rough: f32,
    metal: f32,
    tint: Color,
    emissive: LinearRgba,
) -> Handle<StandardMaterial> {
    let img = assets.load_builder().with_settings(repeat_sampler).load(path.to_string());
    m.add(StandardMaterial {
        base_color: tint,
        base_color_texture: Some(img),
        perceptual_roughness: rough,
        metallic: metal,
        emissive,
        ..default()
    })
}

/// Texture folder + tint per theme. Surfaces are `<dir>/{floor,wall,trim,
/// ceiling,hazard,door}.png`; `metal` is shared from the base set.
struct ThemeSpec {
    dir: &'static str,
    tint: Color,
    /// hazard surface emissive
    hazard_emissive: LinearRgba,
    hazard_rough: f32,
    accent: (Color, LinearRgba),
    fog: (Color, f32, f32),
    ambient: (Color, f32),
    clear: Color,
    hazard_dot: f32,
    hazard_flash: Color,
}

fn theme_spec(id: ThemeId) -> ThemeSpec {
    use ThemeId::*;
    match id {
        Doomed => ThemeSpec {
            dir: "textures/world",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(5.0, 1.2, 0.1),
            hazard_rough: 0.6,
            accent: (rgb(0.5, 0.2, 0.9), LinearRgba::rgb(1.2, 0.4, 3.0)),
            fog: (rgb(0.12, 0.11, 0.15), 16.0, 62.0),
            ambient: (rgb(0.45, 0.45, 0.6), 200.0),
            clear: rgb(0.02, 0.02, 0.03),
            hazard_dot: 12.0,
            hazard_flash: rgb(0.9, 0.35, 0.05),
        },
        Frost => ThemeSpec {
            dir: "textures/world/frost",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(0.15, 0.45, 0.8),
            hazard_rough: 0.2,
            accent: (rgb(0.4, 0.8, 1.0), LinearRgba::rgb(0.6, 1.6, 3.0)),
            fog: (rgb(0.62, 0.74, 0.86), 22.0, 78.0),
            ambient: (rgb(0.6, 0.72, 0.9), 360.0),
            clear: rgb(0.5, 0.62, 0.74),
            hazard_dot: 10.0,
            hazard_flash: rgb(0.5, 0.7, 1.0),
        },
        Brass => ThemeSpec {
            dir: "textures/world/brass",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(4.5, 1.6, 0.2),
            hazard_rough: 0.5,
            accent: (rgb(1.0, 0.7, 0.3), LinearRgba::rgb(3.0, 1.6, 0.4)),
            // Long warm haze: the foundry is the open belly of a flying machine,
            // so the eye needs to carry far enough to watch the hull recede when
            // you plunge down the central shaft.
            fog: (rgb(0.14, 0.10, 0.07), 18.0, 150.0),
            ambient: (rgb(0.55, 0.45, 0.35), 230.0),
            clear: rgb(0.07, 0.05, 0.03),
            hazard_dot: 12.0,
            hazard_flash: rgb(1.0, 0.5, 0.15),
        },
        Tomb => ThemeSpec {
            dir: "textures/world/tomb",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(0.6, 0.45, 0.15),
            hazard_rough: 0.9,
            accent: (rgb(1.0, 0.85, 0.4), LinearRgba::rgb(2.6, 2.0, 0.6)),
            fog: (rgb(0.5, 0.4, 0.26), 20.0, 72.0),
            ambient: (rgb(0.7, 0.6, 0.42), 300.0),
            clear: rgb(0.32, 0.26, 0.18),
            hazard_dot: 9.0,
            hazard_flash: rgb(0.85, 0.7, 0.3),
        },
        Hive => ThemeSpec {
            dir: "textures/world/hive",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(0.3, 4.0, 0.5),
            hazard_rough: 0.4,
            accent: (rgb(0.4, 1.0, 0.4), LinearRgba::rgb(0.6, 3.0, 0.8)),
            fog: (rgb(0.08, 0.16, 0.09), 14.0, 56.0),
            ambient: (rgb(0.4, 0.55, 0.4), 220.0),
            clear: rgb(0.02, 0.05, 0.03),
            hazard_dot: 14.0,
            hazard_flash: rgb(0.4, 0.95, 0.3),
        },
        Pirate => ThemeSpec {
            dir: "textures/world/pirate",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(0.6, 1.6, 3.0),
            hazard_rough: 0.3,
            accent: (rgb(0.4, 0.7, 1.0), LinearRgba::rgb(0.8, 1.8, 3.5)),
            fog: (rgb(0.45, 0.62, 0.85), 30.0, 110.0),
            ambient: (rgb(0.6, 0.72, 0.92), 340.0),
            clear: rgb(0.38, 0.56, 0.82),
            hazard_dot: 22.0,
            hazard_flash: rgb(0.5, 0.7, 1.0),
        },
        Void => ThemeSpec {
            dir: "textures/world/void",
            tint: Color::WHITE,
            hazard_emissive: LinearRgba::rgb(1.6, 0.3, 3.2),
            hazard_rough: 0.3,
            accent: (rgb(0.7, 0.4, 1.0), LinearRgba::rgb(2.2, 0.8, 4.0)),
            fog: (rgb(0.05, 0.03, 0.1), 20.0, 80.0),
            ambient: (rgb(0.4, 0.36, 0.6), 200.0),
            clear: rgb(0.01, 0.0, 0.03),
            hazard_dot: 26.0,
            hazard_flash: rgb(0.7, 0.3, 1.0),
        },
    }
}

pub fn build_theme(m: &mut Assets<StandardMaterial>, assets: &AssetServer, id: ThemeId) -> Theme {
    let s = theme_spec(id);
    let p = |name: &str| format!("{}/{}.png", s.dir, name);
    // The original Doomed set names its hazard `lava.png`; themes use `hazard.png`.
    let hazard_path = if matches!(id, ThemeId::Doomed) { p("lava") } else { p("hazard") };
    // Metal is shared across themes (it's used sparingly for ledges/platforms).
    let metal_path = "textures/world/metal.png".to_string();
    Theme {
        floor: tex_mat(m, assets, &p("floor"), 0.95, 0.0, s.tint, LinearRgba::BLACK),
        wall: tex_mat(m, assets, &p("wall"), 0.9, 0.05, s.tint, LinearRgba::BLACK),
        trim: tex_mat(m, assets, &p("trim"), 0.6, 0.3, s.tint, LinearRgba::BLACK),
        ceiling: tex_mat(m, assets, &p("ceiling"), 1.0, 0.0, s.tint, LinearRgba::BLACK),
        metal: tex_mat(m, assets, &metal_path, 0.4, 0.75, s.tint, LinearRgba::BLACK),
        hazard: tex_mat(m, assets, &hazard_path, s.hazard_rough, 0.0, Color::WHITE, s.hazard_emissive),
        door: tex_mat(m, assets, &p("door"), 0.6, 0.4, s.tint, LinearRgba::BLACK),
        accent: m.add(StandardMaterial { base_color: s.accent.0, emissive: s.accent.1, ..default() }),
        fog_color: s.fog.0,
        fog_start: s.fog.1,
        fog_end: s.fog.2,
        ambient_color: s.ambient.0,
        ambient_brightness: s.ambient.1,
        clear_color: s.clear,
        hazard_emissive: s.hazard_emissive,
        hazard_dot: s.hazard_dot,
        hazard_flash: s.hazard_flash,
    }
}

/// Build the small set of shared render assets (unit cube, spheres, particle /
/// projectile materials). Idempotent enough to call on every (re)build.
fn init_gfx(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, gfx: &mut GfxAssets) {
    gfx.unit_cube = meshes.add(Cuboid::from_size(Vec3::ONE));
    gfx.sphere = meshes.add(Sphere::new(0.5));
    gfx.small_sphere = meshes.add(Sphere::new(0.12));
    gfx.cylinder = meshes.add(Cylinder { radius: 0.5, half_height: 0.5 });
    gfx.cone = meshes.add(Cone { radius: 0.5, height: 1.0 });
    let unlit = |m: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| {
        m.add(StandardMaterial { base_color: c, emissive: e, unlit: false, ..default() })
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
}

// ----------------------------------------------------------------------------
// Brush builder
// ----------------------------------------------------------------------------
const WALL_T: f32 = 0.5;

/// World-space size (meters) that one texture tile covers, so texel density is
/// uniform across the whole level and adjacent brushes line up.
const TEXEL: f32 = 2.5;

/// Build a box brush as its own mesh with world-aligned UVs (so the texture
/// tiles consistently and seams between brushes align).
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

/// The level-authoring context: holds the ECS handles, the active theme, and
/// the mission resources being filled. All geometry helpers hang off this.
pub struct Build<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub assets: &'a AssetServer,
    pub colliders: &'a mut Vec<Aabb>,
    pub theme: &'a Theme,
    pub start: &'a mut PlayerStart,
    pub plan: &'a mut SpawnPlan,
    pub lava: &'a mut LavaVolumes,
}

/// Wall descriptor for `room`: which wall and the gap interval (in world units
/// along that wall) to leave open as a doorway.
pub enum Wall {
    N((f32, f32)),
    S((f32, f32)),
    E((f32, f32)),
    W((f32, f32)),
}

impl<'a, 'w, 's> Build<'a, 'w, 's> {
    // -- raw brushes --------------------------------------------------------
    /// A textured box mesh with no collision.
    pub fn visual(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
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
    pub fn solid(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
        self.visual(min, max, mat);
        self.colliders.push(Aabb::from_corners(min, max));
    }

    /// Decorative brush: visual only (no collision), e.g. a hazard surface.
    pub fn deco(&mut self, min: Vec3, max: Vec3, mat: Handle<StandardMaterial>) {
        self.visual(min, max, mat);
    }

    /// A thin decorative slab (visual only) spanning x/z at height `y`, useful
    /// for rugs, water/ice sheets, banners on a wall, etc.
    pub fn slab(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, t: f32, mat: Handle<StandardMaterial>) {
        self.deco(Vec3::new(x0, y, z0), Vec3::new(x1, y + t, z1), mat);
    }

    // -- themed quick materials --------------------------------------------
    /// A quick solid-color material (for theme props: crystals, sails, gears…).
    pub fn mat(&mut self, color: Color, emissive: LinearRgba, rough: f32, metal: f32) -> Handle<StandardMaterial> {
        self.materials.add(StandardMaterial {
            base_color: color,
            emissive,
            perceptual_roughness: rough,
            metallic: metal,
            ..default()
        })
    }

    /// A quick textured (tiling) material for a custom prop surface.
    pub fn tex(&mut self, path: &str, rough: f32, metal: f32) -> Handle<StandardMaterial> {
        tex_mat(self.materials, self.assets, path, rough, metal, Color::WHITE, LinearRgba::BLACK)
    }

    /// A textured (tiling) material that also self-emits — hot metal, glowing
    /// rock, an ember-lit hull — so its surface stays readable in low light.
    pub fn tex_glow(&mut self, path: &str, rough: f32, metal: f32, emissive: LinearRgba) -> Handle<StandardMaterial> {
        tex_mat(self.materials, self.assets, path, rough, metal, Color::WHITE, emissive)
    }

    // -- walls / floors -----------------------------------------------------
    /// Wall running along X at depth `z`, height [y0,y1], with door-gaps carved.
    pub fn wall_x(&mut self, x0: f32, x1: f32, z: f32, y0: f32, y1: f32, mat: Handle<StandardMaterial>, gaps: &[(f32, f32)]) {
        for (a, b) in subtract(x0, x1, gaps) {
            self.solid(Vec3::new(a, y0, z - WALL_T * 0.5), Vec3::new(b, y1, z + WALL_T * 0.5), mat.clone());
        }
    }

    /// Wall running along Z at position `x`, height [y0,y1], with door-gaps.
    pub fn wall_z(&mut self, z0: f32, z1: f32, x: f32, y0: f32, y1: f32, mat: Handle<StandardMaterial>, gaps: &[(f32, f32)]) {
        for (a, b) in subtract(z0, z1, gaps) {
            self.solid(Vec3::new(x - WALL_T * 0.5, y0, a), Vec3::new(x + WALL_T * 0.5, y1, b), mat.clone());
        }
    }

    pub fn floor(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, mat: Handle<StandardMaterial>) {
        self.solid(Vec3::new(x0, y - 0.5, z0), Vec3::new(x1, y, z1), mat);
    }
    pub fn ceiling(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, mat: Handle<StandardMaterial>) {
        self.solid(Vec3::new(x0, y, z0), Vec3::new(x1, y + 0.5, z1), mat);
    }

    // -- composite rooms ----------------------------------------------------
    /// A closed room (floor + ceiling + four walls) of height `h`, with optional
    /// doorway gaps. Uses the theme's floor/wall/ceiling textures.
    pub fn room(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32, openings: &[Wall]) {
        let y1 = y + h;
        let (floor, wall, ceil) = (self.theme.floor.clone(), self.theme.wall.clone(), self.theme.ceiling.clone());
        self.floor(x0, x1, z0, z1, y, floor);
        self.ceiling(x0, x1, z0, z1, y1, ceil);
        let (mut north, mut south, mut east, mut west) = (vec![], vec![], vec![], vec![]);
        for o in openings {
            match o {
                Wall::N(g) => north.push(*g),
                Wall::S(g) => south.push(*g),
                Wall::E(g) => east.push(*g),
                Wall::W(g) => west.push(*g),
            }
        }
        self.wall_x(x0, x1, z0, y, y1, wall.clone(), &north);
        self.wall_x(x0, x1, z1, y, y1, wall.clone(), &south);
        self.wall_z(z0, z1, x1, y, y1, wall.clone(), &east);
        self.wall_z(z0, z1, x0, y, y1, wall, &west);
    }

    /// Open-floor room (floor + ceiling + walls) with NO ceiling — for open-sky
    /// levels (pirate ship deck, etc.). Walls double as railings if `h` small.
    pub fn roofless(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32, openings: &[Wall]) {
        let y1 = y + h;
        let (floor, wall) = (self.theme.floor.clone(), self.theme.wall.clone());
        self.floor(x0, x1, z0, z1, y, floor);
        let (mut north, mut south, mut east, mut west) = (vec![], vec![], vec![], vec![]);
        for o in openings {
            match o {
                Wall::N(g) => north.push(*g),
                Wall::S(g) => south.push(*g),
                Wall::E(g) => east.push(*g),
                Wall::W(g) => west.push(*g),
            }
        }
        self.wall_x(x0, x1, z0, y, y1, wall.clone(), &north);
        self.wall_x(x0, x1, z1, y, y1, wall.clone(), &south);
        self.wall_z(z0, z1, x1, y, y1, wall.clone(), &east);
        self.wall_z(z0, z1, x0, y, y1, wall, &west);
    }

    /// Corridor running along Z (walls on the x sides, floor+ceiling, open ends).
    pub fn corridor_z(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32) {
        let y1 = y + h;
        let (floor, wall, ceil) = (self.theme.floor.clone(), self.theme.wall.clone(), self.theme.ceiling.clone());
        self.floor(x0, x1, z0, z1, y, floor);
        self.ceiling(x0, x1, z0, z1, y1, ceil);
        self.wall_z(z0, z1, x0, y, y1, wall.clone(), &[]);
        self.wall_z(z0, z1, x1, y, y1, wall, &[]);
    }

    /// Corridor running along X (walls on the z sides).
    pub fn corridor_x(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, h: f32) {
        let y1 = y + h;
        let (floor, wall, ceil) = (self.theme.floor.clone(), self.theme.wall.clone(), self.theme.ceiling.clone());
        self.floor(x0, x1, z0, z1, y, floor);
        self.ceiling(x0, x1, z0, z1, y1, ceil);
        self.wall_x(x0, x1, z0, y, y1, wall.clone(), &[]);
        self.wall_x(x0, x1, z1, y, y1, wall, &[]);
    }

    /// A staircase of `steps` rising from `base_y` to `top_y`, treads spanning
    /// [a0,a1] on the cross axis and marching from `base_along` along `dir`
    /// (unit +/-X or +/-Z). Uses the theme trim texture.
    pub fn stairs(&mut self, a0: f32, a1: f32, base_along: f32, top_y: f32, base_y: f32, steps: u32, dir: Vec3) {
        let rise = (top_y - base_y) / steps as f32;
        let depth = 0.55;
        let trim = self.theme.trim.clone();
        for i in 0..steps {
            let h = base_y + rise * (i as f32 + 1.0);
            let off = i as f32 * depth;
            if dir.z.abs() > 0.5 {
                let sgn = dir.z.signum();
                let z0 = base_along + sgn * off;
                let z1 = z0 + sgn * depth;
                self.solid(Vec3::new(a0, base_y - 0.5, z0.min(z1)), Vec3::new(a1, h, z0.max(z1)), trim.clone());
            } else {
                let sgn = dir.x.signum();
                let x0 = base_along + sgn * off;
                let x1 = x0 + sgn * depth;
                self.solid(Vec3::new(x0.min(x1), base_y - 0.5, a0), Vec3::new(x0.max(x1), h, a1), trim.clone());
            }
        }
    }

    // -- mission props ------------------------------------------------------
    pub fn monster(&mut self, kind: MonsterKind, pos: Vec3) {
        self.plan.monsters.push(MonsterSpawn { kind, pos });
    }
    pub fn item(&mut self, kind: ItemKind, pos: Vec3) {
        self.plan.items.push(ItemSpawn { kind, pos });
    }
    /// A reinforcement that teleports in when the Silver Key is grabbed.
    pub fn ambush(&mut self, kind: MonsterKind, pos: Vec3) {
        self.plan.ambush.push(MonsterSpawn { kind, pos });
    }

    /// A locked door brush filling [min,max] that slides by `open_offset` once
    /// the player has the key and is near. Use a thin slab matching a wall gap.
    pub fn door(&mut self, min: Vec3, max: Vec3, open_offset: Vec3) {
        let aabb = Aabb::from_corners(min, max);
        let center = aabb.center();
        let solid_index = self.colliders.len();
        self.colliders.push(aabb);
        let mesh = self.meshes.add(box_mesh(min, max));
        let door_mat = self.theme.door.clone();
        self.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(door_mat),
            Transform::from_translation(center),
            Door { solid_index, closed_pos: center, open_offset, opening: false, opened: false, t: 0.0 },
            LevelEntity,
        ));
    }

    /// Register the level exit (reaching within ~3m wins/advances).
    pub fn exit(&mut self, pos: Vec3) {
        self.plan.exit = Some(pos);
    }

    /// An emissive "slipgate"/portal slab using the theme accent material.
    pub fn slipgate(&mut self, min: Vec3, max: Vec3) {
        let accent = self.theme.accent.clone();
        self.deco(min, max, accent);
    }

    /// A hazard pool: a glowing surface at `top`, a damage volume around it, and
    /// a solid "catch floor" a little below — so anything that walks OR FALLS in
    /// lands in it and burns (vital for bottomless pits over open void, where a
    /// fast faller would otherwise tunnel through a thin volume and drop forever).
    /// Standing in it hurts (lava, toxic sludge, void, sea…).
    pub fn hazard(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, top: f32) {
        let haz = self.theme.hazard.clone();
        // Glowing surface.
        self.deco(Vec3::new(x0, top - 0.2, z0), Vec3::new(x1, top, z1), haz.clone());
        // Solid catch floor ~1m down (hidden under real floors where one exists).
        self.solid(Vec3::new(x0, top - 1.4, z0), Vec3::new(x1, top - 1.0, z1), haz);
        // Damage volume from the catch floor up to a little above the surface.
        // The upper lip is only +0.2: anything wading at/just below the surface
        // burns, but a low stepping platform standing clear of the surface does
        // NOT (level-5's catwalks sit 0.3 above the sludge; level-2's ice blocks
        // 0.55+). Keep this below ~0.3 or low platforms over a hazard will singe.
        self.lava.volumes.push(Aabb::from_corners(
            Vec3::new(x0, top - 1.2, z0),
            Vec3::new(x1, top + 0.2, z1),
        ));
    }

    /// Set the "fell out of the world" kill plane: any player whose center
    /// drops below `y` dies instantly. For levels with a bottomless void where a
    /// fast faller (no air drag — horizontal speed is kept the whole drop) would
    /// otherwise sail past a finite hazard pool and fall forever. Place it at the
    /// molten/void floor so you die just as you plunge into it.
    pub fn void_kill(&mut self, y: f32) {
        self.lava.kill_y = y;
    }

    // -- lights -------------------------------------------------------------
    /// A warm/colored point light (LevelEntity, no shadows by default).
    pub fn light(&mut self, pos: Vec3, color: Color, intensity: f32, range: f32) {
        self.commands.spawn((
            PointLight { color, intensity, range, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(pos),
            LevelEntity,
        ));
    }

    /// The level's key/sun directional light.
    pub fn sun(&mut self, from: Vec3, at: Vec3, color: Color, illuminance: f32) {
        self.commands.spawn((
            DirectionalLight { color, illuminance, shadow_maps_enabled: true, ..default() },
            Transform::from_translation(from).looking_at(at, Vec3::Y),
            LevelEntity,
        ));
    }
}

/// Subtract a set of intervals (gaps) from [lo,hi], returning the remaining
/// segments.
fn subtract(lo: f32, hi: f32, gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut segs = vec![(lo, hi)];
    for &(ga, gb) in gaps {
        let mut next = Vec::new();
        for (a, b) in segs {
            if gb <= a || ga >= b {
                next.push((a, b));
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

/// Apply the active level's fog to a freshly-spawned player camera.
pub fn apply_fog(style: &LevelStyle) -> DistanceFog {
    DistanceFog {
        color: style.fog_color,
        falloff: bevy::pbr::FogFalloff::Linear { start: style.fog_start, end: style.fog_end },
        ..default()
    }
}
