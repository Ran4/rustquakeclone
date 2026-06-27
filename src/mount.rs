//! Hijack the Horde — mount and drive a whip-staggered Ogre.
//!
//! Crack the Whip across an Ogre to land a heavy stagger; that opens a brief
//! [`Mountable`] window (longer than the 0.22s pain flinch so an **E**-press is
//! actually achievable). Press **E** while standing near the staggered Ogre to
//! climb aboard: now you drive the *live monster rig itself* as a battering ram —
//! **W** charges it forward, **A/D** steer its lumbering gait, and **left mouse**
//! swings its own chainsaw arm into whatever's ahead. You ride on the Ogre's HP
//! (damage aimed at you is redirected to the Ogre — see `combat::apply_damage`)
//! until it dies, at which point its death-topple bucks you off. **E** dismounts
//! early.
//!
//! ## How it hooks into the rest of the engine
//!
//! This mirrors the truck (`vehicle.rs`): an [`ActiveMount`] *resource* (not a
//! marker) so `player_move` and the drive system see the mount the same frame
//! with no command-buffer latency, the same `.after(player_look).before(player_move)`
//! system-set window, and an `OnExit(Playing)` cleanup that clears the handle so a
//! fresh level never starts phantom-mounted.
//!
//! Unlike the truck, the mount needs **no second pose source and no reserved
//! collider slot**. The procedural bone animator (`monster_model::animate_monsters`)
//! already poses the Ogre purely from `Enemy` fields (`vel`/`gait`/`atk_anim`/
//! `pain`). We just become the writer of those fields — and to stop the AI from
//! fighting us, the mounted Ogre carries a [`Mounted`] marker that excludes it from
//! `enemy_ai` (and `assign_squad_roles`). The rig keeps animating with NO pose
//! switch: walk cycle from throttle, chainsaw swing on fire, pain flinch on hits.

use bevy::prelude::*;

use crate::common::{tune::{GRAVITY, PLAYER_HALF, STEP_HEIGHT}, *};
use crate::enemies::Enemy;
use crate::level::MonsterKind;
use crate::monster_model::Dying;
use crate::physics::move_and_slide;
use crate::player::Player;

// ----------------------------------------------------------------------------
// Tuning (units: meters, seconds, radians).
// ----------------------------------------------------------------------------
/// How long a whip-staggered Ogre stays mountable. Longer than the 0.22s pain
/// flinch so closing the gap and pressing E is realistically achievable.
const MOUNT_WINDOW: f32 = 1.5;
/// How close (planar) the player must be to a `Mountable` Ogre to climb aboard.
const MOUNT_REACH: f32 = 3.5;

// -- driving feel ------------------------------------------------------------
/// Forward acceleration under throttle (W).
const OGRE_ACCEL: f32 = 14.0;
/// Brake / reverse deceleration (S).
const OGRE_BRAKE: f32 = 18.0;
/// Passive deceleration when coasting (no throttle).
const OGRE_DRAG: f32 = 8.0;
/// Forward top speed — a slow, heavy charge.
const OGRE_MAX_SPEED: f32 = 10.0;
/// Reverse top speed (a clumsy backstep).
const OGRE_MAX_REVERSE: f32 = 4.0;
/// Steering rate (rad/s) at full authority.
const OGRE_TURN_RATE: f32 = 1.8;
/// Speed (m/s) at which steering reaches full authority (no turning while stopped).
const OGRE_TURN_REF: f32 = 4.0;

// -- chainsaw (left mouse) ---------------------------------------------------
/// Seconds between chainsaw swings.
const CHAINSAW_CD: f32 = 0.5;
/// Damage per chainsaw hit.
const CHAINSAW_DAMAGE: f32 = 35.0;
/// Reach of the chainsaw arc in front of the Ogre.
const CHAINSAW_RANGE: f32 = 3.5;
/// Cosine of the chainsaw's half-arc (≈ a 70° half-cone ahead).
const CHAINSAW_ARC_COS: f32 = 0.34;
/// Knockback magnitude of a chainsaw hit (heavy shove + a little lift).
const CHAINSAW_KNOCK: f32 = 16.0;

