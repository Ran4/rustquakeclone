//! Heads-up display: crosshair, health / armor / ammo / weapon, objective,
//! kill count, damage flash, pickup notifications and a controls hint.

use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::time::Real;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::common::*;
use crate::player::Player;
use crate::weapons::Inventory;

pub struct HudPlugin;
impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlashState>()
            .init_resource::<NotifyState>()
            .init_resource::<FpsMeter>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(
                Update,
                (update_hud, update_objective, update_flash, update_notify, update_hint, update_fps, update_level_banner)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct HealthText;
#[derive(Component)]
struct ArmorText;
#[derive(Component)]
struct AmmoText;
#[derive(Component)]
struct WeaponText;
#[derive(Component)]
struct ObjectiveText;
#[derive(Component)]
struct FlashOverlay;
#[derive(Component)]
struct NotifyText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct FpsText;
#[derive(Component)]
struct LevelBannerText;

/// Exponentially-smoothed frames-per-second, updated from the real-time clock.
#[derive(Resource, Default)]
struct FpsMeter(f32);

#[derive(Resource, Default)]
struct FlashState {
    color: Color,
    strength: f32,
}

#[derive(Resource, Default)]
struct NotifyState {
    text: String,
    timer: f32,
}

fn big() -> TextFont {
    TextFont { font_size: FontSize::Px(34.0), ..default() }
}
fn small() -> TextFont {
    TextFont { font_size: FontSize::Px(20.0), ..default() }
}

fn spawn_hud(mut commands: Commands, existing: Query<Entity, With<HudRoot>>) {
    if !existing.is_empty() {
        return;
    }

    // Damage / pickup flash overlay (full-screen tint).
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(1.0, 0.0, 0.0, 0.0)),
        GlobalZIndex(10),
        FlashOverlay,
        HudRoot,
        LevelEntity,
    ));

    // Crosshair "+".
    let cross = rgb(0.85, 0.9, 0.85);
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), ..default() },
            GlobalZIndex(11),
            LevelEntity,
            HudRoot,
        ))
        .with_children(|p| {
            p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(-9.0), top: Val::Px(-1.0), width: Val::Px(18.0), height: Val::Px(2.0), ..default() }, BackgroundColor(cross)));
            p.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(-1.0), top: Val::Px(-9.0), width: Val::Px(2.0), height: Val::Px(18.0), ..default() }, BackgroundColor(cross)));
        });

    // Bottom-left: health + armor.
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(24.0), bottom: Val::Px(48.0), ..default() },
        Text::new("100"),
        big(),
        TextColor(rgb(0.3, 1.0, 0.3)),
        GlobalZIndex(11),
        HealthText,
        LevelEntity,
        HudRoot,
    ));
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(24.0), bottom: Val::Px(24.0), ..default() },
        Text::new("Armor 0"),
        small(),
        TextColor(rgb(0.6, 0.8, 1.0)),
        GlobalZIndex(11),
        ArmorText,
        LevelEntity,
        HudRoot,
    ));

    // Bottom-right: ammo + weapon name.
    commands.spawn((
        Node { position_type: PositionType::Absolute, right: Val::Px(24.0), bottom: Val::Px(48.0), ..default() },
        Text::new("25"),
        big(),
        TextColor(rgb(1.0, 0.9, 0.4)),
        GlobalZIndex(11),
        AmmoText,
        LevelEntity,
        HudRoot,
    ));
    commands.spawn((
        Node { position_type: PositionType::Absolute, right: Val::Px(24.0), bottom: Val::Px(24.0), ..default() },
        Text::new("Shotgun"),
        small(),
        TextColor(rgb(0.85, 0.85, 0.85)),
        GlobalZIndex(11),
        WeaponText,
        LevelEntity,
        HudRoot,
    ));

    // Top-left: FPS meter.
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(24.0), top: Val::Px(18.0), ..default() },
        Text::new("-- fps"),
        small(),
        TextColor(rgb(0.5, 1.0, 0.6)),
        GlobalZIndex(11),
        FpsText,
        LevelEntity,
        HudRoot,
    ));

    // Top-center: objective + kills.
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Px(18.0), left: Val::Percent(50.0), margin: UiRect { left: Val::Px(-180.0), ..default() }, width: Val::Px(360.0), justify_content: JustifyContent::Center, ..default() },
        Text::new("Find the Silver Key"),
        small(),
        TextColor(rgb(1.0, 0.85, 0.4)),
        TextLayout { justify: Justify::Center, ..default() },
        GlobalZIndex(11),
        ObjectiveText,
        LevelEntity,
        HudRoot,
    ));

    // Notification line (below objective).
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Px(54.0), left: Val::Percent(50.0), margin: UiRect { left: Val::Px(-200.0), ..default() }, width: Val::Px(400.0), justify_content: JustifyContent::Center, ..default() },
        Text::new(""),
        small(),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        TextLayout { justify: Justify::Center, ..default() },
        GlobalZIndex(11),
        NotifyText,
        LevelEntity,
        HudRoot,
    ));

    // Level intro banner (big, fades out a few seconds after a level starts).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(30.0),
            left: Val::Percent(50.0),
            margin: UiRect { left: Val::Px(-380.0), ..default() },
            width: Val::Px(760.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(""),
        TextFont { font_size: FontSize::Px(54.0), ..default() },
        TextColor(Color::srgba(1.0, 0.85, 0.35, 0.0)),
        TextLayout { justify: Justify::Center, ..default() },
        GlobalZIndex(12),
        LevelBannerText,
        LevelEntity,
        HudRoot,
    ));

    // Center hint (shown when the cursor isn't captured).
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Percent(62.0), left: Val::Percent(50.0), margin: UiRect { left: Val::Px(-260.0), ..default() }, width: Val::Px(520.0), justify_content: JustifyContent::Center, ..default() },
        Text::new("Click to play  •  WASD move  •  Space jump  •  Mouse aim  •  LMB fire  •  RMB grapnel (Whip) + Ctrl reel  •  1-7 weapons"),
        small(),
        TextColor(rgb(0.9, 0.9, 0.7)),
        TextLayout { justify: Justify::Center, ..default() },
        GlobalZIndex(11),
        HintText,
        LevelEntity,
        HudRoot,
    ));
}

