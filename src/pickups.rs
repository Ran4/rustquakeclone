//! Pickups: health, armor, ammo, weapons and the silver key.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use crate::common::*;
use crate::level::{ItemKind, SpawnPlan};
use crate::player::Player;
use crate::weapons::{Inventory, WeaponKind, WeaponTex};

pub struct PickupsPlugin;
impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (animate_items, pickup_items).run_if(in_state(GameState::Playing)));
    }
}

#[derive(Component)]
pub struct Item {
    pub kind: ItemKind,
    pub base_y: f32,
    pub bob: f32,
}


// ----------------------------------------------------------------------------
// Pickup models
//
// Weapons and ammo are little hand-built low-poly props (a parented set of
// boxes, barrels, cones and balls) rather than a single coloured cube. The root
// entity carries the `Item` + the bob/spin animation; the model lives in its
// children. Bodies get a modest emissive and the working bits (barrels, shells,
// glowing cores) a strong one so every pickup stays bright and readable in the
// dark, with bloom on the hottest accents.
// ----------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum Prim {
    Cube,
    Cyl,
    Cone,
    Ball,
}

/// One mesh of a pickup (or weapon view-model) model, in the model's local
/// space. The same little-primitive system builds both the ground-pickup props
/// and the first-person view-models (see `weapons::viewmodel_parts`).
#[derive(Clone, Copy)]
pub(crate) struct Part {
    pub(crate) prim: Prim,
    pub(crate) size: Vec3, // full dimensions applied to the unit mesh
    pub(crate) pos: Vec3,
    pub(crate) rot: Quat,
    pub(crate) color: Color,
    pub(crate) emissive: LinearRgba,
    pub(crate) metallic: f32,
    /// Weapon-material albedo skin, or `None` for a flat-coloured/glowing part.
    /// Weapon models set this; ammo/health/armor/key stay untextured.
    pub(crate) tex: Option<WeaponTex>,
}

/// A box (full size).
pub(crate) fn cube(size: Vec3, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cube, size, pos, rot: Quat::IDENTITY, color, emissive, metallic: 0.6, tex: None }
}
/// A cylinder of diameter `d` and length `len` lying along local Z.
pub(crate) fn tube_z(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cyl, size: Vec3::new(d, len, d), pos, rot: Quat::from_rotation_x(FRAC_PI_2), color, emissive, metallic: 0.6, tex: None }
}
/// A cylinder of diameter `d` and length `len` standing along local +Y.
pub(crate) fn tube_y(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cyl, size: Vec3::new(d, len, d), pos, rot: Quat::IDENTITY, color, emissive, metallic: 0.6, tex: None }
}
/// A cylinder of diameter `d` and length `len` lying along local X — cross pins,
/// trigger-guard bars, hinge rods.
pub(crate) fn tube_x(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cyl, size: Vec3::new(d, len, d), pos, rot: Quat::from_rotation_z(FRAC_PI_2), color, emissive, metallic: 0.6, tex: None }
}
/// A cone (apex toward +Z) of base diameter `d` and length `len` — a warhead nose.
pub(crate) fn spike_z(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cone, size: Vec3::new(d, len, d), pos, rot: Quat::from_rotation_x(FRAC_PI_2), color, emissive, metallic: 0.3, tex: None }
}
/// A cone with its apex toward -Z (forward, in view-model space) — a muzzle
/// spike / forward-pointing warhead on the first-person view-models.
pub(crate) fn spike_fwd(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cone, size: Vec3::new(d, len, d), pos, rot: Quat::from_rotation_x(-FRAC_PI_2), color, emissive, metallic: 0.3, tex: None }
}
/// A cone pointing up (+Y) — a standing nail / rocket nose.
pub(crate) fn spike_y(d: f32, len: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cone, size: Vec3::new(d, len, d), pos, rot: Quat::IDENTITY, color, emissive, metallic: 0.3, tex: None }
}
/// A sphere of diameter `d`.
pub(crate) fn ball(d: f32, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Ball, size: Vec3::splat(d), pos, rot: Quat::IDENTITY, color, emissive, metallic: 0.3, tex: None }
}
/// A cone with explicit (non-uniform) dimensions, apex pointing down (-Y) — the
/// pointed bottom of an armor shield.
pub(crate) fn cone_down(size: Vec3, pos: Vec3, color: Color, emissive: LinearRgba) -> Part {
    Part { prim: Prim::Cone, size, pos, rot: Quat::from_rotation_x(PI), color, emissive, metallic: 0.5, tex: None }
}
/// Skin a part with a weapon-material albedo texture (the part's `color` then
/// acts as a tint that multiplies the texture — pass `Color::WHITE` for the
/// material's true albedo, or a colour to tint the neutral `Painted` sheet).
pub(crate) fn skin(part: Part, tex: WeaponTex) -> Part {
    Part { tex: Some(tex), ..part }
}