// -- ramming -----------------------------------------------------------------
/// Min planar Ogre speed (m/s) to deal a ram hit by ploughing into a monster.
const RAM_MIN_SPEED: f32 = 4.0;
/// Reach in front of the Ogre that counts as a ram contact.
const RAM_REACH: f32 = 2.4;
/// Seconds before the same monster can be rammed again (one pass = one hit).
const RAM_CD: f32 = 0.5;

/// Which Ogre the player is currently riding (`None` = on foot). A resource
/// rather than a marker component so `player_move`, `fire_weapon` and the drive
/// system all see the change the same frame, with no command-buffer latency —
/// exactly the [`crate::vehicle::ActiveVehicle`] pattern.
#[derive(Resource, Default)]
pub struct ActiveMount(pub Option<Entity>);

/// Brief window during which a whip-staggered Ogre can be mounted. Set/refreshed
/// by a [`WhipHit`] on an Ogre and decayed each frame; the component is removed
/// when it elapses. Present only on a recently-whipped, not-yet-mounted Ogre.
#[derive(Component)]
pub struct Mountable(pub f32);

/// Marker on the Ogre while it is being ridden. Filters it out of `enemy_ai`
/// (and `assign_squad_roles`) so the AI never fights the mount driver — the
/// same one-line `Without<..>` idiom the AI already uses for `Dying`.
#[derive(Component)]
pub struct Mounted;

/// Emitted by the Whip's melee strike (`weapons::melee_strike` via `fire_weapon`)
/// when the lash connects with a monster. `apply_whip_stagger` reads it and opens
/// a [`Mountable`] window on Ogre targets.
#[derive(Message)]
pub struct WhipHit {
    pub target: Entity,
}

pub struct MountPlugin;
impl Plugin for MountPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveMount>()
            .add_message::<WhipHit>()
            .add_systems(
                Update,
                (
                    mountable_decay,
                    apply_whip_stagger,
                    mount_activate,
                    mount_drive,
                    mount_fire,
                    mount_carry,
                    mount_eject_on_death,
                )
                    .chain()
                    // Same window as the vehicle systems: after look so the carry's
                    // yaw add lands on the fresh mouse-updated heading, before move
                    // so the player resolves its own collision on the already-carried
                    // position. Also after fire_weapon so a same-frame WhipHit is
                    // consumed before the player moves on.
                    .after(crate::player::player_look)
                    .after(crate::weapons::fire_weapon)
                    .before(crate::player::player_move)
                    // Own the E press deterministically: run before vehicle_activate
                    // so a mount made this frame is visible to it (it bails when a
                    // mount is active), and a single E tap can't both mount an Ogre
                    // and board the truck.
                    .before(crate::vehicle::vehicle_activate)
                    .run_if(in_state(GameState::Playing)),
            )
            // The ridden Ogre is a LevelEntity (despawned on rebuild); drop the
            // dangling handle so a fresh level never starts phantom-mounted, on
            // death/victory/level-change.
            .add_systems(OnExit(GameState::Playing), cleanup_mount);
    }
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------
/// Horizontal unit forward vector for a heading (local -Z, matching the player
/// and the truck so the Ogre and player share one heading convention).
fn forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// Coast a forward speed toward zero by drag this frame.
fn coast(speed: f32, dt: f32) -> f32 {
    let d = OGRE_DRAG * dt;
    if speed > 0.0 {
        (speed - d).max(0.0)
    } else {
        (speed + d).min(0.0)
    }
}

// ----------------------------------------------------------------------------
// Systems
// ----------------------------------------------------------------------------
/// Tick down every `Mountable` window; remove the component once it elapses so
/// the Ogre is no longer mount-eligible.
fn mountable_decay(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Mountable)>,
) {
    let dt = time.delta_secs();
    for (e, mut m) in &mut q {
        m.0 -= dt;
        if m.0 <= 0.0 {
            commands.entity(e).remove::<Mountable>();
        }
    }
}

