//! Mission flow: objective, the key-locked door, lava hazard, the exit, plus
//! death / victory screens and restart.

use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::common::{tune::PLAYER_HALF, *};
use crate::level::{LavaVolumes, SpawnPlan};
use crate::physics::Aabb;
use crate::player::Player;

pub struct GameStatePlugin;
impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (objective, door_system, lava_damage, exit_system, animate_lava)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, restart_system)
        .add_systems(OnEnter(GameState::Dead), (release_cursor, spawn_dead_ui))
        .add_systems(OnExit(GameState::Dead), despawn_overlay)
        .add_systems(OnEnter(GameState::Victory), (release_cursor, victory_jingle, spawn_victory_ui))
        .add_systems(OnExit(GameState::Victory), despawn_overlay);
    }
}

fn objective(mut mission: ResMut<Mission>) {
    mission.objective = if mission.has_key {
        "Reach the Exit slipgate".into()
    } else {
        "Find the Silver Key".into()
    };
}

fn degenerate() -> Aabb {
    Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

fn door_system(
    time: Res<Time>,
    mut colliders: ResMut<WorldColliders>,
    mission: Res<Mission>,
    q_player: Query<&Transform, With<Player>>,
    mut q_doors: Query<(&mut Transform, &mut Door), Without<Player>>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
) {
    let dt = time.delta_secs();
    let Ok(ptf) = q_player.single() else { return };
    let ppos = ptf.translation;
    for (mut tf, mut door) in &mut q_doors {
        if door.opened {
            continue;
        }
        if !door.opening {
            if mission.has_key && ppos.distance(door.closed_pos) < 6.0 {
                door.opening = true;
                if let Some(s) = colliders.solids.get_mut(door.solid_index) {
                    *s = degenerate();
                }
                sfx.write(Sfx::at(Sound::Door, door.closed_pos));
                notify.write(Notify::new("The door grinds open!"));
            }
        } else {
            door.t = (door.t + dt * 0.6).min(1.0);
            let sm = door.t * door.t * (3.0 - 2.0 * door.t);
            tf.translation = door.closed_pos + door.open_offset * sm;
            if door.t >= 1.0 {
                door.opened = true;
            }
        }
    }
}

fn lava_damage(
    time: Res<Time>,
    lava: Res<LavaVolumes>,
    q_player: Query<(Entity, &Transform), With<Player>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut flash: MessageWriter<ScreenFlash>,
    mut acc: Local<f32>,
) {
    let dt = time.delta_secs();
    let Ok((pe, ptf)) = q_player.single() else { return };
    let pbox = Aabb::from_center_half(ptf.translation, Vec3::from_array(PLAYER_HALF));
    let burning = lava.volumes.iter().any(|v| v.overlaps(&pbox));
    if burning {
        // continuous orange singe + periodic damage ticks
        flash.write(ScreenFlash { color: rgb(0.9, 0.35, 0.05), strength: 0.3 });
        *acc += dt;
        if *acc >= 0.3 {
            *acc = 0.0;
            dmg.write(DamageEvent { target: pe, amount: 12.0, source: None, knockback: Vec3::Y * 3.5 });
        }
    } else {
        *acc = 0.0;
    }
}

/// Pulse the shared lava material's emissive so it looks molten/alive.
fn animate_lava(time: Res<Time>, gfx: Res<GfxAssets>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if let Some(mut m) = materials.get_mut(&gfx.lava_mat) {
        let p = 0.7 + 0.45 * (time.elapsed_secs() * 2.3).sin();
        m.emissive = LinearRgba::rgb(5.0 * p, 1.2 * p, 0.1 * p);
    }
}

fn exit_system(
    plan: Res<SpawnPlan>,
    q_player: Query<&Transform, With<Player>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if let (Some(exit), Ok(ptf)) = (plan.exit, q_player.single()) {
        if ptf.translation.distance(exit) < 3.0 {
            next.set(GameState::Victory);
        }
    }
}

fn restart_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
    q_level: Query<Entity, With<LevelEntity>>,
) {
    let over = matches!(state.get(), GameState::Dead | GameState::Victory);
    if over && keys.just_pressed(KeyCode::KeyR) {
        for e in &q_level {
            commands.entity(e).despawn();
        }
        next.set(GameState::Playing);
    }
}

fn release_cursor(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut c) = windows.single_mut() {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
}

fn victory_jingle(mut sfx: MessageWriter<Sfx>) {
    sfx.write(Sfx::global(Sound::Victory));
}

#[derive(Component)]
struct Overlay;

fn overlay_root() -> (Node, BackgroundColor, GlobalZIndex, Overlay) {
    (
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(18.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        GlobalZIndex(100),
        Overlay,
    )
}

fn spawn_dead_ui(mut commands: Commands) {
    commands.spawn(overlay_root()).with_children(|p| {
        p.spawn((Text::new("YOU DIED"), TextFont { font_size: FontSize::Px(84.0), ..default() }, TextColor(rgb(0.85, 0.1, 0.1))));
        p.spawn((Text::new("Press R to try again"), TextFont { font_size: FontSize::Px(30.0), ..default() }, TextColor(rgb(0.9, 0.9, 0.9))));
    });
}

fn spawn_victory_ui(mut commands: Commands, mission: Res<Mission>) {
    let kills = mission.kills;
    let total = mission.total_enemies;
    commands.spawn(overlay_root()).with_children(|p| {
        p.spawn((Text::new("YOU ESCAPED!"), TextFont { font_size: FontSize::Px(80.0), ..default() }, TextColor(rgb(0.9, 0.8, 0.3))));
        p.spawn((Text::new(format!("Slain: {kills} / {total} fiends")), TextFont { font_size: FontSize::Px(30.0), ..default() }, TextColor(rgb(0.9, 0.9, 0.9))));
        p.spawn((Text::new("Press R to play again"), TextFont { font_size: FontSize::Px(26.0), ..default() }, TextColor(rgb(0.8, 0.8, 0.8))));
    });
}

fn despawn_overlay(mut commands: Commands, q: Query<Entity, With<Overlay>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
