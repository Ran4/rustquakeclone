//! QUAKECLONE — a single-mission Quake-like FPS in Rust + Bevy 0.19.

// Some event-metadata fields (damage source, explosion color, hit_wall) are part
// of the deliberate data model and not consumed by every system yet.
#![allow(dead_code)]

mod audio;
mod audio_gen;
mod combat;
mod common;
mod config;
mod effects;
mod enemies;
mod gallery;
mod gamestate;
mod hud;
mod itemshot;
mod level;
mod levels;
mod levelshot;
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
use bevy::window::{MonitorSelection, PresentMode, WindowMode, WindowResolution};

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
    let levelshot = std::env::var("QC_LEVELSHOT").is_ok();
    let itemshot = std::env::var("QC_ITEMSHOT").is_ok();
    let preview = gallery || levelshot || itemshot; // debug screenshot modes: small window, fast saves
    let cfg = config::Config::load();
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
                        present_mode: if cfg.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync },
                        mode: if cfg.fullscreen && !preview {
                            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                        } else {
                            WindowMode::Windowed
                        },
                        resolution: if preview {
                            WindowResolution::new(1280, 720)
                        } else {
                            WindowResolution::new(cfg.width, cfg.height)
                        },
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
        // configured fresh-run starting level (config.ron `start_level`)
        .insert_resource(StartLevelConfig(match cfg.start_level {
            config::StartLevel::Random => None,
            config::StartLevel::Fixed(n) => Some(n.saturating_sub(1).min(NUM_LEVELS - 1)),
        }))
        // shared resources
        .init_resource::<WorldColliders>()
        .init_resource::<GfxAssets>()
        .init_resource::<Mission>()
        .init_resource::<RunState>()
        .init_resource::<LevelStyle>()
        .init_resource::<LevelIntro>()
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
    } else if levelshot {
        app.add_systems(OnEnter(GameState::Playing), (weapons::create_weapon_vis, levelshot::setup))
            .add_systems(Update, levelshot::tick.run_if(in_state(GameState::Playing)));
    } else if itemshot {
        app.add_systems(OnEnter(GameState::Playing), (weapons::create_weapon_vis, itemshot::setup))
            .add_systems(Update, itemshot::tick.run_if(in_state(GameState::Playing)));
    } else {
        app.add_plugins(hud::HudPlugin).add_systems(
            OnEnter(GameState::Playing),
            (
                weapons::create_weapon_vis,
                level::setup_level,
                player::spawn_player,
                player::grab_cursor,
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

    // Optional frame-time logging (handy for the big level-8 stress test).
    if std::env::var("QC_FPSLOG").is_ok() {
        app.add_plugins((
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::LogDiagnosticsPlugin::default(),
        ));
    }

    // Optional one-shot screenshot: capture the live game (HUD + level banner)
    // a couple seconds into Playing, then exit. `QC_SHOT=name` sets the file.
    if std::env::var("QC_SHOT").is_ok() {
        app.add_systems(Update, oneshot_screenshot.run_if(in_state(GameState::Playing)));
    }

    // Optional headless progression test: teleports the player onto each level's
    // exit so the campaign advances start→…→final win, logging the carried
    // loadout at every level — validates level progression + weapon carry-over.
    if std::env::var("QC_EXIT_RUSH").is_ok() {
        app.add_systems(Update, exit_rush.run_if(in_state(GameState::Playing)))
            .add_systems(OnEnter(GameState::Victory), || info!("EXITRUSH reached VICTORY — campaign complete"));
    }

    app.run();
}

#[derive(Resource)]
struct AutoTest {
    t: f32,
    elapsed: f32,
}

/// Capture one screenshot of the live game ~2s into a level (so the intro banner
/// is still up), then exit. Driven by `QC_SHOT`.
fn oneshot_screenshot(
    time: Res<Time>,
    mut commands: Commands,
    mut t: Local<f32>,
    mut shot: Local<bool>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    *t += time.delta_secs();
    if !*shot && *t > 1.8 {
        let name = std::env::var("QC_SHOT").ok().filter(|s| !s.is_empty() && s != "1").unwrap_or_else(|| "shot".into());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{name}.png")));
        *shot = true;
    }
    if *t > 3.0 {
        std::process::exit(0);
    }
}

/// QC_EXIT_RUSH driver: after a short pause on each level, snap the player onto
/// the exit so the campaign advances; log the level + carried loadout each time
/// the level changes so weapon carry-over can be verified.
#[allow(clippy::type_complexity)]
fn exit_rush(
    time: Res<Time>,
    plan: Res<level::SpawnPlan>,
    run: Res<RunState>,
    mut q: Query<(&mut Transform, &weapons::Inventory, &Health), With<player::Player>>,
    mut t: Local<f32>,
    mut last: Local<Option<usize>>,
) {
    let (name, _) = levels::LEVEL_META[run.level.min(NUM_LEVELS - 1)];
    if *last != Some(run.level) {
        *last = Some(run.level);
        *t = 0.0;
        if let Ok((_, inv, hp)) = q.single() {
            let owned = inv.owned.iter().filter(|o| **o).count();
            info!(
                "EXITRUSH level {} \"{}\": weapons_owned={} ammo={:?} hp={:.0} carry={}",
                run.level + 1, name, owned, inv.ammo, hp.current, run.carry_inventory
            );
        }
    }
    *t += time.delta_secs();
    if *t > 0.6 {
        if let (Some(exit), Ok((mut tf, _, _))) = (plan.exit, q.single_mut()) {
            tf.translation = exit;
        }
    }
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