/// The shared unit mesh for a primitive (cube/cylinder/cone/sphere).
pub(crate) fn prim_mesh(prim: Prim, gfx: &GfxAssets) -> Handle<Mesh> {
    match prim {
        Prim::Cube => gfx.unit_cube.clone(),
        Prim::Cyl => gfx.cylinder.clone(),
        Prim::Cone => gfx.cone.clone(),
        Prim::Ball => gfx.sphere.clone(),
    }
}

/// Build the `StandardMaterial` for one part (albedo skin or flat colour + glow).
pub(crate) fn part_material(part: &Part, assets: &AssetServer, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: part.color,
        base_color_texture: part.tex.map(|t| assets.load(t.file())),
        emissive: part.emissive,
        perceptual_roughness: 0.4,
        metallic: part.metallic,
        ..default()
    })
}

/// The mesh parts that make up a pickup — every kind is a little hand-built
/// low-poly prop (a parented set of boxes, barrels, cones and balls).
fn item_model(kind: ItemKind) -> Vec<Part> {
    use ItemKind::*;
    match kind {
        WeaponSuperShotgun => super_shotgun(),
        WeaponNailgun => nailgun(),
        WeaponGrenade => grenade_launcher(),
        WeaponRocket => rocket_launcher(),
        WeaponLightning => lightning_gun(),
        Shells(_) => shells_box(),
        Nails(_) => nails_box(),
        Rockets(_) => rockets_box(),
        Cells(_) => cells_box(),
        Health(_) => health_kit(),
        MegaHealth => mega_health(),
        ArmorGreen => armor_shield(rgb(0.2, 0.78, 0.32), LinearRgba::rgb(0.12, 0.7, 0.18), rgb(0.6, 1.0, 0.6), LinearRgba::rgb(0.5, 2.2, 0.6)),
        ArmorYellow => armor_shield(rgb(0.92, 0.8, 0.2), LinearRgba::rgb(0.9, 0.7, 0.1), rgb(1.0, 0.95, 0.5), LinearRgba::rgb(2.5, 2.0, 0.4)),
        SilverKey => silver_key(),
    }
}

// --- weapons ---------------------------------------------------------------

// Weapon parts are skinned with weapon-material albedos: natural materials
// (gunmetal/brass/steel/wood) use a WHITE base so the texture shows true, while
// the green/red/blue bodies tint the neutral `Painted` sheet. The glow accents
// (muzzle rings, warhead, lightning core, loaded grenade) stay untextured and
// emissive so they keep popping bright in the dark.
fn super_shotgun() -> Vec<Part> {
    let w = Color::WHITE;
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    let brass_e = LinearRgba::rgb(0.25, 0.16, 0.03);
    let wood_e = LinearRgba::rgb(0.06, 0.025, 0.008);
    vec![
        skin(cube(Vec3::new(0.26, 0.18, 0.34), Vec3::new(0.0, 0.0, -0.06), w, metal_e), WeaponTex::Gunmetal),
        skin(tube_z(0.11, 0.52, Vec3::new(-0.07, 0.03, 0.22), w, brass_e), WeaponTex::Brass),
        skin(tube_z(0.11, 0.52, Vec3::new(0.07, 0.03, 0.22), w, brass_e), WeaponTex::Brass),
        // bright muzzle ring (untextured glow)
        tube_z(0.15, 0.05, Vec3::new(0.0, 0.03, 0.48), rgb(0.95, 0.72, 0.22), LinearRgba::rgb(2.4, 1.4, 0.18)),
        skin(cube(Vec3::new(0.12, 0.16, 0.22), Vec3::new(0.0, -0.06, -0.3), w, wood_e), WeaponTex::Wood),
        skin(cube(Vec3::new(0.09, 0.2, 0.1), Vec3::new(0.0, -0.13, -0.14), w, wood_e), WeaponTex::Wood),
    ]
}

