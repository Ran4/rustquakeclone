//! Monsters: spawning, perception, the AI state machine, and attacks.

use bevy::prelude::*;

use crate::common::{tune::EYE_OFFSET, *};
use crate::effects::Lifetime;
use crate::level::{MonsterKind, SpawnPlan};
use crate::monster_model::{build_monster_visual, cripple_speed, Crippled, Dying, MonsterMats, MonsterTextures};
use crate::physics::{carry_translate, ground_brush, line_of_sight, move_and_slide, ray_aabb, raycast_world, Aabb};
use crate::player::{Player, PlayerHistory};
use crate::projectiles::{spawn_projectile, ProjKind};

/// How far in the past the Grunt aims — it shoots at where the player was this
/// long ago, so moving makes the shot lag behind you and become dodgeable.
const GRUNT_AIM_LAG: f32 = 0.090;
/// Random aim error per axis (yaw + pitch), uniform in ±this many degrees.
const GRUNT_AIM_SPREAD_DEG: f32 = 5.0;

// --- Pack doctrine (squad blackboard) tuning ----------------------------------
/// Melee monsters within this distance of any current squad member join that
/// squad (greedy proximity clustering). Squads of one stay lone wolves.
const SQUAD_RADIUS: f32 = 16.0;
/// How far to the player's side a flanker's *transit* waypoint sits while it is
/// still rounding the player's front cone (it is a way-point, not a parking
/// spot — once the flanker is abreast/behind the player it closes into melee).
const FLANK_RADIUS: f32 = 6.0;
/// Raycast length used to test whether a flank bearing is wall-free.
const FLANK_PROBE: f32 = 3.0;
/// The baiter closes slower than a normal charge — the telegraph that lets the
/// player read which member is the bait.
const BAITER_SPEED_SCALE: f32 = 0.65;
/// A squadmate counts as "in melee" with the player when within
/// `melee_range + this`, used to gate when the reserve commits.
const ENGAGE_MELEE_PAD: f32 = 1.5;
/// A flanker is "abreast or behind" the player — and so should stop arcing and
/// drive straight into melee — once the cosine of its bearing off the player's
/// forward axis drops below this (≈ a 65° half-cone in front of the player).
const FLANK_CONE_COS: f32 = 0.42;
/// Keep the previous tick's baiter unless a rival is at least this much nearer
/// the player, so the bait role doesn't ping-pong between two even contenders.
const BAITER_HYSTERESIS: f32 = 0.6;

/// A thin emissive tracer line (Grunt shot telegraph / hit feedback).
fn draw_tracer(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3) {
    let mid = (a + b) * 0.5;
    let len = a.distance(b).max(0.05);
    let dir = (b - a).normalize_or_zero();
    let tf = Transform::from_translation(mid)
        .looking_to(dir, Vec3::Y)
        .with_scale(Vec3::new(0.03, 0.03, len));
    commands.spawn((
        Mesh3d(gfx.unit_cube.clone()),
        MeshMaterial3d(gfx.muzzle.clone()),
        tf,
        Lifetime(0.06),
        LevelEntity,
    ));
}

pub struct EnemiesPlugin;
impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SquadBlackboard>()
            .add_systems(
                FixedUpdate,
                // The blackboard is rebuilt every tick BEFORE the AI reads it, so
                // role assignments always reflect the live (post-death) squad.
                (assign_squad_roles, enemy_ai).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                // `apply_pins` runs AFTER `projectile_move` (which writes the PinEvent)
                // and is ordered BEFORE combat's `check_deaths` (see CombatPlugin). Those
                // two edges force sync points, so a single nail that both pins AND kills
                // a monster in one frame has `Pinned` committed and visible when the death
                // forks to the wall-décor path instead of the normal topple/ragdoll.
                (enemy_hit_flash, key_ambush, apply_pins.after(crate::projectiles::projectile_move))
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// A monster's per-tick job within its squad. Computed from the player's view
/// direction by `assign_squad_roles`; absent from the blackboard => lone wolf.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SquadRole {
    /// Advance head-on into the player's view, closing slower (the telegraph).
    Baiter,
    /// Route to the player's left side / rear, outside the facing cone.
    FlankLeft,
    /// Route to the player's right side / rear, outside the facing cone.
    FlankRight,
    /// Hold back until two squadmates are already engaged in melee.
    Reserve,
}

/// Per-tick squad role assignments, keyed by monster entity. Rebuilt every
/// FixedUpdate by `assign_squad_roles` before `enemy_ai` reads it. Absent => the
/// monster is a lone wolf (plain Chase, exactly as before this feature).
#[derive(Resource, Default)]
pub struct SquadBlackboard {
    pub roles: std::collections::HashMap<Entity, SquadRole>,
}

