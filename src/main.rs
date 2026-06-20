//! QUAKECLONE — a single-mission Quake-like FPS in Rust + Bevy 0.19.

// Some event-metadata fields (damage source, explosion color, hit_wall) are part
// of the deliberate data model and not consumed by every system yet.
#![allow(dead_code)]

mod audio;
mod audio_gen;
mod combat;
mod common;
mod effects;
mod enemies;
mod gallery;
mod gamestate;
mod hud;
mod level;
mod monster_model;
mod physics;
mod pickups;
mod player;
mod projectiles;
mod weapons;

use bevy::asset::AssetPlugin;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::time::Fixed;
use bevy::window::PresentMode;

use common::*;

/// Absolute path to the asset folder, resolved the same way regardless of how
/// the game is launched (via `cargo run` or the binary directly).
fn assets_dir() -> String {
    let base = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("assets").to_string_lossy().into_owned()
}

fn main() {
    let assets = assets_dir();
    let gallery = std::env::var("QC_GALLERY").is_ok();
    // Synthesize the SFX set into <assets>/sounds/ before the engine starts so
    // the AssetServer (pointed at the same dir) can load them.
    audio_gen::generate(&assets);

    let mut app = App::new();
    app
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "QUAKECLONE — Dimension of the Doomed".into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: assets,
                    ..default()
                }),
        )
        // Run physics & AI at a stable 60 Hz.
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .insert_resource(ClearColor(rgb(0.02, 0.02, 0.03)))
        .insert_resource(GlobalAmbientLight {
            brightness: 200.0,
            color: rgb(0.45, 0.45, 0.6),
            affects_lightmapped_meshes: true,
        })
        .init_state::<GameState>()
        // shared resources
        .init_resource::<WorldColliders>()
        .init_resource::<GfxAssets>()
        .init_resource::<Mission>()
        .init_resource::<Sounds>()
        // messages
        .add_message::<DamageEvent>()
        .add_message::<ExplosionEvent>()
        .add_message::<ImpactEvent>()
        .add_message::<Sfx>()
        .add_message::<ScreenShake>()
        .add_message::<ScreenFlash>()
        .add_message::<Notify>()
        // feature plugins
        .add_plugins((
            level::LevelPlugin,
            player::PlayerPlugin,
            weapons::WeaponsPlugin,
            projectiles::ProjectilePlugin,
            combat::CombatPlugin,
            effects::EffectsPlugin,
            enemies::EnemiesPlugin,
            monster_model::MonsterModelPlugin,
            pickups::PickupsPlugin,
            gamestate::GameStatePlugin,
            audio::AudioPlugin,
        ));

    // World setup runs at startup and on every restart into Playing. The optional
    // QC_GALLERY mode swaps the real level for a line-up of every monster and
    // screenshots it — a headless visual check of the rendered models.
    if gallery {
        app.add_systems(
            OnEnter(GameState::Playing),
            (weapons::create_weapon_vis, gallery::setup),
        )
        .add_systems(Update, gallery::tick);
    } else {
        app.add_plugins(hud::HudPlugin).add_systems(
            OnEnter(GameState::Playing),
            (
                weapons::create_weapon_vis,
                level::setup_level,
                player::spawn_player,
                weapons::setup_player_weapons,
                enemies::spawn_monsters,
                pickups::spawn_items,
            )
                .chain(),
        );
    }

    // Optional headless self-test: drives the player forward + fires so we can
    // validate movement/collision/combat without a human at the controls.
    if std::env::var("QC_AUTOTEST").is_ok() {
        app.insert_resource(AutoTest { t: 0.0, elapsed: 0.0 })
            .add_systems(Update, (autotest_drive, autotest_log).run_if(in_state(GameState::Playing)));
    }

    app.run();
}

#[derive(Resource)]
struct AutoTest {
    t: f32,
    elapsed: f32,
}

fn autotest_drive(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    _at: Res<AutoTest>,
) {
    keys.press(KeyCode::KeyW);
    mouse.press(MouseButton::Left);
}

fn autotest_log(
    time: Res<Time>,
    mut at: ResMut<AutoTest>,
    q: Query<(&Transform, &player::Player)>,
    hp: Query<&Health, With<player::Player>>,
    mission: Res<Mission>,
) {
    at.t += time.delta_secs();
    at.elapsed += time.delta_secs();
    if at.t > 0.5 {
        at.t = 0.0;
        if let Ok((tf, p)) = q.single() {
            let health = hp.single().map(|h| h.current).unwrap_or(-1.0);
            info!(
                "AUTOTEST t={:.1} pos=({:.2},{:.2},{:.2}) speed={:.2} ground={} hp={:.0} kills={}",
                at.elapsed, tf.translation.x, tf.translation.y, tf.translation.z,
                Vec3::new(p.vel.x, 0.0, p.vel.z).length(), p.on_ground, health, mission.kills
            );
        }
    }
}