fn nailgun() -> Vec<Part> {
    let w = Color::WHITE;
    let metal_e = LinearRgba::rgb(0.08, 0.09, 0.12);
    let steel_e = LinearRgba::rgb(0.4, 0.45, 0.7);
    vec![
        skin(cube(Vec3::new(0.2, 0.2, 0.3), Vec3::new(0.0, 0.0, -0.05), w, metal_e), WeaponTex::Gunmetal),
        skin(tube_z(0.07, 0.56, Vec3::new(-0.06, 0.04, 0.24), w, steel_e), WeaponTex::Steel),
        skin(tube_z(0.07, 0.56, Vec3::new(0.06, 0.04, 0.24), w, steel_e), WeaponTex::Steel),
        // drum magazine under the receiver
        skin(tube_z(0.28, 0.13, Vec3::new(0.0, -0.1, 0.0), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.09, 0.2, 0.1), Vec3::new(0.0, -0.13, -0.13), w, metal_e), WeaponTex::Gunmetal),
    ]
}

fn grenade_launcher() -> Vec<Part> {
    let w = Color::WHITE;
    let body = rgb(0.32, 0.52, 0.22);
    let body_e = LinearRgba::rgb(0.1, 0.35, 0.05);
    let dark_e = LinearRgba::rgb(0.04, 0.04, 0.05);
    let glow = rgb(0.35, 0.95, 0.28);
    let glow_e = LinearRgba::rgb(0.4, 2.2, 0.18);
    vec![
        skin(cube(Vec3::new(0.22, 0.2, 0.3), Vec3::new(0.0, 0.0, -0.05), body, body_e), WeaponTex::Painted),
        // fat short barrel
        skin(tube_z(0.22, 0.4, Vec3::new(0.0, 0.03, 0.24), w, dark_e), WeaponTex::Gunmetal),
        // bright muzzle ring + a loaded grenade at the mouth (untextured glow)
        tube_z(0.27, 0.06, Vec3::new(0.0, 0.03, 0.44), glow, glow_e),
        ball(0.17, Vec3::new(0.0, 0.03, 0.46), glow, glow_e),
        // drum
        skin(tube_z(0.24, 0.12, Vec3::new(0.0, -0.1, 0.02), body, body_e), WeaponTex::Painted),
        skin(cube(Vec3::new(0.09, 0.2, 0.1), Vec3::new(0.0, -0.13, -0.13), w, dark_e), WeaponTex::Gunmetal),
    ]
}

fn rocket_launcher() -> Vec<Part> {
    let w = Color::WHITE;
    let tube_e = LinearRgba::rgb(0.06, 0.06, 0.08);
    let band = rgb(0.55, 0.16, 0.12);
    let band_e = LinearRgba::rgb(0.4, 0.06, 0.02);
    let head = rgb(1.0, 0.55, 0.15);
    let head_e = LinearRgba::rgb(3.2, 1.1, 0.18);
    vec![
        // launch tube
        skin(tube_z(0.27, 0.66, Vec3::new(0.0, 0.04, -0.04), w, tube_e), WeaponTex::Gunmetal),
        // red casing band
        skin(tube_z(0.31, 0.16, Vec3::new(0.0, 0.04, -0.12), band, band_e), WeaponTex::Painted),
        // rocket poking out the front: body + nose cone (untextured glow)
        tube_z(0.13, 0.2, Vec3::new(0.0, 0.04, 0.36), head, head_e),
        spike_z(0.13, 0.16, Vec3::new(0.0, 0.04, 0.54), head, head_e),
        // top sight + pistol grip
        skin(cube(Vec3::new(0.05, 0.09, 0.2), Vec3::new(0.0, 0.21, -0.04), w, tube_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.1, 0.2, 0.12), Vec3::new(0.0, -0.16, -0.14), band, band_e), WeaponTex::Painted),
    ]
}