/// A whip lash that struck an Ogre opens (or refreshes) its mount window.
fn apply_whip_stagger(
    mut commands: Commands,
    mut whip: MessageReader<WhipHit>,
    q_enemy: Query<&Enemy, Without<Dying>>,
    mut notify: MessageWriter<Notify>,
) {
    for ev in whip.read() {
        let Ok(en) = q_enemy.get(ev.target) else { continue };
        if en.kind != MonsterKind::Ogre {
            continue;
        }
        commands.entity(ev.target).insert(Mountable(MOUNT_WINDOW));
        notify.write(Notify::new("Ogre staggered — press E to mount"));
    }
}

/// Press **E**: dismount the Ogre you're riding, or (on foot, not in a truck)
/// mount the nearest staggered Ogre in reach.
///
/// E double-handling: `vehicle_activate` also reads `just_pressed(E)`. The Ogre
/// is not a `Vehicle`, so the truck system ignores it; we only ever *mount* when
/// `ActiveVehicle` is `None`, and `vehicle_activate` only mounts when the player
/// is on a truck deck — so a single tap can't both mount and toggle a truck.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn mount_activate(
    keys: Res<ButtonInput<KeyCode>>,
    mut active: ResMut<ActiveMount>,
    vehicle: Res<crate::vehicle::ActiveVehicle>,
    mut commands: Commands,
    q_player: Query<&Transform, With<Player>>,
    q_ogre: Query<(Entity, &Transform), (With<Mountable>, With<Enemy>, Without<Dying>, Without<crate::enemies::PinnedCorpse>)>,
    mut notify: MessageWriter<Notify>,
    mut sfx: MessageWriter<Sfx>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    // Dismount the Ogre we're riding.
    if let Some(e) = active.0 {
        active.0 = None;
        commands.entity(e).remove::<Mounted>();
        notify.write(Notify::new("Dismounted"));
        return;
    }
    // Don't grab the wheel of an Ogre while driving a truck (let vehicle own E).
    if vehicle.0.is_some() {
        return;
    }
    let Ok(ptf) = q_player.single() else { return };
    let pp = ptf.translation;
    // Mount the nearest staggered Ogre within reach.
    let mut best: Option<(f32, Entity, Vec3)> = None;
    for (e, otf) in &q_ogre {
        let d = (otf.translation.x - pp.x).hypot(otf.translation.z - pp.z);
        if d <= MOUNT_REACH && best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, e, otf.translation));
        }
    }
    if let Some((_, e, opos)) = best {
        active.0 = Some(e);
        commands.entity(e).insert(Mounted);
        commands.entity(e).remove::<Mountable>();
        sfx.write(Sfx::pitched(Sound::EnemySight, opos, 0.6)); // a roar as you grab it
        notify.write(Notify::new("Mounted Ogre — W charge, A/D steer, LMB chainsaw, E to dismount"));
    }
}

