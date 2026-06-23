//! QC_GALLERY debug mode: render each monster on its own, centered and lit, and
//! save a close-up screenshot — a headless visual check of the rendered models.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::common::rgb;
use crate::enemies::{spawn_monster, Enemy};
use crate::level::MonsterKind;
use crate::monster_model::MonsterTextures;

const KINDS: [(MonsterKind, &str); 7] = [
    (MonsterKind::Grunt, "grunt"),
    (MonsterKind::Enforcer, "enforcer"),
    (MonsterKind::Knight, "knight"),
    (MonsterKind::Scrag, "scrag"),
    (MonsterKind::Ogre, "ogre"),
    (MonsterKind::DeathKnight, "deathknight"),
    (MonsterKind::Weaver, "weaver"),
];

#[derive(Resource)]
pub struct GalleryState {
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
) {
    // Camera framed on a single monster standing at the origin.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.7, 1.15, -4.4).looking_at(Vec3::new(0.0, 0.75, 0.0), Vec3::Y),
    ));
    // Three-point-ish lighting so the textures and forms read clearly.
    commands.spawn((
        DirectionalLight { illuminance: 6500.0, ..default() },
        Transform::from_xyz(-3.0, 6.0, -5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 2500.0, color: rgb(0.55, 0.6, 0.95), ..default() },
        Transform::from_xyz(5.0, 3.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let floor = materials.add(StandardMaterial {
        base_color: rgb(0.1, 0.1, 0.12),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::new(20.0, 0.2, 20.0)))),
        MeshMaterial3d(floor),
        Transform::from_xyz(0.0, -0.1, 0.0),
    ));

    commands.insert_resource(GalleryState { idx: 0, t: 0.0, stage: Stage::Spawn });
}

pub fn tick(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<MonsterTextures>,
    mut st: ResMut<GalleryState>,
    q: Query<Entity, With<Enemy>>,
) {
    st.t += time.delta_secs();
    // First monster waits longer so all 14 textures finish decoding; the rest are
    // cached and only need a moment to settle into an idle pose.
    let settle = if st.idx == 0 { 5.0 } else { 1.5 };

    match st.stage {
        Stage::Spawn => {
            if st.idx >= KINDS.len() {
                info!("QC_GALLERY: done.");
                std::process::exit(0);
            }
            // Wait until any previous monster has fully despawned.
            if q.iter().next().is_none() {
                let (kind, _) = KINDS[st.idx];
                spawn_monster(&mut commands, &mut meshes, &mut materials, &tex, kind, Vec3::ZERO);
                st.t = 0.0;
                st.stage = Stage::Settle;
            }
        }
        Stage::Settle => {
            if st.t > settle {
                let name = KINDS[st.idx].1;
                let path = format!("gallery_{}_{}.png", st.idx, name);
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path.clone()));
                info!("QC_GALLERY: shot {}", path);
                st.t = 0.0;
                st.stage = Stage::Hold;
            }
        }
        Stage::Hold => {
            if st.t > 1.2 {
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