fn lightning_gun() -> Vec<Part> {
    let w = Color::WHITE;
    let body = rgb(0.22, 0.34, 0.6);
    let body_e = LinearRgba::rgb(0.1, 0.25, 0.6);
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    let core = rgb(0.65, 0.92, 1.0);
    let core_e = LinearRgba::rgb(0.7, 2.8, 6.5);
    let prong = rgb(0.82, 0.88, 0.97);
    let prong_e = LinearRgba::rgb(0.6, 1.8, 4.0);
    vec![
        skin(cube(Vec3::new(0.22, 0.2, 0.32), Vec3::new(0.0, 0.0, -0.05), body, body_e), WeaponTex::Painted),
        skin(tube_z(0.12, 0.36, Vec3::new(0.0, 0.02, 0.18), body, body_e), WeaponTex::Painted),
        // glowing coil rings + emitter core + prongs (untextured glow)
        tube_z(0.19, 0.04, Vec3::new(0.0, 0.02, 0.12), core, core_e),
        tube_z(0.19, 0.04, Vec3::new(0.0, 0.02, 0.26), core, core_e),
        ball(0.18, Vec3::new(0.0, 0.02, 0.42), core, core_e),
        tube_z(0.04, 0.3, Vec3::new(-0.1, 0.02, 0.44), prong, prong_e),
        tube_z(0.04, 0.3, Vec3::new(0.1, 0.02, 0.44), prong, prong_e),
        skin(cube(Vec3::new(0.09, 0.2, 0.1), Vec3::new(0.0, -0.13, -0.12), w, metal_e), WeaponTex::Gunmetal),
    ]
}

// --- ammo ------------------------------------------------------------------

fn shells_box() -> Vec<Part> {
    let crate_c = rgb(0.48, 0.38, 0.16);
    let crate_e = LinearRgba::rgb(0.2, 0.14, 0.04);
    let shell = rgb(0.9, 0.16, 0.1);
    let shell_e = LinearRgba::rgb(1.3, 0.14, 0.06);
    let brass = rgb(0.95, 0.72, 0.2);
    let brass_e = LinearRgba::rgb(0.95, 0.6, 0.08);
    let mut v = vec![cube(Vec3::new(0.5, 0.26, 0.36), Vec3::new(0.0, -0.05, 0.0), crate_c, crate_e)];
    // a row of shells standing in the crate
    for x in [-0.14, 0.0, 0.14] {
        v.push(tube_y(0.1, 0.26, Vec3::new(x, 0.16, 0.04), shell, shell_e));
        v.push(tube_y(0.115, 0.09, Vec3::new(x, 0.07, 0.04), brass, brass_e));
    }
    v
}

fn nails_box() -> Vec<Part> {
    let crate_c = rgb(0.3, 0.32, 0.38);
    let crate_e = LinearRgba::rgb(0.11, 0.12, 0.16);
    let steel = rgb(0.85, 0.9, 1.0);
    let steel_e = LinearRgba::rgb(0.95, 1.05, 1.7);
    let mut v = vec![cube(Vec3::new(0.5, 0.26, 0.36), Vec3::new(0.0, -0.05, 0.0), crate_c, crate_e)];
    // a fistful of bright nails standing point-up
    for (x, z) in [(-0.14, 0.06), (-0.05, -0.05), (0.04, 0.07), (0.13, -0.03), (0.0, 0.12)] {
        v.push(tube_y(0.05, 0.26, Vec3::new(x, 0.15, z), steel, steel_e));
        v.push(spike_y(0.05, 0.1, Vec3::new(x, 0.31, z), steel, steel_e));
    }
    v
}