/// Drive the mounted Ogre from WASD: integrate a heading + forward speed, set the
/// Ogre's `Enemy.vel` so the bone animator's walk cycle reads as *driven*, step it
/// with the same grounded `move_and_slide` block the AI uses, and advance `gait`.
///
/// Because the Ogre is excluded from `enemy_ai` while `Mounted`, this system must
/// also tick the timers the AI normally would (`pain`/`pain_cd`/`attack_cd`/
/// `atk_anim`) so the rig's flinch decays and the chainsaw swing settles.
fn mount_drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveMount>,
    colliders: Res<WorldColliders>,
    mut q_ogre: Query<(&mut Transform, &mut Enemy, &mut Knockback), With<Mounted>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Some(e) = active.0 else { return };
    let Ok((mut tf, mut en, mut kb)) = q_ogre.get_mut(e) else { return };

    // Tick the timers enemy_ai would (it's skipped for the mounted Ogre).
    en.attack_cd = (en.attack_cd - dt).max(0.0);
    en.pain = (en.pain - dt).max(0.0);
    en.pain_cd = (en.pain_cd - dt).max(0.0);
    en.atk_anim = (en.atk_anim - dt * 3.5).max(0.0);
    en.bob += dt * 3.0;

    // Drain the Ogre's Knockback the way enemy_ai does — otherwise the tank rule
    // (`combat::apply_damage` adds every redirected player-targeted hit's knockback
    // onto the mounted Ogre's `Knockback`) would bank an impulse that nothing
    // consumes while `Mounted`, then dump it all in one tick the instant `enemy_ai`
    // reacquires the Ogre on dismount — the latent-knockback slingshot guarded
    // against elsewhere (enemies.rs). The ride owns the Ogre's velocity, so we
    // simply discard the banked impulse rather than shove the steered charge with it.
    kb.0 = Vec3::ZERO;

    // Heading: start from the Ogre's own heading, then let A/D steer it (only with
    // authority while moving, like the truck). Mouse-look turns the view (the
    // carry re-syncs the player's yaw to the Ogre), so steering owns the heading.
    let mut yaw = tf.rotation.to_euler(EulerRot::YXZ).0;

    // Forward speed from the current velocity projected on the (pre-steer) heading.
    let fwd0 = forward(yaw);
    let mut fs = Vec3::new(en.vel.x, 0.0, en.vel.z).dot(fwd0);
    let mut throttle = false;
    if keys.pressed(KeyCode::KeyW) {
        fs += OGRE_ACCEL * dt;
        throttle = true;
    }
    if keys.pressed(KeyCode::KeyS) {
        fs -= OGRE_BRAKE * dt;
        throttle = true;
    }
    if !throttle {
        fs = coast(fs, dt);
    }
    fs = fs.clamp(-OGRE_MAX_REVERSE, OGRE_MAX_SPEED);

    // Steer: scales with (signed) speed; reverses when backing up like a real body.
    let mut steer = 0.0;
    if keys.pressed(KeyCode::KeyA) {
        steer += 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        steer -= 1.0;
    }
    yaw += steer * OGRE_TURN_RATE * dt * (fs / OGRE_TURN_REF).clamp(-1.0, 1.0);

    // Recompose the world velocity around the steered heading + gravity, then run
    // the exact grounded integrate block enemy_ai uses for a legged monster.
    let fwd = forward(yaw);
    let pos = tf.translation;
    en.vel.x = fwd.x * fs;
    en.vel.z = fwd.z * fs;
    en.vel.y -= GRAVITY * dt;
    let half = en.half;
    let res = move_and_slide(pos, half, en.vel, dt, &colliders.solids, STEP_HEIGHT);
    tf.translation = res.pos;
    en.vel = res.vel;
    if res.on_ground && en.vel.y < 0.0 {
        en.vel.y = 0.0;
    }
    tf.rotation = Quat::from_rotation_y(yaw);

    // Advance the walk-cycle phase by how fast we're actually moving (drives legs).
    let speed_h = Vec3::new(en.vel.x, 0.0, en.vel.z).length();
    en.gait += speed_h * dt * 2.4;
}

