//! Mission flow: objective, the key-locked door, lava hazard, the exit, plus
//! death / victory screens and restart.

use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::common::{tune::PLAYER_HALF, *};
use crate::level::{LavaVolumes, SpawnPlan};
use crate::physics::Aabb;
use crate::player::Player;
use crate::weapons::Inventory;

pub struct GameStatePlugin;
impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (objective, door_system, lava_damage, corpse_hazard, exit_system, animate_lava)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, restart_system)
        // One-frame bounce: tear down the old level, then rebuild the next one.
        .add_systems(OnEnter(GameState::Loading), enter_loading)
        .add_systems(OnEnter(GameState::Dead), (release_cursor, spawn_dead_ui))
        .add_systems(OnExit(GameState::Dead), despawn_overlay)
        .add_systems(OnEnter(GameState::Victory), (release_cursor, victory_jingle, spawn_victory_ui))
        .add_systems(OnExit(GameState::Victory), despawn_overlay);
    }
}

/// Loading is a transient state: despawn the finished level, then re-enter
/// Playing so the OnEnter(Playing) chain builds the next level.
fn enter_loading(
    mut commands: Commands,
    q_level: Query<Entity, With<LevelEntity>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for e in &q_level {
        commands.entity(e).despawn();
    }
    next.set(GameState::Playing);
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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn lava_damage(
    time: Res<Time>,
    lava: Res<LavaVolumes>,
    style: Res<LevelStyle>,
    q_player: Query<(Entity, &Transform), With<Player>>,
    q_monsters: Query<(Entity, &Transform, &Hurtbox), (With<crate::enemies::Enemy>, Without<crate::monster_model::Dying>, Without<crate::enemies::PinnedCorpse>, Without<Player>)>,
    mut dmg: MessageWriter<DamageEvent>,
    mut flash: MessageWriter<ScreenFlash>,
    mut acc: Local<f32>,
    mut macc: Local<f32>,
) {
    let dt = time.delta_secs();

    // Monsters cook in the same hazard (feature 41): a monster whose body overlaps a
    // lava volume takes the same 0.3s tick the player does — this is what lets a nail-
    // pinned (or shoved) monster bake in the lava. In practice living monsters rarely
    // overlap the volume (they stand on floor brushes whose surface sits above it), so
    // it's usually a no-op; a dead pinned corpse is excluded outright so it can't spam
    // no-op DoT events on its already-dead Health. The player tick below is unchanged.
    *macc += dt;
    if *macc >= 0.3 {
        *macc = 0.0;
        for (me, mtf, mhb) in &q_monsters {
            let mbox = Aabb::from_center_half(mtf.translation, mhb.half);
            if lava.volumes.iter().any(|v| v.overlaps(&mbox)) {
                dmg.write(DamageEvent::body(me, style.hazard_dot, None, Vec3::Y * 3.5));
            }
        }
    }

    let Ok((pe, ptf)) = q_player.single() else { return };

    // Fell into the bottomless void (e.g. level 3's foundry shaft): die the
    // instant you plunge past the molten core, however far you've drifted.
    if ptf.translation.y < lava.kill_y {
        flash.write(ScreenFlash { color: style.hazard_flash, strength: 1.0 });
        dmg.write(DamageEvent::body(pe, 10_000.0, None, Vec3::ZERO));
        return;
    }

    let pbox = Aabb::from_center_half(ptf.translation, Vec3::from_array(PLAYER_HALF));
    let burning = lava.volumes.iter().any(|v| v.overlaps(&pbox));
    if burning {
        // continuous hazard-tinted singe + periodic damage ticks
        flash.write(ScreenFlash { color: style.hazard_flash, strength: 0.3 });
        *acc += dt;
        if *acc >= 0.3 {
            *acc = 0.0;
            dmg.write(DamageEvent::body(pe, style.hazard_dot, None, Vec3::Y * 3.5));
        }
    } else {
        *acc = 0.0;
    }
}

/// Dissolve a ragdoll-brush corpse (feature 17) dropped into lava/hazard or off
/// the world: despawn it and let `reclaim_corpse_slots` reclaim its collider slot
/// next frame. This is what makes "boot a body into the lava and it's gone" pay
/// off — and stops a body skidding off a ledge from becoming a phantom collider
/// floating in the void.
fn corpse_hazard(
    mut commands: Commands,
    lava: Res<LavaVolumes>,
    mut colliders: ResMut<WorldColliders>,
    q_corpse: Query<(Entity, &crate::corpse::Ragdoll)>,
) {
    for (e, rag) in &q_corpse {
        // Test the flopped body's tracking box, not the root transform (the root
        // stays put at the death spot while the bones ragdoll away from it).
        let cbox = rag.body_box;
        let in_lava = lava.volumes.iter().any(|v| v.overlaps(&cbox));
        if in_lava || cbox.center().y < lava.kill_y {
            // Retire the collision slot the same frame we despawn the body, so it
            // doesn't linger as a one-frame phantom solid in the lava / over the
            // void before `reclaim_ragdoll_slots` frees it next frame.
            if let Some(s) = rag.slot().and_then(|i| colliders.solids.get_mut(i)) {
                *s = crate::corpse::degenerate();
            }
            commands.entity(e).despawn();
        }
    }
}

/// Pulse the active hazard material's emissive so it looks molten/alive.
fn animate_lava(
    time: Res<Time>,
    gfx: Res<GfxAssets>,
    style: Res<LevelStyle>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Some(mut m) = materials.get_mut(&gfx.lava_mat) {
        let base = style.hazard_emissive;
        let p = 0.7 + 0.45 * (time.elapsed_secs() * 2.3).sin();
        m.emissive = LinearRgba::rgb(base.red * p, base.green * p, base.blue * p);
    }
}

/// Reaching the exit slipgate: advance to the next level carrying the player's
/// loadout, or — if this was the last level — win the campaign.
fn exit_system(
    plan: Res<SpawnPlan>,
    mut run: ResMut<RunState>,
    q_player: Query<(&Transform, &Health, &Armor, &Inventory), With<Player>>,
    mut next: ResMut<NextState<GameState>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(exit) = plan.exit else { return };
    let Ok((tf, hp, armor, inv)) = q_player.single() else { return };
    if tf.translation.distance(exit) >= 3.0 {
        return;
    }
    if run.level + 1 >= NUM_LEVELS {
        next.set(GameState::Victory); // finished the final dimension — total win
        return;
    }
    // Snapshot the loadout and bounce through Loading to build the next level.
    run.carry = Carry {
        owned: inv.owned,
        ammo: inv.ammo,
        current: inv.current.index(),
        health: hp.current.max(1.0),
        armor_points: armor.points,
        armor_absorb: armor.absorb,
    };
    run.carry_inventory = true;
    run.level += 1;
    sfx.write(Sfx::global(Sound::Victory));
    next.set(GameState::Loading);
}

fn restart_system(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
    mut run: ResMut<RunState>,
    q_level: Query<Entity, With<LevelEntity>>,
) {
    let over = matches!(state.get(), GameState::Dead | GameState::Victory);
    if over && keys.just_pressed(KeyCode::KeyR) {
        // A full restart is a fresh run: random level, default loadout.
        run.carry_inventory = false;
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

fn spawn_victory_ui(mut commands: Commands) {
    commands.spawn(overlay_root()).with_children(|p| {
        p.spawn((Text::new("CONGRATULATIONS!"), TextFont { font_size: FontSize::Px(82.0), ..default() }, TextColor(rgb(1.0, 0.85, 0.3))));
        p.spawn((Text::new("You have conquered all seven dimensions."), TextFont { font_size: FontSize::Px(30.0), ..default() }, TextColor(rgb(0.95, 0.95, 0.95))));
        p.spawn((Text::new("YOU WIN"), TextFont { font_size: FontSize::Px(40.0), ..default() }, TextColor(rgb(0.6, 1.0, 0.6))));
        p.spawn((Text::new("Press R to play again"), TextFont { font_size: FontSize::Px(26.0), ..default() }, TextColor(rgb(0.8, 0.8, 0.8))));
    });
}

fn despawn_overlay(mut commands: Commands, q: Query<Entity, With<Overlay>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}
