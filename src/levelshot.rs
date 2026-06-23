//! QC_LEVELSHOT debug mode: build each level in turn, frame it with a high
//! bird's-eye camera and save `levelshot_<n>_<slug>.png` — a headless way to
//! eyeball every level's geometry, textures and lighting.

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::render::view::Msaa;

use crate::common::*;
use crate::level::{apply_theme_and_build, LavaVolumes, PlayerStart, SpawnPlan, StyleOut};

#[derive(Component)]
pub struct ShotCam;

#[derive(Resource)]
pub struct LevelShot {
    idx: usize,
    t: f32,
    stage: Stage,
}
#[derive(PartialEq, Eq)]
enum Stage {
    Build,
    Settle,
    Hold,
}

pub fn setup(mut commands: Commands) {
    commands.insert_resource(LevelShot { idx: 0, t: 0.0, stage: Stage::Build });
}

#[allow(clippy::too_many_arguments)]
pub fn tick(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
    mut colliders: ResMut<WorldColliders>,
    mut start: ResMut<PlayerStart>,
    mut plan: ResMut<SpawnPlan>,
    mut lava: ResMut<LavaVolumes>,
    mut gfx: ResMut<GfxAssets>,
    mut so: StyleOut,
    mut st: ResMut<LevelShot>,
    q_old: Query<Entity, With<LevelEntity>>,
    q_cam: Query<Entity, With<ShotCam>>,
) {
    st.t += time.delta_secs();
    match st.stage {
        Stage::Build => {
            if st.idx >= NUM_LEVELS {
                info!("QC_LEVELSHOT: done.");
                std::process::exit(0);
            }
            for e in &q_old {
                commands.entity(e).despawn();
            }
            for e in &q_cam {
                commands.entity(e).despawn();
            }
            let _bounds = apply_theme_and_build(
                st.idx, &mut commands, &mut meshes, &mut materials, &assets, &mut colliders,
                &mut start, &mut plan, &mut lava, &mut gfx, &mut so.style, &mut so.clear,
                &mut so.ambient, &mut so.resonant,
            );
            spawn_shot_cam(&mut commands, start.pos, start.yaw, &so.style);
            st.t = 0.0;
            st.stage = Stage::Settle;
        }
        Stage::Settle => {
            // The first build waits longer so all textures finish decoding.
            let wait = if st.idx == 0 { 4.5 } else { 2.6 };
            if st.t > wait {
                let (name, _) = crate::levels::LEVEL_META[st.idx];
                let slug: String = name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                    .collect();
                let path = format!("levelshot_{}_{}.png", st.idx + 1, slug);
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
                info!("QC_LEVELSHOT: shot {}", path);
                st.t = 0.0;
                st.stage = Stage::Hold;
            }
        }
        Stage::Hold => {
            if st.t > 1.0 {
                st.idx += 1;
                st.t = 0.0;
                st.stage = Stage::Build;
            }
        }
    }
}

fn spawn_shot_cam(commands: &mut Commands, spawn: Vec3, yaw: f32, style: &LevelStyle) {
    // Render from the player's entry vantage: eye height at the spawn, looking
    // the way the player faces with a slight downward tilt. This shows each
    // level's first room with its real textures, fog and lighting.
    let eye = spawn + Vec3::new(0.0, 0.7, 0.0);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-0.14);
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 82f32.to_radians(), near: 0.04, ..default() }),
        Hdr,
        Msaa::Sample4,
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        crate::level::apply_fog(style),
        Transform::from_translation(eye).with_rotation(rot),
        ShotCam,
    ));
}