/// Forward (heading) vector for a player yaw — matches `player_move`'s basis.
fn forward_from_yaw(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

/// Rightward vector for a player yaw — matches `player_move`'s basis.
fn right_from_yaw(yaw: f32) -> Vec3 {
    Vec3::new(yaw.cos(), 0.0, -yaw.sin())
}

/// When the player grabs the Silver Key, the vault erupts: a reinforcement wave
/// teleports in and every monster still in the level wakes up. Classic Quake.
fn key_ambush(
    mut commands: Commands,
    mut mission: ResMut<Mission>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<MonsterTextures>,
    plan: Res<SpawnPlan>,
    mut sprung: Local<bool>,
    mut q: Query<&mut Enemy>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
) {
    if *sprung {
        // Reset the latch if the mission restarted (key cleared).
        if !mission.has_key {
            *sprung = false;
        }
        return;
    }
    if !mission.has_key {
        return;
    }
    *sprung = true;

    // Wake everything already placed.
    for mut en in &mut q {
        en.awake = true;
    }
    // Ambush wave from this level's plan (placed near the key/exit).
    for spawn in &plan.ambush {
        spawn_monster(&mut commands, &mut meshes, &mut materials, &tex, spawn.kind, spawn.pos);
    }
    if !plan.ambush.is_empty() {
        mission.total_enemies += plan.ambush.len() as u32;
        sfx.write(Sfx::global(Sound::Door));
        notify.write(Notify::new("The way erupts — the dimension awakens!"));
    }
}

/// Pop every body-part's emissive when the monster takes a hit (drives `Enemy.flash`).
fn enemy_hit_flash(
    time: Res<Time>,
    mut q: Query<(&mut Enemy, &MonsterMats)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (mut en, mats) in &mut q {
        if en.flash <= 0.0 {
            continue;
        }
        en.flash = (en.flash - dt * 6.0).max(0.0);
        let k = en.flash.clamp(0.0, 1.0);
        for (handle, base) in &mats.0 {
            if let Some(mut m) = materials.get_mut(handle) {
                m.emissive = LinearRgba::rgb(
                    base.red + 3.0 * k,
                    base.green + 3.0 * k,
                    base.blue + 3.0 * k,
                );
            }
        }
    }
}

/// Consume `PinEvent`s (feature 41): stake the target monster to the surface the
/// nail drove it against. Sets the `Pinned` state (or REFRESHES/EXTENDS it
/// additively, capped at `PIN_MAX`, when re-pinned) and snaps the rig flush to the
/// anchor. Wakes the monster so it chases the instant the root frees. Dead bodies
/// (a pinned corpse) are skipped — they're already wall décor.
pub(crate) fn apply_pins(
    mut commands: Commands,
    mut ev: MessageReader<PinEvent>,
    mut q: Query<(&mut Transform, &mut Enemy, Option<&mut Pinned>, &Health), Without<PinnedCorpse>>,
) {
    for p in ev.read() {
        let Ok((mut tf, mut en, existing, hp)) = q.get_mut(p.target) else { continue };
        if hp.dead {
            continue;
        }
        if let Some(mut pin) = existing {
            // Re-pin: additive refresh, capped so it always still counts down to 0.
            pin.until = (pin.until + p.dur).min(tune::PIN_MAX);
            pin.normal = p.normal;
            pin.anchor = p.anchor;
        } else {
            commands.entity(p.target).insert(Pinned { until: p.dur.min(tune::PIN_MAX), normal: p.normal, anchor: p.anchor });
        }
        // Snap flush to the surface and drop any momentum so it doesn't drift.
        tf.translation = p.anchor;
        en.vel = Vec3::ZERO;
        en.awake = true;
        en.state = AiState::Chase;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiState {
    Idle,
    Chase,
    Attack,
}

#[derive(Component)]
pub struct Enemy {
    pub kind: MonsterKind,
    pub vel: Vec3,
    pub state: AiState,
    pub attack_cd: f32,
    pub awake: bool,
    pub flying: bool,
    pub speed: f32,
    pub sight: f32,
    pub attack_range: f32,
    pub melee_range: f32,
    pub cd: f32,
    pub damage: f32,
    pub eye_h: f32,
    pub bob: f32,
    pub half: Vec3,
    /// Pain stagger: while >0 the monster flinches (can't move or attack).
    pub pain: f32,
    /// Cooldown gating how often pain can re-trigger.
    pub pain_cd: f32,
    /// Hit-flash intensity, decayed each frame to drive an emissive pop.
    pub flash: f32,
    /// Base material emissive, restored after a hit-flash.
    pub base_emissive: LinearRgba,
    /// Hitscan wind-up (Grunt telegraph): a pending shot resolves at <=0.
    pub windup: f32,
    /// Accumulated walk-cycle phase (advanced by horizontal speed) for the rig.
    pub gait: f32,
    /// Attack animation envelope (1 at strike, decays to 0) driving the weapon arm.
    pub atk_anim: f32,
    /// Cooldown gating how often a vehicle ram can re-hit this monster (so one
    /// pass-through deals a single hit, not one per frame of overlap).
    pub ram_cd: f32,
}

/// Nail-pin state (feature 41): a monster staked to a surface. While present the
/// AI is fully rooted — movement zeroed, Chase/Attack overridden — but the monster
/// is still alive and takes damage normally. `until` ALWAYS counts down to 0 and
/// the component is removed, so the AI can never deadlock; a fresh nail refreshes
/// it (capped at `PIN_MAX`). `anchor` is the snapped body centre it's held at.
#[derive(Component)]
pub struct Pinned {
    /// Seconds of root left (counts down in `enemy_ai`; <=0 => component removed).
    pub until: f32,
    /// Surface normal of the wall it's pinned to (points back toward the monster).
    pub normal: Vec3,
    /// Snapped body-centre position the rig is held flush at.
    pub anchor: Vec3,
}

/// A monster that DIED while pinned (feature 41): it stays staked to the surface as
/// grisly wall décor instead of toppling into a ragdoll. The marker freezes the rig
/// (`animate_monsters` leaves its bones be) and excludes it from the live AI.
#[derive(Component)]
pub struct PinnedCorpse;

struct MStats {
    health: f32,
    speed: f32,
    sight: f32,
    attack_range: f32,
    melee_range: f32,
    cd: f32,
    damage: f32,
    half: Vec3,
    flying: bool,
    color: Color,
    emissive: LinearRgba,
}

fn stats(kind: MonsterKind) -> MStats {
    use MonsterKind::*;
    match kind {
        Grunt => MStats { health: 30.0, speed: 3.4, sight: 30.0, attack_range: 24.0, melee_range: 0.0, cd: 1.3, damage: 9.0, half: Vec3::new(0.4, 0.9, 0.4), flying: false, color: rgb(0.45, 0.36, 0.28), emissive: LinearRgba::BLACK },
        Enforcer => MStats { health: 55.0, speed: 3.1, sight: 32.0, attack_range: 26.0, melee_range: 0.0, cd: 1.5, damage: 10.0, half: Vec3::new(0.45, 0.95, 0.45), flying: false, color: rgb(0.32, 0.4, 0.5), emissive: LinearRgba::rgb(0.0, 0.05, 0.2) },
        Knight => MStats { health: 60.0, speed: 7.2, sight: 28.0, attack_range: 2.2, melee_range: 2.4, cd: 0.85, damage: 16.0, half: Vec3::new(0.4, 0.9, 0.4), flying: false, color: rgb(0.55, 0.56, 0.6), emissive: LinearRgba::BLACK },
        Scrag => MStats { health: 45.0, speed: 4.0, sight: 32.0, attack_range: 28.0, melee_range: 0.0, cd: 1.4, damage: 10.0, half: Vec3::new(0.5, 0.7, 0.5), flying: true, color: rgb(0.3, 0.55, 0.3), emissive: LinearRgba::rgb(0.05, 0.3, 0.05) },
        Ogre => MStats { health: 200.0, speed: 2.7, sight: 28.0, attack_range: 22.0, melee_range: 2.8, cd: 1.9, damage: 22.0, half: Vec3::new(0.6, 1.1, 0.6), flying: false, color: rgb(0.4, 0.3, 0.22), emissive: LinearRgba::BLACK },
        DeathKnight => MStats { health: 350.0, speed: 3.4, sight: 32.0, attack_range: 26.0, melee_range: 3.0, cd: 1.6, damage: 22.0, half: Vec3::new(0.6, 1.2, 0.6), flying: false, color: rgb(0.4, 0.12, 0.14), emissive: LinearRgba::rgb(0.4, 0.0, 0.05) },
        // Low, wide spider: a ranged venom-spitter that anchors a silk strand.
        Weaver => MStats { health: 50.0, speed: 4.2, sight: 30.0, attack_range: 24.0, melee_range: 0.0, cd: 1.3, damage: 9.0, half: Vec3::new(0.6, 0.5, 0.6), flying: false, color: rgb(0.14, 0.12, 0.16), emissive: LinearRgba::rgb(0.05, 0.2, 0.05) },
    }
}

fn sight_sound(_kind: MonsterKind) -> Sound {
    Sound::EnemySight
}

/// Voice pitch per kind so the bestiary sounds distinct (big = deep).
pub fn kind_pitch(kind: MonsterKind) -> f32 {
    use MonsterKind::*;
    match kind {
        Ogre | DeathKnight => 0.7,
        Scrag => 1.45,
        Weaver => 1.3, // chittery, skittering voice
        Knight => 1.15,
        _ => 1.0,
    }
}

/// Spawn one monster of `kind` at `pos` (feet just above pos so it settles).
pub fn spawn_monster(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tex: &MonsterTextures,
    kind: MonsterKind,
    pos: Vec3,
) {
    let s = stats(kind);
    let eye_h = s.half.y * 0.6;
    // The root is an invisible transform carrying the AI/physics; the visible,
    // articulated, textured model is built as a bone hierarchy beneath it.
    let root = commands
        .spawn((
            Transform::from_translation(pos + Vec3::Y * s.half.y),
            Visibility::Visible,
            Enemy {
                kind,
                vel: Vec3::ZERO,
                state: AiState::Idle,
                attack_cd: 0.5,
                awake: false,
                flying: s.flying,
                speed: s.speed,
                sight: s.sight,
                attack_range: s.attack_range,
                melee_range: s.melee_range,
                cd: s.cd,
                damage: s.damage,
                eye_h,
                bob: 0.0,
                half: s.half,
                pain: 0.0,
                pain_cd: 0.0,
                flash: 0.0,
                base_emissive: s.emissive,
                windup: 0.0,
                gait: 0.0,
                atk_anim: 0.0,
                ram_cd: 0.0,
            },
            Health::new(s.health),
            Faction::Monster,
            Hurtbox { half: s.half },
            Knockback::default(),
            LevelEntity,
            Name::new(format!("{kind:?}")),
        ))
        .id();
    let (mats, limbs) = build_monster_visual(commands, meshes, materials, tex, kind, root);
    commands.entity(root).insert((MonsterMats(mats), limbs, Crippled::default()));
}

/// Spawn every monster from the level's SpawnPlan. Runs in the OnEnter chain.
pub fn spawn_monsters(
    mut commands: Commands,
    plan: Res<SpawnPlan>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    tex: Res<MonsterTextures>,
    mut mission: ResMut<Mission>,
) {
    mission.total_enemies = plan.monsters.len() as u32;
    for spawn in &plan.monsters {
        spawn_monster(&mut commands, &mut meshes, &mut materials, &tex, spawn.kind, spawn.pos);
    }
}

/// A melee monster considered for squad roles this tick. Pulled out of the ECS
/// so the clustering + role math is a pure, unit-testable function.
struct SquadMember {
    e: Entity,
    pos: Vec3,
    melee_range: f32,
    has_los: bool,
}

/// Rebuild the squad blackboard for this tick: cluster awake melee monsters by
/// proximity and hand each clustered squad (size >= 2) a role. Lone monsters and
/// ranged monsters (melee_range == 0) are left out of the map entirely, so they
/// keep their unchanged solo Idle->Chase->Attack behaviour.
#[allow(clippy::type_complexity)]
fn assign_squad_roles(
    colliders: Res<WorldColliders>,
    q_player: Query<(&Transform, &Player)>,
    q_mon: Query<(Entity, &Transform, &Enemy), (Without<Player>, Without<Dying>, Without<PinnedCorpse>, Without<crate::mount::Mounted>)>,
    mut blackboard: ResMut<SquadBlackboard>,
) {
    let Ok((player_tf, player)) = q_player.single() else {
        blackboard.roles.clear();
        return;
    };
    let player_pos = player_tf.translation;
    let player_eye = player_pos + Vec3::Y * EYE_OFFSET;

    // Candidate melee members: awake, alive (Dying excluded by the query), and
    // actually melee (ranged casters have melee_range == 0 and never get a role).
    let mut members: Vec<SquadMember> = Vec::new();
    for (e, tf, en) in &q_mon {
        if !en.awake || en.melee_range <= 0.0 {
            continue;
        }
        let pos = tf.translation;
        let eye = pos + Vec3::Y * en.eye_h;
        members.push(SquadMember {
            e,
            pos,
            melee_range: en.melee_range,
            has_los: line_of_sight(eye, player_eye, &colliders.solids),
        });
    }

    // Recompute roles into a fresh map (read the previous one first, for the bait
    // + reserve hysteresis below), then swap it in. The blackboard is otherwise
    // stateless per tick, so dead members drop out automatically.
    let next = compute_squad_roles(&members, player_pos, right_from_yaw(player.yaw), &blackboard.roles);
    blackboard.roles = next;
}

/// Pure squad solver: cluster `members` by proximity and assign each clustered
/// squad (size >= 2) a role. `prev` is last tick's assignment, used only for
/// light hysteresis (sticky baiter, banded reserve release) so roles don't flip
/// frame-to-frame. Returns the new role map keyed by entity; members absent from
/// it are lone wolves and chase exactly as before this feature.
fn compute_squad_roles(
    members: &[SquadMember],
    player_pos: Vec3,
    player_right: Vec3,
    prev: &std::collections::HashMap<Entity, SquadRole>,
) -> std::collections::HashMap<Entity, SquadRole> {
    let mut roles = std::collections::HashMap::new();
    let n = members.len();
    if n < 2 {
        return roles; // nobody can have a squadmate
    }

    // Greedy proximity clustering: members within SQUAD_RADIUS of any current
    // squad member join it. n is tiny so the O(n^2) flood is free.
    let r2 = SQUAD_RADIUS * SQUAD_RADIUS;
    let mut squad_of = vec![usize::MAX; n]; // cluster id per member
    let mut squads: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        if squad_of[i] != usize::MAX {
            continue;
        }
        let id = squads.len();
        let mut stack = vec![i];
        squad_of[i] = id;
        let mut cluster = Vec::new();
        while let Some(cur) = stack.pop() {
            cluster.push(cur);
            for j in 0..n {
                if squad_of[j] == usize::MAX && members[cur].pos.distance_squared(members[j].pos) <= r2 {
                    squad_of[j] = id;
                    stack.push(j);
                }
            }
        }
        squads.push(cluster);
    }

    for squad in &squads {
        if squad.len() < 2 {
            continue; // lone wolf — no role, unchanged solo behaviour
        }

        // How many squadmates are already pressing the player in melee — gates
        // whether the reserve commits this tick (baked here so enemy_ai never
        // needs the engaged count).
        let engaged = squad
            .iter()
            .filter(|&&m| {
                members[m].pos.distance(player_pos) <= members[m].melee_range + ENGAGE_MELEE_PAD
            })
            .count();

        // Baiter: the member that holds the player's gaze — prefer one with clear
        // LoS, and among those (or all, if none has LoS) the one nearest the player.
        let key = |m: usize| {
            let los_rank = if members[m].has_los { 0 } else { 1 };
            (los_rank, members[m].pos.distance(player_pos))
        };
        let mut baiter = squad
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let (la, da) = key(a);
                let (lb, db) = key(b);
                la.cmp(&lb).then(da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal))
            })
            .unwrap();
        // Hysteresis: stick with last tick's baiter unless the challenger beats it
        // on LoS rank or is meaningfully (BAITER_HYSTERESIS) nearer — otherwise two
        // near-equal contenders trade the bait role (and its 0.65x gait) each tick.
        if let Some(&prev_baiter) = squad.iter().find(|&&m| prev.get(&members[m].e) == Some(&SquadRole::Baiter)) {
            let (lp, dp) = key(prev_baiter);
            let (lb, db) = key(baiter);
            if lp <= lb && dp <= db + BAITER_HYSTERESIS {
                baiter = prev_baiter;
            }
        }
        roles.insert(members[baiter].e, SquadRole::Baiter);

        // Non-baiters, farthest-from-player first, so reserves (if any) come off
        // the rear of the squad rather than its front.
        let mut rest: Vec<usize> = squad.iter().copied().filter(|&m| m != baiter).collect();
        rest.sort_by(|&a, &b| {
            members[b]
                .pos
                .distance(player_pos)
                .partial_cmp(&members[a].pos.distance(player_pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Reserve release with an enter/exit band so a squadmate dithering at the
        // melee edge doesn't toggle the rear member between full-stop and full-
        // advance: commit reserves once two are engaged, and only RE-impose the
        // hold once the squad falls back below one engaged. Mid-band, keep whatever
        // we did last tick (any non-Reserve role in `prev` => stay committed).
        let was_committed = squad
            .iter()
            .any(|&m| matches!(prev.get(&members[m].e), Some(r) if *r != SquadRole::Reserve));
        let committed = engaged >= 2 || (engaged >= 1 && was_committed);
        let max_reserves = squad.len().saturating_sub(2);
        let reserves = if committed { 0 } else { max_reserves };
        for (idx, &m) in rest.iter().enumerate() {
            let role = if idx < reserves {
                SquadRole::Reserve
            } else {
                // Flank toward whichever side of the player's facing this member
                // already sits on, so it stays outside the front cone.
                let to_member = members[m].pos - player_pos;
                if to_member.dot(player_right) >= 0.0 {
                    SquadRole::FlankRight
                } else {
                    SquadRole::FlankLeft
                }
            };
            roles.insert(members[m].e, role);
        }
    }
    roles
}

/// The desired horizontal bearing for a flanker. Two phases (a transit waypoint,
/// not a parking spot): while the flanker is still inside the player's front cone
/// it arcs to a point genuinely behind-and-to-its-assigned-side so it rounds the
/// player rather than crossing their view; once it is abreast/behind the player it
/// drives straight in so it actually reaches melee (where the Attack state takes
/// over). `flank_right` picks the side; `to_player_h` is the unit vector toward
/// the player. Returns a horizontal unit bearing (caller still wall-probes it).
fn flank_bearing(
    flanker_pos: Vec3,
    player_pos: Vec3,
    player_fwd: Vec3,
    player_right: Vec3,
    flank_right: bool,
    melee_range: f32,
    to_player_h: Vec3,
) -> Vec3 {
    let s = if flank_right { 1.0 } else { -1.0 };
    // How far off the player's forward axis the flanker currently sits: cos of the
    // angle between "player -> flanker" and the player's facing.
    let to_flanker = flanker_pos - player_pos;
    let off_axis_cos = to_flanker.normalize_or_zero().dot(player_fwd);
    if off_axis_cos <= FLANK_CONE_COS {
        // Abreast or behind the player already — close straight into melee from the
        // side it has reached (no more orbiting at a fixed standoff).
        return to_player_h;
    }
    // Still in front: aim for a point behind-and-to-the-side so the path arcs
    // around the player. The waypoint sits inside melee range along the player's
    // forward (behind), so on arrival the flanker is already in attack reach.
    let behind = (melee_range * 0.8).max(0.5);
    let target = player_pos + player_right * (s * FLANK_RADIUS) - player_fwd * behind;
    let to_t = Vec3::new(target.x - flanker_pos.x, 0.0, target.z - flanker_pos.z);
    to_t.normalize_or_zero()
}

/// Pick a horizontal bearing for a flanker that doesn't walk it into a wall.
/// Probes `primary` with two rays offset by ±the body half-width (so a lane that
/// merely grazes a corner counts as blocked); if clear, take it. Otherwise try
/// the mirrored flank (`mirror`) — the other side may be open in an asymmetric
/// room — and only then fall back to `fallback` (straight at the player) so a
/// boxed-in flanker never freezes. Bearings are horizontal unit vectors.
fn pick_walkfree_bearing(pos: Vec3, half: Vec3, primary: Vec3, mirror: Vec3, fallback: Vec3, solids: &[Aabb]) -> Vec3 {
    let probe_origin = pos + Vec3::Y * (half.y * 0.6); // body mid-height
    let clear = |dir: Vec3| -> bool {
        if dir == Vec3::ZERO {
            return false;
        }
        // Two parallel rays offset by the body half-width, so a wall the body
        // would scrape (but a single centre ray would miss) still reads as blocked.
        let perp = Vec3::new(-dir.z, 0.0, dir.x).normalize_or_zero() * half.x.max(0.05);
        for o in [probe_origin + perp, probe_origin - perp] {
            match raycast_world(o, dir, FLANK_PROBE, solids) {
                Some((t, _, _)) if t < FLANK_PROBE - 1e-3 => return false,
                _ => {}
            }
        }
        true
    };
    if clear(primary) {
        primary
    } else if clear(mirror) {
        mirror
    } else {
        fallback // direct chase whether or not it's clear — never freeze
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn enemy_ai(
    time: Res<Time>,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
    history: Res<PlayerHistory>,
    blackboard: Res<SquadBlackboard>,
    mut commands: Commands,
    mut rng_state: Local<u32>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Knockback, Option<&Crippled>, Option<&mut Pinned>), (Without<Player>, Without<Dying>, Without<PinnedCorpse>, Without<crate::mount::Mounted>)>,
    q_player: Query<(Entity, &Transform, &Player)>,
    mut dmg: MessageWriter<DamageEvent>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let now = time.elapsed_secs();
    let Ok((player_e, player_tf, player)) = q_player.single() else { return };
    let player_pos = player_tf.translation;
    let player_eye = player_pos + Vec3::Y * EYE_OFFSET;
    let player_yaw = player.yaw;

    if *rng_state == 0 {
        *rng_state = 0x9e37_79b9;
    }
    let mut rand = || {
        let mut x = *rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *rng_state = x;
        x as f32 / u32::MAX as f32
    };

    for (e, mut tf, mut en, mut kb, crippled, pinned) in &mut q {
        en.attack_cd = (en.attack_cd - dt).max(0.0);
        en.pain = (en.pain - dt).max(0.0);
        en.pain_cd = (en.pain_cd - dt).max(0.0);
        en.atk_anim = (en.atk_anim - dt * 3.5).max(0.0);
        en.bob += dt * 3.0;

        // Cripples: lost legs slow movement, a blown wing grounds a flyer, a
        // severed weapon arm disarms (gated in the attack block below).
        let cr = crippled.copied().unwrap_or_default();
        let leg_scale = cripple_speed(cr.legs_lost);
        if cr.grounded && en.flying {
            en.flying = false; // wing gone — it falls and drags
        }

        let pos = tf.translation;
        let eye = pos + Vec3::Y * en.eye_h;
        let to_player = player_pos - pos;
        let dist = to_player.length();
        let los = line_of_sight(eye, player_eye, &colliders.solids);

        // Consume knockback (push/blast/implode-pull) once per tick, awake or not —
        // so a sleeping monster is still dragged by a Lodestone well and never banks
        // a latent impulse to dump as a slingshot the instant it wakes.
        if kb.0 != Vec3::ZERO {
            en.vel += kb.0;
            kb.0 = Vec3::ZERO;
        }

        // Pinned (feature 41): staked to a surface. The timer ALWAYS counts down to
        // 0 and frees it (never re-armed here — only `apply_pins` extends it), so the
        // AI can't deadlock even if the anchor brush moves or vanishes. While rooted
        // we hold it flush at the anchor, drop any banked knockback (don't let a blast
        // fling it off the stake), and skip the rest of the AI so it can't move or
        // attack — but keep `pain` topped up so the rig keeps twitching against the
        // wall (it reads as struggling, not frozen).
        if let Some(mut pin) = pinned {
            pin.until -= dt;
            if pin.until <= 0.0 {
                commands.entity(e).remove::<Pinned>();
            } else {
                tf.translation = pin.anchor;
                en.vel = Vec3::ZERO;
                kb.0 = Vec3::ZERO;
                // Drive an actual oscillation (not a constant lean) so the rig visibly
                // thrashes to tear free. `pain` feeds the animator's torso lean, so a
                // fast sine on it reads as a struggle; the per-entity phase (`bob`)
                // keeps a clump of pinned bodies from twitching in lockstep.
                en.pain = (0.10 + 0.06 * (now * 13.0 + en.bob).sin()).max(0.0);
                en.windup = 0.0; // never resolve a telegraphed shot while staked
                continue;
            }
        }

        // Wake up when the player is seen.
        if !en.awake {
            if dist < en.sight && los {
                en.awake = true;
                en.state = AiState::Chase;
                sfx.write(Sfx::pitched(sight_sound(en.kind), pos, kind_pitch(en.kind)));
            } else {
                // idle: apply gravity so they rest on the floor, and drift + bleed any
                // knockback velocity (e.g. a well's pull) so they slide in and settle.
                settle(&mut tf, &mut en, dt, &colliders.solids);
                continue;
            }
        }

        // Face the player (yaw only).
        if to_player.x.abs() + to_player.z.abs() > 0.01 {
            let yaw = to_player.x.atan2(to_player.z) + std::f32::consts::PI;
            tf.rotation = Quat::from_rotation_y(yaw);
        }

        // Resolve a pending Grunt hitscan once the wind-up telegraph elapses.
        // The Grunt aims where the player *was* ~250ms ago (so moving lets the
        // shot lag behind you) with a small random spread, and only connects if
        // that ray truly strikes the player — no more guaranteed hitscan.
        if en.windup > 0.0 {
            en.windup -= dt;
            if en.windup <= 0.0 {
                let aim = history.position_ago(now, GRUNT_AIM_LAG).unwrap_or(player_eye);
                resolve_grunt_shot(
                    &mut commands, &gfx, &colliders.solids, e, player_e, eye, player_pos,
                    aim, en.damage, &mut dmg, &mut sfx, &mut rand,
                );
            }
        }

        let staggered = en.pain > 0.0 || en.windup > 0.0;
        let in_attack = (dist <= en.attack_range || dist <= en.melee_range) && los;
        let needs_approach = en.melee_range > 0.0 && dist > en.melee_range * 0.9;

        // Decide desired horizontal movement (flinching/aiming monsters hold still).
        let mut wish = Vec3::ZERO;
        if staggered {
            // hold position while flinching or telegraphing a shot
        } else if !in_attack || needs_approach {
            let dir = if en.flying { to_player } else { Vec3::new(to_player.x, 0.0, to_player.z) };
            wish = dir.normalize_or_zero() * en.speed * leg_scale;
            en.state = AiState::Chase;

            // Pack doctrine: a clustered melee monster steers its chase by the
            // role the blackboard handed it this tick. Lone wolves and ranged
            // casters never appear in the map and so chase exactly as before.
            if !en.flying {
                if let Some(role) = blackboard.roles.get(&e).copied() {
                    let to_player_h = Vec3::new(to_player.x, 0.0, to_player.z).normalize_or_zero();
                    match role {
                        SquadRole::Baiter => {
                            // Straight head-on, but slower (the telegraph).
                            wish = to_player_h * en.speed * leg_scale * BAITER_SPEED_SCALE;
                        }
                        SquadRole::FlankLeft | SquadRole::FlankRight => {
                            let pf = forward_from_yaw(player_yaw);
                            let pr = right_from_yaw(player_yaw);
                            let flank_right = role == SquadRole::FlankRight;
                            // Arc around the player's front cone, then close into
                            // melee once abreast/behind (a transit waypoint, not a
                            // fixed standoff).
                            let primary = flank_bearing(
                                pos, player_pos, pf, pr, flank_right, en.melee_range, to_player_h,
                            );
                            // If that lane is walled, the mirrored side may be open.
                            let mirror = flank_bearing(
                                pos, player_pos, pf, pr, !flank_right, en.melee_range, to_player_h,
                            );
                            let bearing = pick_walkfree_bearing(pos, en.half, primary, mirror, to_player_h, &colliders.solids);
                            wish = bearing * en.speed * leg_scale;
                        }
                        SquadRole::Reserve => {
                            // Hold back (still faces the player, but doesn't advance).
                            wish = Vec3::ZERO;
                        }
                    }
                }
            }
        } else {
            en.state = AiState::Attack;
            let strafe = Vec3::new(-to_player.z, 0.0, to_player.x).normalize_or_zero();
            wish = strafe * (en.speed * 0.4 * leg_scale) * if rand() > 0.5 { 1.0 } else { -1.0 };
        }

        // Initiate an attack if able (Grunt telegraphs via a wind-up first).
        if !staggered && in_attack && en.attack_cd <= 0.0 && dist > 0.3 {
            en.attack_cd = en.cd;
            en.atk_anim = 1.0; // still flails menacingly even when disarmed
            if en.kind == MonsterKind::Grunt {
                if !cr.disarmed {
                    en.windup = 0.35;
                    en.flash = en.flash.max(0.5); // brief "aim" glint
                }
            } else {
                do_attack(&mut en, &mut commands, &gfx, e, player_e, eye, player_eye, dist, cr.disarmed, cr.head_gone, &mut dmg, &mut sfx, &mut rand);
            }
        }

        // Integrate movement.
        if en.flying {
            // Hover toward a point near the player, no gravity.
            let mut v = wish;
            // keep some altitude relative to player
            let target_y = player_pos.y + 1.5 + (en.bob).sin() * 0.4;
            v.y += (target_y - pos.y) * 2.0;
            en.vel = en.vel.lerp(v, (dt * 4.0).min(1.0));
            let res = move_and_slide(pos, en.half, en.vel, dt, &colliders.solids, 0.0);
            tf.translation = res.pos;
            en.vel = res.vel;
        } else {
            // ground: accelerate horizontally toward wish, apply gravity.
            // Floor material (feature 47): the AI moves on the same probe as the
            // player, so it inherits the same slip/drag — a Knight lured onto tar
            // wades, on ice it overshoots. We scale ONLY accel_mul (the lerp rate
            // toward `wish` AND the speed it converges on); accel_mul is clamped at
            // 0.35 (ice) / 0.5 (tar), never 0, so a sticky floor can slow the AI to a
            // wade but can never bleed it to a standstill and freeze it. friction_mul
            // is deliberately NOT applied: the enemy has no separate friction term
            // (the lerp toward a smaller wish is its only deceleration), and tar's
            // >1 friction would perversely make it stop FASTER, the opposite of drag.
            let half = en.half;
            let fmat = if colliders.has_floor_material {
                ground_brush(pos, half, &colliders.solids)
                    .map(|i| colliders.material(i))
                    .unwrap_or_default()
            } else {
                FloorMaterial::Normal
            };
            let m = fmat.accel_mul();
            let cur_h = Vec3::new(en.vel.x, 0.0, en.vel.z);
            let new_h = cur_h.lerp(wish * m, (dt * 8.0 * m).min(1.0));
            en.vel.x = new_h.x;
            en.vel.z = new_h.z;
            en.vel.y -= tune::GRAVITY * dt;
            let res = move_and_slide(pos, half, en.vel, dt, &colliders.solids, tune::STEP_HEIGHT);
            tf.translation = res.pos;
            en.vel = res.vel;
            if res.on_ground && en.vel.y < 0.0 {
                en.vel.y = 0.0;
            }
            // Conveyor floors carry monsters too — a belt over a lava channel slides
            // them along just like the player (the same `vehicle_carry` delta, routed
            // through the swept slide so a wall still stops them).
            if res.on_ground {
                let belt = fmat.push();
                if belt != Vec3::ZERO {
                    tf.translation = carry_translate(tf.translation, half, belt * dt, &colliders.solids);
                }
            }
        }

        // Advance the walk-cycle phase by how fast we're actually moving.
        let speed_h = Vec3::new(en.vel.x, 0.0, en.vel.z).length();
        en.gait += speed_h * dt * 2.4;
    }
}

/// Idle gravity settle so sleeping monsters sit on the ground. Any horizontal
/// velocity (e.g. a Lodestone well's pull, consumed before this is called) carries
/// them along and is bled off by friction so they drift in and come to rest rather
/// than sliding forever — and don't hoard a knockback to slingshot on waking.
fn settle(tf: &mut Transform, en: &mut Enemy, dt: f32, solids: &[crate::physics::Aabb]) {
    if en.flying {
        return;
    }
    // Bleed horizontal drift toward a stop (mirrors the player's ground friction).
    let horiz = Vec3::new(en.vel.x, 0.0, en.vel.z);
    let speed = horiz.length();
    if speed > 1e-4 {
        let drop = speed.max(1.0) * tune::FRICTION * dt;
        let scale = (speed - drop).max(0.0) / speed;
        en.vel.x *= scale;
        en.vel.z *= scale;
    }
    en.vel.y -= tune::GRAVITY * dt;
    let res = move_and_slide(tf.translation, en.half, en.vel, dt, solids, tune::STEP_HEIGHT);
    tf.translation = res.pos;
    en.vel = res.vel;
    if res.on_ground && en.vel.y < 0.0 {
        en.vel.y = 0.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn do_attack(
    en: &mut Enemy,
    commands: &mut Commands,
    gfx: &GfxAssets,
    self_e: Entity,
    player_e: Entity,
    eye: Vec3,
    player_eye: Vec3,
    dist: f32,
    disarmed: bool,
    head_gone: bool,
    dmg: &mut MessageWriter<DamageEvent>,
    sfx: &mut MessageWriter<Sfx>,
    rand: &mut impl FnMut() -> f32,
) {
    use MonsterKind::*;
    let mut to = (player_eye - eye).normalize_or_zero();
    if to == Vec3::ZERO {
        to = Vec3::NEG_Z;
    }
    // A blown-off head wrecks aim (bosses only — normals die on head sever).
    if head_gone {
        let j = 0.17; // ~10 degrees of extra scatter
        to = (to + Vec3::new((rand() * 2.0 - 1.0) * j, (rand() * 2.0 - 1.0) * j, (rand() * 2.0 - 1.0) * j)).normalize_or_zero();
    }
    let muzzle = eye + to * 0.6;
    // A severed weapon arm still swings/sounds (reads as "fighting around the
    // hole") but deals no damage — every damaging line below is gated on !disarmed.
    match en.kind {
        Grunt => {
            sfx.write(Sfx::at(Sound::Shotgun, eye));
            if !disarmed && rand() < 0.6 {
                dmg.write(DamageEvent::body(player_e, en.damage, Some(self_e), to * 2.0));
            }
        }
        Enforcer | Scrag => {
            sfx.write(Sfx::at(Sound::Nailgun, eye));
            if !disarmed {
                spawn_projectile(commands, gfx, ProjKind::Bolt, muzzle, to, false, Some(self_e));
            }
        }
        Weaver => {
            // A venom spit — a single bolt, like the other ranged casters.
            sfx.write(Sfx::at(Sound::Nailgun, eye));
            if !disarmed {
                spawn_projectile(commands, gfx, ProjKind::Bolt, muzzle, to, false, Some(self_e));
            }
        }
        Ogre => {
            if dist <= en.melee_range {
                sfx.write(Sfx::at(Sound::EnemyPain, eye));
                if !disarmed {
                    dmg.write(DamageEvent::body(player_e, en.damage, Some(self_e), to * 4.0));
                }
            } else {
                sfx.write(Sfx::at(Sound::GrenadeFire, eye));
                if !disarmed {
                    let lob = (to + Vec3::Y * 0.35).normalize_or_zero();
                    spawn_projectile(commands, gfx, ProjKind::Grenade, muzzle, lob, false, Some(self_e));
                }
            }
        }
        Knight => {
            sfx.write(Sfx::at(Sound::EnemyPain, eye));
            if !disarmed {
                dmg.write(DamageEvent::body(player_e, en.damage, Some(self_e), to * 3.0));
            }
        }
        DeathKnight => {
            if dist <= en.melee_range {
                if !disarmed {
                    dmg.write(DamageEvent::body(player_e, en.damage, Some(self_e), to * 4.0));
                }
                sfx.write(Sfx::at(Sound::EnemyPain, eye));
            } else {
                sfx.write(Sfx::at(Sound::RocketFire, eye));
                // fan of three bolts
                if !disarmed {
                    for off in [-0.12f32, 0.0, 0.12] {
                        let d = (to + Vec3::new(off, 0.0, 0.0)).normalize_or_zero();
                        spawn_projectile(commands, gfx, ProjKind::Bolt, muzzle, d, false, Some(self_e));
                    }
                }
            }
        }
    }
}

/// Resolve a Grunt's hitscan. Unlike a perfect tracker, it aims at `aim` (where
/// the player was ~250ms ago), perturbs that aim by a uniform ±4° in yaw and
/// pitch, then traces the shot: it deals damage only if the spread ray strikes
/// the player's hurtbox before any wall. The tracer is drawn along the actual
/// shot so a miss is visible whizzing past.
#[allow(clippy::too_many_arguments)]
fn resolve_grunt_shot(
    commands: &mut Commands,
    gfx: &GfxAssets,
    solids: &[Aabb],
    self_e: Entity,
    player_e: Entity,
    eye: Vec3,
    player_pos: Vec3,
    aim: Vec3,
    damage: f32,
    dmg: &mut MessageWriter<DamageEvent>,
    sfx: &mut MessageWriter<Sfx>,
    rand: &mut impl FnMut() -> f32,
) {
    const MAX_RANGE: f32 = 100.0;
    sfx.write(Sfx::at(Sound::Shotgun, eye));

    // Base direction toward the lagged aim point.
    let mut base = (aim - eye).normalize_or_zero();
    if base == Vec3::ZERO {
        base = Vec3::NEG_Z;
    }
    // Apply a uniform ±spread jitter in yaw (left/right) and pitch (up/down).
    let spread = GRUNT_AIM_SPREAD_DEG.to_radians();
    let yaw = (rand() * 2.0 - 1.0) * spread;
    let pitch = (rand() * 2.0 - 1.0) * spread;
    let right = {
        let r = base.cross(Vec3::Y).normalize_or_zero();
        if r == Vec3::ZERO { Vec3::X } else { r }
    };
    let dir = (Quat::from_axis_angle(Vec3::Y, yaw)
        * Quat::from_axis_angle(right, pitch)
        * base)
        .normalize_or_zero();

    // Nearest wall along the shot, and whether the player is struck before it.
    let wall_t = raycast_world(eye, dir, MAX_RANGE, solids).map(|(t, _, _)| t);
    let player_aabb = Aabb::from_center_half(player_pos, Vec3::from_array(tune::PLAYER_HALF));
    let hit_t = ray_aabb(eye, dir, MAX_RANGE, &player_aabb)
        .map(|(t, _)| t)
        .filter(|&t| wall_t.map_or(true, |w| t <= w));

    let end_t = hit_t.or(wall_t).unwrap_or(MAX_RANGE);
    draw_tracer(commands, gfx, eye, eye + dir * end_t);

    if hit_t.is_some() {
        dmg.write(DamageEvent::body(player_e, damage, Some(self_e), dir * 2.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ent(i: u32) -> Entity {
        Entity::from_raw_u32(i).unwrap()
    }

    /// Build a Knight-like squad member at `pos` (melee_range 2.4, LoS clear).
    fn mem(i: u32, pos: Vec3) -> SquadMember {
        SquadMember { e: ent(i), pos, melee_range: 2.4, has_los: true }
    }

    // Player faces -Z (yaw 0): forward = (0,0,-1), right = (1,0,0). Player at origin.
    const PR: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    const PF: Vec3 = Vec3::new(0.0, 0.0, -1.0);

    #[test]
    fn lone_or_ranged_get_no_roles() {
        let none = HashMap::new();
        // A single member never has a squadmate.
        let one = [mem(1, Vec3::new(0.0, 0.0, -10.0))];
        assert!(compute_squad_roles(&one, Vec3::ZERO, PR, &none).is_empty());
    }

    #[test]
    fn pair_yields_one_baiter_and_one_flanker() {
        let none = HashMap::new();
        // Two melee members in front of the player, both within SQUAD_RADIUS.
        let m = [
            mem(1, Vec3::new(-2.0, 0.0, -8.0)), // farther
            mem(2, Vec3::new(1.0, 0.0, -5.0)),  // nearer -> baiter
        ];
        let roles = compute_squad_roles(&m, Vec3::ZERO, PR, &none);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[&ent(2)], SquadRole::Baiter, "nearest-with-LoS is the baiter");
        // The other is a flanker (left, since it sits on the player's -x side).
        assert_eq!(roles[&ent(1)], SquadRole::FlankLeft);
    }

    #[test]
    fn baiter_prefers_los_over_nearer_blind_member() {
        let none = HashMap::new();
        let mut m = [
            mem(1, Vec3::new(0.0, 0.0, -3.0)), // nearer but blind
            mem(2, Vec3::new(0.0, 0.0, -6.0)), // farther but has LoS
        ];
        m[0].has_los = false;
        let roles = compute_squad_roles(&m, Vec3::ZERO, PR, &none);
        assert_eq!(roles[&ent(2)], SquadRole::Baiter, "LoS outranks raw distance");
    }

    #[test]
    fn flank_side_follows_player_right() {
        let none = HashMap::new();
        // Baiter dead-ahead and one squadmate to the right both in melee (engaged ==
        // 2 => no reserve held back), and a left squadmate further out: with reserves
        // off, every non-baiter is a flanker so we can read pure side selection.
        let m = [
            mem(1, Vec3::new(0.0, 0.0, -1.5)), // ahead, in melee -> baiter
            mem(2, Vec3::new(1.5, 0.0, -1.5)), // +x, in melee => right of player
            mem(3, Vec3::new(-5.0, 0.0, -2.0)), // -x => left of player
        ];
        let roles = compute_squad_roles(&m, Vec3::ZERO, PR, &none);
        assert_eq!(roles[&ent(1)], SquadRole::Baiter);
        assert_eq!(roles[&ent(2)], SquadRole::FlankRight);
        assert_eq!(roles[&ent(3)], SquadRole::FlankLeft);
    }

    #[test]
    fn third_member_held_in_reserve_until_two_engaged() {
        let none = HashMap::new();
        // Three members, none in melee yet (all > melee_range + pad from player):
        // the farthest is held in Reserve.
        let m = [
            mem(1, Vec3::new(0.0, 0.0, -6.0)),
            mem(2, Vec3::new(4.0, 0.0, -6.0)),
            mem(3, Vec3::new(-2.0, 0.0, -12.0)), // farthest -> reserve
        ];
        let roles = compute_squad_roles(&m, Vec3::ZERO, PR, &none);
        let n_reserve = roles.values().filter(|&&r| r == SquadRole::Reserve).count();
        assert_eq!(n_reserve, 1, "one reserve while engaged < 2");
        assert_eq!(roles[&ent(3)], SquadRole::Reserve, "the farthest holds back");

        // Now push two members into melee range: nobody is held back.
        let m2 = [
            mem(1, Vec3::new(0.0, 0.0, -2.0)), // within melee_range + pad (3.9)
            mem(2, Vec3::new(1.0, 0.0, -2.0)), // within melee_range + pad
            mem(3, Vec3::new(-2.0, 0.0, -12.0)),
        ];
        let roles2 = compute_squad_roles(&m2, Vec3::ZERO, PR, &none);
        let n_reserve2 = roles2.values().filter(|&&r| r == SquadRole::Reserve).count();
        assert_eq!(n_reserve2, 0, "two engaged => reserve commits");
    }

    #[test]
    fn reserve_release_has_an_exit_band() {
        // Squad of three with exactly one engaged: if it was previously committed
        // (a member held a flank role last tick), the hold stays released — it only
        // re-imposes once engaged drops below one.
        let m = [
            mem(1, Vec3::new(0.0, 0.0, -2.0)), // engaged (within 3.9)
            mem(2, Vec3::new(4.0, 0.0, -6.0)), // not engaged
            mem(3, Vec3::new(-2.0, 0.0, -12.0)),
        ];
        // Fresh (no history): one engaged < 2 => one reserve.
        let fresh = compute_squad_roles(&m, Vec3::ZERO, PR, &HashMap::new());
        assert_eq!(fresh.values().filter(|&&r| r == SquadRole::Reserve).count(), 1);

        // With history showing the squad already committed, one engaged keeps it so.
        let mut prev = HashMap::new();
        prev.insert(ent(1), SquadRole::Baiter);
        prev.insert(ent(2), SquadRole::FlankRight);
        prev.insert(ent(3), SquadRole::FlankLeft);
        let held = compute_squad_roles(&m, Vec3::ZERO, PR, &prev);
        assert_eq!(
            held.values().filter(|&&r| r == SquadRole::Reserve).count(),
            0,
            "stays committed in the mid-band"
        );
    }

    #[test]
    fn baiter_is_sticky_within_hysteresis_margin() {
        // Two near-equal contenders; entity 1 was baiter last tick and is only
        // slightly farther than entity 2 now (< BAITER_HYSTERESIS) -> it keeps it.
        let m = [
            mem(1, Vec3::new(0.0, 0.0, -5.2)),
            mem(2, Vec3::new(0.0, 0.0, -5.0)), // marginally nearer
        ];
        let mut prev = HashMap::new();
        prev.insert(ent(1), SquadRole::Baiter);
        let roles = compute_squad_roles(&m, Vec3::ZERO, PR, &prev);
        assert_eq!(roles[&ent(1)], SquadRole::Baiter, "previous baiter holds within margin");

        // But a clearly nearer rival (> margin) takes the role.
        let m2 = [
            mem(1, Vec3::new(0.0, 0.0, -8.0)),
            mem(2, Vec3::new(0.0, 0.0, -5.0)),
        ];
        let roles2 = compute_squad_roles(&m2, Vec3::ZERO, PR, &prev);
        assert_eq!(roles2[&ent(2)], SquadRole::Baiter, "decisive challenger wins");
    }

    #[test]
    fn flank_arcs_behind_when_in_front_then_closes_when_abreast() {
        // A flanker directly in front of the player should NOT just head straight
        // at the player — it should bias sideways/behind to round the front cone.
        let in_front = Vec3::new(0.0, 0.0, -8.0);
        let b = flank_bearing(in_front, Vec3::ZERO, PF, PR, true, 2.4, Vec3::new(0.0, 0.0, -1.0));
        // "toward player" for a front flanker is +Z; an arcing bearing must carry a
        // real sideways (+x for FlankRight) component and not be a pure head-on run.
        assert!(b.x > 0.3, "front flanker arcs to its right, got {b:?}");

        // Once it is abreast/behind the player, it drives straight into melee.
        let abreast = Vec3::new(8.0, 0.0, 0.0); // 90° to the side
        let to_player = (Vec3::ZERO - abreast).normalize_or_zero();
        let b2 = flank_bearing(abreast, Vec3::ZERO, PF, PR, true, 2.4, to_player);
        assert!(b2.distance(to_player) < 1e-4, "abreast flanker closes straight in");
    }

    #[test]
    fn flank_target_is_inside_melee_range() {
        // The transit waypoint a front flanker steers toward must sit within melee
        // range of the player (so arriving == being in attack reach), unlike the old
        // fixed 6u standoff. Reconstruct the waypoint from the bearing geometry.
        let melee = 2.4;
        let behind = (melee * 0.8_f32).max(0.5);
        let target = Vec3::ZERO + PR * FLANK_RADIUS - PF * behind;
        // The waypoint's *distance behind* the player is within melee range.
        assert!(behind <= melee, "behind offset {behind} within melee {melee}");
        // And it is genuinely behind the player (negative along forward), not in front.
        assert!(target.dot(PF) < 0.0, "waypoint sits behind the player");
    }

    #[test]
    fn walkfree_falls_back_to_chase_when_blocked() {
        let pos = Vec3::new(0.0, 0.0, 0.0);
        let half = Vec3::new(0.4, 0.9, 0.4);
        let primary = Vec3::new(1.0, 0.0, 0.0); // +x
        let mirror = Vec3::new(-1.0, 0.0, 0.0); // -x
        let fallback = Vec3::new(0.0, 0.0, -1.0); // toward player

        // No walls: the primary bearing is taken unchanged.
        let open = pick_walkfree_bearing(pos, half, primary, mirror, fallback, &[]);
        assert_eq!(open, primary);

        // A wall straight ahead on +x within FLANK_PROBE but the mirror side open:
        // the mirrored flank is chosen rather than collapsing to head-on.
        let wall_px = Aabb::from_center_half(Vec3::new(2.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 4.0));
        let m = pick_walkfree_bearing(pos, half, primary, mirror, fallback, &[wall_px]);
        assert_eq!(m, mirror, "open mirror side is preferred over head-on");

        // Both flank sides walled in: fall back to direct chase (never freeze).
        let wall_nx = Aabb::from_center_half(Vec3::new(-2.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 4.0));
        let f = pick_walkfree_bearing(pos, half, primary, mirror, fallback, &[wall_px, wall_nx]);
        assert_eq!(f, fallback, "boxed in => plain chase");
    }
}