/// Left mouse while mounted runs the Ogre's chainsaw, and ramming is folded in
/// here too. The mounted Ogre is queried *mutably* through `With<Mounted>` (so we
/// can set its `atk_anim` for the swing animation) while targets are queried
/// through `Without<Mounted>` — disjoint, so Bevy allows both. On a swing it sets
/// `atk_anim = 1.0` (the rig plays the overhead chop) and damages monsters in the
/// forward arc; while charging fast it rams whatever it ploughs into (one hit per
/// pass via a per-target cooldown).
#[allow(clippy::too_many_arguments)]
fn mount_fire(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    active: Res<ActiveMount>,
    mut saw_cd: Local<f32>,
    mut q_ogre: Query<(&Transform, &mut Enemy), With<Mounted>>,
    mut q_targets: Query<(Entity, &Transform, &mut Enemy, &Health), Without<Mounted>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    *saw_cd = (*saw_cd - dt).max(0.0);
    // Recover per-target ram cooldowns so a monster is rammable again after RAM_CD.
    for (_, _, mut en, _) in &mut q_targets {
        if en.ram_cd > 0.0 {
            en.ram_cd = (en.ram_cd - dt).max(0.0);
        }
    }
    let Some(me) = active.0 else { return };
    let Ok((otf, mut oen)) = q_ogre.get_mut(me) else { return };
    let opos = otf.translation;
    let ohalf = oen.half;
    let ovel = oen.vel;
    let yaw = otf.rotation.to_euler(EulerRot::YXZ).0;
    let fwd = forward(yaw);

    // --- ram: ploughing into a monster at speed ----------------------------
    let planar = Vec3::new(ovel.x, 0.0, ovel.z);
    let speed = planar.length();
    if speed >= RAM_MIN_SPEED {
        let dir = planar / speed;
        for (e, etf, mut en, hp) in &mut q_targets {
            if e == me || hp.dead || en.ram_cd > 0.0 {
                continue;
            }
            let to = etf.translation - opos;
            let reach = RAM_REACH + en.half.x.max(en.half.z) + ohalf.x.max(ohalf.z);
            if Vec3::new(to.x, 0.0, to.z).length() > reach || to.y.abs() > ohalf.y + en.half.y {
                continue;
            }
            // Only ram what's roughly ahead of the charge.
            if Vec3::new(to.x, 0.0, to.z).normalize_or_zero().dot(dir) < 0.2 {
                continue;
            }
            en.ram_cd = RAM_CD;
            let amount = (speed * 4.0).clamp(20.0, 80.0);
            let knockback = dir * (speed * 1.2).clamp(8.0, 22.0) + Vec3::Y * 5.0;
            dmg.write(DamageEvent::body(e, amount, Some(me), knockback));
            sfx.write(Sfx::at(Sound::RamHit, etf.translation));
        }
    }

    // --- chainsaw swing on left mouse --------------------------------------
    if !mouse.pressed(MouseButton::Left) || *saw_cd > 0.0 {
        return;
    }
    *saw_cd = CHAINSAW_CD;
    oen.atk_anim = 1.0; // animate_monsters reads atk_anim for the overhead chop
    sfx.write(Sfx::pitched(Sound::Nailgun, opos, 0.55)); // chainsaw rev/bite

    for (e, etf, en, hp) in &q_targets {
        if e == me || hp.dead {
            continue;
        }
        let to = etf.translation - opos;
        let reach = CHAINSAW_RANGE + en.half.x.max(en.half.z);
        let planar_to = Vec3::new(to.x, 0.0, to.z);
        if planar_to.length() > reach || to.y.abs() > ohalf.y + en.half.y {
            continue;
        }
        if planar_to.normalize_or_zero().dot(fwd) < CHAINSAW_ARC_COS {
            continue; // outside the forward arc
        }
        let push = fwd * CHAINSAW_KNOCK + Vec3::Y * CHAINSAW_KNOCK * 0.3;
        dmg.write(DamageEvent::body(e, CHAINSAW_DAMAGE, Some(me), push));
    }
}