#[allow(clippy::type_complexity)]
fn update_hud(
    q_player: Query<(&Health, &Armor, &Inventory), With<Player>>,
    mut texts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<HealthText>>,
        Query<&mut Text, With<ArmorText>>,
        Query<&mut Text, With<AmmoText>>,
        Query<&mut Text, With<WeaponText>>,
    )>,
) {
    let Ok((health, armor, inv)) = q_player.single() else { return };

    if let Ok((mut t, mut col)) = texts.p0().single_mut() {
        t.0 = format!("{}", health.current.max(0.0).ceil() as i32);
        let r = (health.current / health.max).clamp(0.0, 1.0);
        col.0 = if r > 0.5 { rgb(0.3, 1.0, 0.3) } else if r > 0.25 { rgb(1.0, 0.9, 0.3) } else { rgb(1.0, 0.25, 0.2) };
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        t.0 = format!("Armor {}", armor.points.max(0.0) as i32);
    }
    if let Ok(mut t) = texts.p2().single_mut() {
        t.0 = if inv.current.infinite() {
            "\u{221e}".to_string() // ∞ — the whip needs no ammo
        } else {
            format!("{}", inv.ammo[inv.current.ammo()])
        };
    }
    if let Ok(mut t) = texts.p3().single_mut() {
        t.0 = inv.current.name().to_string();
    }
}

fn update_objective(mission: Res<Mission>, mut q: Query<&mut Text, With<ObjectiveText>>) {
    if let Ok(mut t) = q.single_mut() {
        t.0 = format!("{}   [{}/{} slain]", mission.objective, mission.kills, mission.total_enemies);
    }
}

fn update_flash(
    time: Res<Time>,
    mut state: ResMut<FlashState>,
    mut reader: MessageReader<ScreenFlash>,
    mut q: Query<&mut BackgroundColor, With<FlashOverlay>>,
) {
    for ev in reader.read() {
        if ev.strength >= state.strength {
            state.color = ev.color;
            state.strength = ev.strength;
        }
    }
    state.strength = (state.strength - time.delta_secs() * 1.5).max(0.0);
    if let Ok(mut bg) = q.single_mut() {
        let c = state.color.to_srgba();
        bg.0 = Color::srgba(c.red, c.green, c.blue, state.strength);
    }
}

fn update_notify(
    time: Res<Time>,
    mut state: ResMut<NotifyState>,
    mut reader: MessageReader<Notify>,
    mut q: Query<(&mut Text, &mut TextColor), With<NotifyText>>,
) {
    for ev in reader.read() {
        state.text = ev.text.clone();
        state.timer = 3.0;
    }
    state.timer = (state.timer - time.delta_secs()).max(0.0);
    if let Ok((mut t, mut col)) = q.single_mut() {
        if t.0 != state.text {
            t.0 = state.text.clone();
        }
        // fade in (3.0->2.8s), hold, fade out (last 0.4s)
        let a = if state.timer > 2.8 {
            (3.0 - state.timer) / 0.2
        } else if state.timer > 0.4 {
            1.0
        } else {
            state.timer / 0.4
        };
        col.0 = Color::srgba(1.0, 1.0, 0.85, a.clamp(0.0, 1.0));
    }
}

fn update_fps(
    time: Res<Time<Real>>,
    mut meter: ResMut<FpsMeter>,
    mut q: Query<&mut Text, With<FpsText>>,
) {
    let dt = time.delta_secs();
    if dt > 0.0 {
        let inst = 1.0 / dt;
        // Exponential moving average so the readout doesn't flicker every frame.
        meter.0 = if meter.0 <= 0.0 { inst } else { meter.0 * 0.9 + inst * 0.1 };
    }
    if let Ok(mut t) = q.single_mut() {
        t.0 = format!("{:.0} fps", meter.0);
    }
}

/// Fade the "Level N: Name" banner in for ~0.4s, hold, then fade out.
fn update_level_banner(
    time: Res<Time>,
    mut intro: ResMut<LevelIntro>,
    mut q: Query<(&mut Text, &mut TextColor), With<LevelBannerText>>,
) {
    const FULL: f32 = 4.5;
    if intro.timer > 0.0 {
        intro.timer = (intro.timer - time.delta_secs()).max(0.0);
    }
    let Ok((mut t, mut col)) = q.single_mut() else { return };
    if t.0 != intro.text {
        t.0 = intro.text.clone();
    }
    let a = if intro.timer <= 0.0 {
        0.0
    } else if intro.timer > FULL - 0.4 {
        (FULL - intro.timer) / 0.4
    } else if intro.timer > 1.0 {
        1.0
    } else {
        intro.timer
    };
    col.0 = Color::srgba(1.0, 0.85, 0.35, a.clamp(0.0, 1.0));
}

fn update_hint(
    windows: Query<&CursorOptions, With<PrimaryWindow>>,
    mut q: Query<&mut Node, With<HintText>>,
) {
    let grabbed = windows.single().map(|c| c.grab_mode != CursorGrabMode::None).unwrap_or(false);
    if let Ok(mut node) = q.single_mut() {
        node.display = if grabbed { Display::None } else { Display::Flex };
    }
}