fn rockets_box() -> Vec<Part> {
    let crate_c = rgb(0.4, 0.22, 0.16);
    let crate_e = LinearRgba::rgb(0.16, 0.07, 0.03);
    let body = rgb(1.0, 0.5, 0.12);
    let body_e = LinearRgba::rgb(2.4, 0.7, 0.1);
    let tip = rgb(1.0, 0.85, 0.22);
    let tip_e = LinearRgba::rgb(2.8, 1.8, 0.2);
    let mut v = vec![cube(Vec3::new(0.5, 0.26, 0.36), Vec3::new(0.0, -0.05, 0.0), crate_c, crate_e)];
    // two rockets standing nose-up
    for x in [-0.11, 0.12] {
        v.push(tube_y(0.13, 0.3, Vec3::new(x, 0.15, 0.0), body, body_e));
        v.push(spike_y(0.13, 0.15, Vec3::new(x, 0.34, 0.0), tip, tip_e));
    }
    v
}

fn cells_box() -> Vec<Part> {
    let crate_c = rgb(0.18, 0.2, 0.26);
    let crate_e = LinearRgba::rgb(0.06, 0.07, 0.11);
    let glow = rgb(0.4, 0.85, 1.0);
    let glow_e = LinearRgba::rgb(0.35, 1.5, 4.8);
    let cap = rgb(0.72, 0.74, 0.8);
    let cap_e = LinearRgba::rgb(0.2, 0.22, 0.26);
    vec![
        cube(Vec3::new(0.46, 0.3, 0.34), Vec3::new(0.0, -0.04, 0.0), crate_c, crate_e),
        // glowing energy column + terminal cap
        tube_y(0.2, 0.36, Vec3::new(0.0, 0.16, 0.0), glow, glow_e),
        tube_y(0.1, 0.07, Vec3::new(0.0, 0.36, 0.0), cap, cap_e),
        // bright window band around the casing
        cube(Vec3::new(0.48, 0.09, 0.36), Vec3::new(0.0, 0.0, 0.0), glow, glow_e),
    ]
}

// --- health / armor / key --------------------------------------------------

fn health_kit() -> Vec<Part> {
    // A medkit: pale box with a glowing green medical cross (green is this game's
    // health colour). Cross on the top and front so it reads from any spin angle.
    let box_c = rgb(0.9, 0.93, 0.92);
    let box_e = LinearRgba::rgb(0.3, 0.45, 0.35);
    let cross = rgb(0.35, 1.0, 0.4);
    let cross_e = LinearRgba::rgb(0.4, 3.0, 0.6);
    vec![
        cube(Vec3::new(0.42, 0.3, 0.36), Vec3::ZERO, box_c, box_e),
        // top-face cross
        cube(Vec3::new(0.28, 0.06, 0.1), Vec3::new(0.0, 0.16, 0.0), cross, cross_e),
        cube(Vec3::new(0.1, 0.06, 0.28), Vec3::new(0.0, 0.16, 0.0), cross, cross_e),
        // front-face cross
        cube(Vec3::new(0.28, 0.1, 0.06), Vec3::new(0.0, 0.0, 0.19), cross, cross_e),
        cube(Vec3::new(0.1, 0.26, 0.06), Vec3::new(0.0, 0.0, 0.19), cross, cross_e),
    ]
}

fn mega_health() -> Vec<Part> {
    // Mega health: a bright blue power orb on a collar base, with a small white
    // cross on its face — distinct from the green medkit and clearly "special".
    let base = rgb(0.2, 0.3, 0.55);
    let base_e = LinearRgba::rgb(0.1, 0.2, 0.7);
    let orb = rgb(0.5, 0.75, 1.0);
    let orb_e = LinearRgba::rgb(0.5, 1.6, 5.5);
    let ring = rgb(0.7, 0.85, 1.0);
    let ring_e = LinearRgba::rgb(0.4, 1.2, 4.0);
    let mark = rgb(1.0, 1.0, 1.0);
    let mark_e = LinearRgba::rgb(4.0, 4.0, 4.5);
    vec![
        cube(Vec3::new(0.4, 0.2, 0.4), Vec3::new(0.0, -0.12, 0.0), base, base_e),
        tube_y(0.36, 0.06, Vec3::new(0.0, 0.0, 0.0), ring, ring_e),
        ball(0.42, Vec3::new(0.0, 0.16, 0.0), orb, orb_e),
        // white cross on the orb's face
        cube(Vec3::new(0.2, 0.05, 0.05), Vec3::new(0.0, 0.16, 0.2), mark, mark_e),
        cube(Vec3::new(0.05, 0.2, 0.05), Vec3::new(0.0, 0.16, 0.2), mark, mark_e),
    ]
}