/// Snap the player onto the Ogre's shoulders each frame and turn the view with the
/// mount's heading change — the rider-carry analogue of `vehicle_carry`. Runs after
/// the drive (so the Ogre has moved) and before `player_move` (so the player's own
/// collision resolves on the carried position).
fn mount_carry(
    active: Res<ActiveMount>,
    // Previous frame's (mounted entity, Ogre yaw) so we can turn the view by the
    // Ogre's *delta* this frame rather than welding it — mirrors the truck's
    // `Vehicle.last_dyaw`. Resets when a fresh Ogre is mounted (no view jump).
    mut last: Local<Option<(Entity, f32)>>,
    q_ogre: Query<(&Transform, &Enemy), With<Mounted>>,
    mut q_player: Query<(&mut Transform, &mut Player), Without<Mounted>>,
) {
    let Some(e) = active.0 else {
        *last = None;
        return;
    };
    let Ok((otf, oen)) = q_ogre.get(e) else {
        *last = None;
        return;
    };
    let Ok((mut ptf, mut p)) = q_player.single_mut() else { return };
    // Feet rest on the Ogre's shoulders: ogre center + its half-height, plus the
    // player's own half so the player AABB sits on top.
    let saddle = oen.half.y * 0.9;
    ptf.translation = Vec3::new(
        otf.translation.x,
        otf.translation.y + saddle + PLAYER_HALF[1],
        otf.translation.z,
    );
    // The Ogre isn't a solid collider (no reserved slot, by design), so nothing
    // zeroes the player's downward velocity — `player_move` would otherwise pile
    // up gravity every frame (a huge `vel.y` that slams the player into the floor
    // on dismount). Rigidly carried means zero rider velocity.
    p.vel = Vec3::ZERO;
    p.on_ground = true;
    // Turn the view BY the mount's per-frame heading change (a delta add, like
    // `vehicle_carry`'s `p.yaw += v.last_dyaw`) instead of welding `p.yaw` to the
    // Ogre — that preserves the horizontal mouse-look `player_look` just wrote, so
    // you can still aim left/right while you ride (the chainsaw still fires along
    // the Ogre's heading; only the view is free). Wrap the diff to [-pi, pi] so a
    // heading that crosses ±pi doesn't spin the view the long way round.
    let ogre_yaw = otf.rotation.to_euler(EulerRot::YXZ).0;
    let d_yaw = match *last {
        Some((prev_e, prev_yaw)) if prev_e == e => {
            (ogre_yaw - prev_yaw).rem_euclid(std::f32::consts::TAU) // 0..TAU
        }
        // Fresh mount (or entity changed): no turn this frame.
        _ => 0.0,
    };
    let d_yaw = if d_yaw > std::f32::consts::PI { d_yaw - std::f32::consts::TAU } else { d_yaw };
    p.yaw += d_yaw;
    ptf.rotation = Quat::from_rotation_y(p.yaw);
    *last = Some((e, ogre_yaw));
}

/// End the ride when the Ogre dies: if the mounted Ogre has despawned (overkill
/// gib path), gained a `Dying` component, or its `Health` is dead, clear the mount
/// and give the player a small upward+backward impulse so the death-topple throws
/// you off. Tolerates the entity having already vanished this frame.
fn mount_eject_on_death(
    mut active: ResMut<ActiveMount>,
    q_ogre: Query<(Option<&Dying>, &Health, &Transform), With<Enemy>>,
    mut q_player: Query<(&mut Knockback, &Transform), With<Player>>,
) {
    let Some(e) = active.0 else { return };
    let eject = match q_ogre.get(e) {
        // Gone (overkill despawn) → eject.
        Err(_) => true,
        // Toppling or dead → eject.
        Ok((dying, hp, _)) => dying.is_some() || hp.dead || hp.current <= 0.0,
    };
    if !eject {
        return;
    }
    active.0 = None;
    // The Mounted marker dies with the entity (or is harmless on a Dying corpse,
    // which enemy_ai excludes anyway). Buck the player off: up + away from the fall.
    if let Ok((mut kb, ptf)) = q_player.single_mut() {
        let away = if let Ok((_, _, otf)) = q_ogre.get(e) {
            Vec3::new(ptf.translation.x - otf.translation.x, 0.0, ptf.translation.z - otf.translation.z)
                .normalize_or_zero()
        } else {
            Vec3::ZERO
        };
        kb.0 += Vec3::Y * 5.0 + away * 3.0;
    }
}

/// On leaving `Playing` (death / victory / level change): clear the mount handle
/// so a fresh level never starts phantom-mounted. The ridden Ogre is a
/// `LevelEntity` and despawns with the level, taking its `Mounted` marker with it.
fn cleanup_mount(mut active: ResMut<ActiveMount>) {
    active.0 = None;
}
