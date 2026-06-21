//! QC_ITEMSHOT debug mode: render each pickup model on its own, centered and lit,
//! and save a close-up screenshot — a headless visual check of the pickup props
//! (parallels QC_GALLERY for monsters).

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::common::{rgb, GfxAssets};
use crate::level::ItemKind;
use crate::pickups::{spawn_pickup, Item};

const KINDS: [(ItemKind, &str); 14] = [
    (ItemKind::WeaponSuperShotgun, "super_shotgun"),
    (ItemKind::WeaponNailgun, "nailgun"),
    (ItemKind::WeaponGrenade, "grenade_launcher"),
    (ItemKind::WeaponRocket, "rocket_launcher"),
    (ItemKind::WeaponLightning, "lightning_gun"),
    (ItemKind::Shells(20), "shells"),
    (ItemKind::Nails(25), "nails"),
    (ItemKind::Rockets(5), "rockets"),
    (ItemKind::Cells(15), "cells"),
    (ItemKind::Health(25), "health"),
    (ItemKind::MegaHealth, "mega_health"),
    (ItemKind::ArmorGreen, "armor_green"),
    (ItemKind::ArmorYellow, "armor_yellow"),
    (ItemKind::SilverKey, "silver_key"),
];

#[derive(Resource)]
pub struct ItemshotState {
    idx: usize,
    t: f32,
    stage: Stage,
}
#[derive(PartialEq, Eq)]
enum Stage {
    Spawn,
    Settle,
    Hold,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gfx: ResMut<GfxAssets>,
) {
    // Pickup models need these shared meshes; `init_gfx` (in setup_level) isn't
    // run in this standalone mode, so build them here.
    gfx.unit_cube = meshes.add(Cuboid::from_size(Vec3::ONE));
    gfx.sphere = meshes.add(Sphere::new(0.5));
    gfx.cylinder = meshes.add(Cylinder { radius: 0.5, half_height: 0.5 });
    gfx.cone = meshes.add(Cone { radius: 0.5, height: 1.0 });

    // Camera framed on a single pickup floating near the origin.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.95, 0.85, -2.1).looking_at(Vec3::new(0.0, 0.25, 0.0), Vec3::Y),
    ));
    // Two-light setup so the forms read; the bright emissive accents do the rest.
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, ..default() },
        Transform::from_xyz(-3.0, 6.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 2200.0, color: rgb(0.55, 0.6, 0.95), ..default() },
        Transform::from_xyz(5.0, 3.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let floor = materials.add(StandardMaterial {
        base_color: rgb(0.07, 0.07, 0.09),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::new(20.0, 0.2, 20.0)))),
        MeshMaterial3d(floor),
        Transform::from_xyz(0.0, -0.3, 0.0),
    ));

    commands.insert_resource(ItemshotState { idx: 0, t: 0.0, stage: Stage::Spawn });
}

pub fn tick(
    time: Res<Time>,
    mut commands: Commands,
    gfx: Res<GfxAssets>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut st: ResMut<ItemshotState>,
    q: Query<Entity, With<Item>>,
) {
    st.t += time.delta_secs();
    match st.stage {
        Stage::Spawn => {
            if st.idx >= KINDS.len() {
                info!("QC_ITEMSHOT: done.");
                std::process::exit(0);
            }
            if q.iter().next().is_none() {
                let (kind, _) = KINDS[st.idx];
                spawn_pickup(&mut commands, &gfx, &assets, &mut materials, kind, Vec3::ZERO);
                st.t = 0.0;
                st.stage = Stage::Settle;
            }
        }
        Stage::Settle => {
            // First item waits longer so the weapon textures finish decoding
            // before the shot; the rest are cached. Spin to a ~3/4 view first.
            let settle = if st.idx == 0 { 5.0 } else { 0.45 };
            if st.t > settle {
                let name = KINDS[st.idx].1;
                let path = format!("itemshot_{}_{}.png", st.idx, name);
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
                info!("QC_ITEMSHOT: shot {}", path);
                st.t = 0.0;
                st.stage = Stage::Hold;
            }
        }
        Stage::Hold => {
            if st.t > 0.8 {
                for e in &q {
                    commands.entity(e).despawn();
                }
                st.idx += 1;
                st.t = 0.0;
                st.stage = Stage::Spawn;
            }
        }
    }
}