fn armor_shield(base: Color, base_e: LinearRgba, trim: Color, trim_e: LinearRgba) -> Vec<Part> {
    // A standing shield: a flat upper plate tapering to a point at the bottom,
    // with a bright central stripe + boss so it glows and reads as armor.
    vec![
        cube(Vec3::new(0.44, 0.34, 0.12), Vec3::new(0.0, 0.12, 0.0), base, base_e),
        cone_down(Vec3::new(0.44, 0.34, 0.12), Vec3::new(0.0, -0.17, 0.0), base, base_e),
        // bright vertical crest stripe + central boss
        cube(Vec3::new(0.08, 0.52, 0.14), Vec3::new(0.0, 0.04, 0.0), trim, trim_e),
        ball(0.16, Vec3::new(0.0, 0.13, 0.07), trim, trim_e),
    ]
}

fn silver_key() -> Vec<Part> {
    // A key: a shaft along +Z, a square bow (ring with a hole) at the back, and
    // teeth hanging off the front — all bright glowing silver.
    let silver = rgb(0.85, 0.86, 0.95);
    let silver_e = LinearRgba::rgb(1.4, 1.4, 2.0);
    let z = -0.16;
    vec![
        // shaft
        tube_z(0.07, 0.42, Vec3::new(0.0, 0.0, 0.08), silver, silver_e),
        // bow: a square ring (four bars framing a hole) at the back
        cube(Vec3::new(0.24, 0.05, 0.06), Vec3::new(0.0, 0.1, z), silver, silver_e),
        cube(Vec3::new(0.24, 0.05, 0.06), Vec3::new(0.0, -0.1, z), silver, silver_e),
        cube(Vec3::new(0.05, 0.24, 0.06), Vec3::new(-0.095, 0.0, z), silver, silver_e),
        cube(Vec3::new(0.05, 0.24, 0.06), Vec3::new(0.095, 0.0, z), silver, silver_e),
        // teeth at the business end
        cube(Vec3::new(0.05, 0.12, 0.05), Vec3::new(0.0, -0.07, 0.26), silver, silver_e),
        cube(Vec3::new(0.05, 0.08, 0.05), Vec3::new(0.0, -0.05, 0.32), silver, silver_e),
    ]
}

/// Spawn one pickup (its animated root + the parented model meshes) at `pos`.
/// Shared by the live level spawner and the `QC_ITEMSHOT` gallery.
pub fn spawn_pickup(
    commands: &mut Commands,
    gfx: &GfxAssets,
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    kind: ItemKind,
    pos: Vec3,
) -> Entity {
    let parts = item_model(kind);
    commands
        .spawn((
            Transform::from_translation(pos),
            Visibility::default(),
            Item { kind, base_y: pos.y, bob: 0.0 },
            LevelEntity,
        ))
        .with_children(|p| {
            for part in &parts {
                p.spawn((
                    Mesh3d(prim_mesh(part.prim, gfx)),
                    MeshMaterial3d(part_material(part, assets, materials)),
                    Transform { translation: part.pos, rotation: part.rot, scale: part.size },
                ));
            }
        })
        .id()
}

pub fn spawn_items(
    mut commands: Commands,
    plan: Res<SpawnPlan>,
    gfx: Res<GfxAssets>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for it in &plan.items {
        spawn_pickup(&mut commands, &gfx, &assets, &mut materials, it.kind, it.pos);
    }
}

fn animate_items(time: Res<Time>, mut q: Query<(&mut Transform, &mut Item)>) {
    let dt = time.delta_secs();
    for (mut tf, mut item) in &mut q {
        item.bob += dt * 2.0;
        tf.translation.y = item.base_y + (item.bob).sin() * 0.12 + 0.2;
        tf.rotation = Quat::from_rotation_y(item.bob);
    }
}

#[allow(clippy::too_many_arguments)]
fn pickup_items(
    mut commands: Commands,
    q_items: Query<(Entity, &Transform, &Item)>,
    mut q_player: Query<(&Transform, &mut Health, &mut Armor, &mut Inventory), With<Player>>,
    mut mission: ResMut<Mission>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    let Ok((ptf, mut health, mut armor, mut inv)) = q_player.single_mut() else { return };
    let ppos = ptf.translation;
    for (e, itf, item) in &q_items {
        if itf.translation.distance(ppos) > 1.7 {
            continue;
        }
        if apply_item(item.kind, &mut health, &mut armor, &mut inv, &mut mission, &mut sfx, &mut notify) {
            flash.write(ScreenFlash { color: rgb(0.6, 0.55, 0.2), strength: 0.18 });
            commands.entity(e).despawn();
        }
    }
}

fn apply_item(
    kind: ItemKind,
    health: &mut Health,
    armor: &mut Armor,
    inv: &mut Inventory,
    mission: &mut Mission,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
) -> bool {
    use ItemKind::*;
    let cap = 200;
    match kind {
        Health(n) => {
            if health.current >= health.max {
                return false;
            }
            health.current = (health.current + n as f32).min(health.max);
            sfx.write(Sfx::global(Sound::PickupHealth));
            notify.write(Notify::new(format!("+{n} Health")));
        }
        MegaHealth => {
            if health.current >= health.max {
                return false;
            }
            health.current = health.max;
            sfx.write(Sfx::global(Sound::PickupHealth));
            notify.write(Notify::new("Mega Health!"));
        }
        ArmorGreen => {
            if armor.points >= 100.0 {
                return false;
            }
            armor.points = armor.points.max(100.0);
            armor.absorb = armor.absorb.max(0.5);
            sfx.write(Sfx::global(Sound::PickupArmor));
            notify.write(Notify::new("Green Armor"));
        }
        ArmorYellow => {
            if armor.points >= 150.0 && armor.absorb >= 0.75 {
                return false;
            }
            armor.points = armor.points.max(150.0);
            armor.absorb = armor.absorb.max(0.75);
            sfx.write(Sfx::global(Sound::PickupArmor));
            notify.write(Notify::new("Yellow Armor"));
        }
        Shells(n) => return give_ammo(inv, 0, n, cap, sfx, notify, "Shells"),
        Nails(n) => return give_ammo(inv, 1, n, cap, sfx, notify, "Nails"),
        Rockets(n) => return give_ammo(inv, 2, n, cap, sfx, notify, "Rockets"),
        Cells(n) => return give_ammo(inv, 3, n, cap, sfx, notify, "Cells"),
        WeaponSuperShotgun => return give_weapon(inv, WeaponKind::SuperShotgun, 0, 10, sfx, notify),
        WeaponNailgun => return give_weapon(inv, WeaponKind::Nailgun, 1, 50, sfx, notify),
        WeaponGrenade => return give_weapon(inv, WeaponKind::Grenade, 2, 10, sfx, notify),
        WeaponRocket => return give_weapon(inv, WeaponKind::Rocket, 2, 10, sfx, notify),
        WeaponLightning => return give_weapon(inv, WeaponKind::Lightning, 3, 25, sfx, notify),
        SilverKey => {
            mission.has_key = true;
            sfx.write(Sfx::global(Sound::KeyPickup));
            notify.write(Notify::new("Picked up the Silver Key!"));
        }
    }
    true
}

fn give_ammo(
    inv: &mut Inventory,
    idx: usize,
    n: u32,
    cap: i32,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
    label: &str,
) -> bool {
    if inv.ammo[idx] >= cap {
        return false;
    }
    inv.ammo[idx] = (inv.ammo[idx] + n as i32).min(cap);
    sfx.write(Sfx::global(Sound::PickupAmmo));
    notify.write(Notify::new(format!("+{n} {label}")));
    true
}

fn give_weapon(
    inv: &mut Inventory,
    w: WeaponKind,
    ammo_idx: usize,
    ammo: i32,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
) -> bool {
    let i = w.index();
    let had = inv.owned[i];
    inv.owned[i] = true;
    inv.ammo[ammo_idx] = (inv.ammo[ammo_idx] + ammo).min(200);
    inv.current = w;
    sfx.write(Sfx::global(Sound::PickupWeapon));
    notify.write(Notify::new(if had { format!("{}", w.name()) } else { format!("Got the {}!", w.name()) }));
    true
}
