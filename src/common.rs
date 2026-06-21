//! Cross-module shared types: constants, game state, health, messages, resources.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::physics::Aabb;

// ----------------------------------------------------------------------------
// Tuning constants (units: meters, seconds) — chosen to feel like Quake.
// ----------------------------------------------------------------------------
pub mod tune {
    pub const GRAVITY: f32 = 26.0;
    pub const MAX_GROUND_SPEED: f32 = 9.0;
    pub const GROUND_ACCEL: f32 = 11.0;
    pub const AIR_ACCEL: f32 = 11.0;
    /// Air wish-speed cap — the classic Quake value that enables air-strafing.
    pub const AIR_CAP: f32 = 1.1;
    pub const FRICTION: f32 = 6.0;
    pub const STOP_SPEED: f32 = 1.6;
    pub const JUMP_SPEED: f32 = 8.6;
    pub const STEP_HEIGHT: f32 = 0.5;

    /// Wall jump: while airborne and pressed against a wall, a fresh jump press
    /// kicks off it — an upward boost (`WALLJUMP_UP`) plus an outward push away
    /// from the wall (`WALLJUMP_PUSH`). `WALLJUMP_REACH` is how close the
    /// player's AABB must be to the wall (meters from its surface) to qualify.
    pub const WALLJUMP_UP: f32 = 8.0;
    pub const WALLJUMP_PUSH: f32 = 4.5;
    pub const WALLJUMP_REACH: f32 = 0.2;

    /// Cap on downward fall speed (m/s). A fall accelerates up to this and then
    /// holds, so a long drop reads as a steady, trackable plunge instead of
    /// runaway acceleration. Set well above any normal-gameplay fall (you only
    /// reach it after dropping ~20m), so ordinary jumps/ledges are unaffected —
    /// it only bites on the deep void falls (e.g. level 3's foundry shaft).
    pub const TERMINAL_VELOCITY: f32 = 32.0;

    /// Player AABB half-extents and eye offset from the AABB center.
    pub const PLAYER_HALF: [f32; 3] = [0.4, 0.9, 0.4];
    pub const EYE_OFFSET: f32 = 0.65; // eye sits near the top of the box
}

// ----------------------------------------------------------------------------
// Game state
// ----------------------------------------------------------------------------
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum GameState {
    #[default]
    Playing,
    /// One-frame bounce state used to tear down the old level and rebuild the
    /// next one (Playing → Loading → Playing) when advancing between levels.
    Loading,
    Dead,
    Victory,
}

/// How many levels the campaign has. A fresh run starts at a random one and
/// then advances level-by-level to the last; finishing the last one wins.
pub const NUM_LEVELS: usize = 7;

/// Marker for entities that belong to the active mission and should be cleared
/// when the level is rebuilt on restart.
#[derive(Component)]
pub struct LevelEntity;

/// A sliding door. Opens (slides by `open_offset`) once the player has the key
/// and is nearby; opening disables its collider in `WorldColliders.solids`.
#[derive(Component)]
pub struct Door {
    pub solid_index: usize,
    pub closed_pos: Vec3,
    pub open_offset: Vec3,
    pub opening: bool,
    pub opened: bool,
    pub t: f32,
}

// ----------------------------------------------------------------------------
// Factions / combat tags
// ----------------------------------------------------------------------------
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction {
    Player,
    Monster,
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    pub dead: bool,
}
impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max, dead: false }
    }
}

/// Accumulated knockback impulse, consumed by movement systems.
#[derive(Component, Default)]
pub struct Knockback(pub Vec3);

/// Axis-aligned hit volume (half-extents) used for hitscan/splash targeting.
#[derive(Component, Clone, Copy)]
pub struct Hurtbox {
    pub half: Vec3,
}

/// Armor absorbs a fraction of incoming damage until it runs out.
#[derive(Component, Default)]
pub struct Armor {
    pub points: f32,
    pub absorb: f32, // fraction of damage absorbed by armor (0..1)
}

// ----------------------------------------------------------------------------
// Messages (buffered events — Bevy 0.19 renamed Event->Message for these)
// ----------------------------------------------------------------------------

/// Apply damage to a target entity.
#[derive(Message)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: f32,
    pub source: Option<Entity>,
    pub knockback: Vec3,
}

/// Spawn an explosion that deals radius damage + knockback and a visual blast.
#[derive(Message)]
pub struct ExplosionEvent {
    pub pos: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub source: Option<Entity>,
    pub from_player: bool,
    pub color: Color,
    pub push: f32,
}

/// A monster corpse left behind on death; fades out after the timer.
#[derive(Component)]
pub struct Corpse(pub f32);

/// Small visual hit decoration.
#[derive(Message)]
pub struct ImpactEvent {
    pub pos: Vec3,
    pub normal: Vec3,
    pub blood: bool,
}

/// Play a sound effect, optionally positioned (volume falls off with distance).
#[derive(Message)]
pub struct Sfx {
    pub sound: Sound,
    pub pos: Option<Vec3>,
    pub volume: f32,
    pub pitch: f32,
}
impl Sfx {
    pub fn global(sound: Sound) -> Self {
        Self { sound, pos: None, volume: 1.0, pitch: 1.0 }
    }
    pub fn at(sound: Sound, pos: Vec3) -> Self {
        Self { sound, pos: Some(pos), volume: 1.0, pitch: 1.0 }
    }
    pub fn pitched(sound: Sound, pos: Vec3, pitch: f32) -> Self {
        Self { sound, pos: Some(pos), volume: 1.0, pitch }
    }
}

/// Camera kick / screen shake impulse.
#[derive(Message)]
pub struct ScreenShake {
    pub amount: f32,
}

/// Brief full-screen colored flash (e.g. red when hurt, gold on pickup).
#[derive(Message)]
pub struct ScreenFlash {
    pub color: Color,
    pub strength: f32,
}

/// A short HUD notification line (pickups, objectives).
#[derive(Message)]
pub struct Notify {
    pub text: String,
}
impl Notify {
    pub fn new(s: impl Into<String>) -> Self {
        Self { text: s.into() }
    }
}

// ----------------------------------------------------------------------------
// Sounds
// ----------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sound {
    Shotgun,
    SuperShotgun,
    Nailgun,
    RocketFire,
    GrenadeFire,
    Explosion,
    GrenadeBounce,
    Impact,
    PickupHealth,
    PickupArmor,
    PickupAmmo,
    PickupWeapon,
    KeyPickup,
    Jump,
    Land,
    PlayerPain,
    PlayerDeath,
    EnemySight,
    EnemyPain,
    EnemyDeath,
    Door,
    Victory,
    Ambient,
    Lightning,
    Whip,
}
impl Sound {
    pub fn file(self) -> &'static str {
        match self {
            Sound::Shotgun => "sounds/shotgun.wav",
            Sound::SuperShotgun => "sounds/super_shotgun.wav",
            Sound::Nailgun => "sounds/nailgun.wav",
            Sound::RocketFire => "sounds/rocket_fire.wav",
            Sound::GrenadeFire => "sounds/grenade_fire.wav",
            Sound::Explosion => "sounds/explosion.wav",
            Sound::GrenadeBounce => "sounds/grenade_bounce.wav",
            Sound::Impact => "sounds/impact.wav",
            Sound::PickupHealth => "sounds/pickup_health.wav",
            Sound::PickupArmor => "sounds/pickup_armor.wav",
            Sound::PickupAmmo => "sounds/pickup_ammo.wav",
            Sound::PickupWeapon => "sounds/pickup_weapon.wav",
            Sound::KeyPickup => "sounds/key_pickup.wav",
            Sound::Jump => "sounds/jump.wav",
            Sound::Land => "sounds/land.wav",
            Sound::PlayerPain => "sounds/player_pain.wav",
            Sound::PlayerDeath => "sounds/player_death.wav",
            Sound::EnemySight => "sounds/enemy_sight.wav",
            Sound::EnemyPain => "sounds/enemy_pain.wav",
            Sound::EnemyDeath => "sounds/enemy_death.wav",
            Sound::Door => "sounds/door.wav",
            Sound::Victory => "sounds/victory.wav",
            Sound::Ambient => "sounds/ambient.wav",
            Sound::Lightning => "sounds/lightning.wav",
            Sound::Whip => "sounds/whip.wav",
        }
    }
    pub fn all() -> [Sound; 25] {
        use Sound::*;
        [
            Shotgun, SuperShotgun, Nailgun, RocketFire, GrenadeFire, Explosion,
            GrenadeBounce, Impact, PickupHealth, PickupArmor, PickupAmmo,
            PickupWeapon, KeyPickup, Jump, Land, PlayerPain, PlayerDeath,
            EnemySight, EnemyPain, EnemyDeath, Door, Victory, Ambient, Lightning,
            Whip,
        ]
    }
}

#[derive(Resource, Default)]
pub struct Sounds {
    pub map: HashMap<Sound, Handle<AudioSource>>,
}
impl Sounds {
    pub fn get(&self, s: Sound) -> Handle<AudioSource> {
        self.map.get(&s).cloned().unwrap_or_default()
    }
}

// ----------------------------------------------------------------------------
// World collision geometry (static level brushes). Doors are handled separately.
// ----------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct WorldColliders {
    pub solids: Vec<Aabb>,
}

// ----------------------------------------------------------------------------
// Mission objective tracking
// ----------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct Mission {
    pub has_key: bool,
    pub kills: u32,
    pub total_enemies: u32,
    pub objective: String,
}

// ----------------------------------------------------------------------------
// Campaign / run progression
// ----------------------------------------------------------------------------
/// A snapshot of the player's inventory carried from one level into the next
/// (weapons keep, ammo keeps, health/armor keep). Stored as primitives so this
/// lives in `common` without depending on `weapons`.
#[derive(Default, Clone)]
pub struct Carry {
    pub owned: [bool; 7],
    pub ammo: [i32; 4],
    pub current: usize, // index into WeaponKind::ALL
    pub health: f32,
    pub armor_points: f32,
    pub armor_absorb: f32,
}

/// Drives which level is built and whether to carry the player's loadout into
/// it. A fresh start/restart randomizes `level` and clears `carry_inventory`;
/// finishing a level increments `level` and sets `carry_inventory`.
#[derive(Resource)]
pub struct RunState {
    pub level: usize,
    pub carry_inventory: bool,
    pub carry: Carry,
}
impl Default for RunState {
    fn default() -> Self {
        Self { level: 0, carry_inventory: false, carry: Carry::default() }
    }
}

/// The configured fresh-run starting level, read from `config.ron` at startup.
/// `None` → pick a random level each run (the default); `Some(i)` → always start
/// on the fixed 0-based level index `i`. The `QC_LEVEL` env var overrides it.
#[derive(Resource, Clone, Copy, Default)]
pub struct StartLevelConfig(pub Option<usize>);

/// Per-level visual/hazard styling consumed by systems outside `level.rs`
/// (player fog, lava/hazard pulse + damage). Set by `setup_level`.
#[derive(Resource)]
pub struct LevelStyle {
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    /// Base emissive of the hazard material (pulsed each frame).
    pub hazard_emissive: LinearRgba,
    /// Damage dealt per hazard tick (every 0.3s) while standing in it.
    pub hazard_dot: f32,
    /// Screen-tint color while burning/freezing/etc. in the hazard.
    pub hazard_flash: Color,
}
impl Default for LevelStyle {
    fn default() -> Self {
        Self {
            fog_color: rgb(0.12, 0.11, 0.15),
            fog_start: 16.0,
            fog_end: 62.0,
            hazard_emissive: LinearRgba::rgb(5.0, 1.2, 0.1),
            hazard_dot: 12.0,
            hazard_flash: rgb(0.9, 0.35, 0.05),
        }
    }
}

/// The "Level N: Name" banner shown for a few seconds when a level starts.
#[derive(Resource, Default)]
pub struct LevelIntro {
    pub text: String,
    pub timer: f32,
}

// ----------------------------------------------------------------------------
// Shared prebuilt rendering assets
// ----------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct GfxAssets {
    pub unit_cube: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub small_sphere: Handle<Mesh>,
    /// Unit cylinder (radius 0.5, height 1, axis +Y) — barrels, shells, batteries.
    pub cylinder: Handle<Mesh>,
    /// Unit cone (radius 0.5, height 1, apex +Y) — warheads, nails, muzzle spikes.
    pub cone: Handle<Mesh>,
    pub white_unlit: Handle<StandardMaterial>,
    pub blood: Handle<StandardMaterial>,
    pub gib: Handle<StandardMaterial>,
    pub spark: Handle<StandardMaterial>,
    pub smoke: Handle<StandardMaterial>,
    pub muzzle: Handle<StandardMaterial>,
    pub explosion: Handle<StandardMaterial>,
    pub rocket: Handle<StandardMaterial>,
    pub grenade: Handle<StandardMaterial>,
    pub nail: Handle<StandardMaterial>,
    pub plasma: Handle<StandardMaterial>,
    pub lava_mat: Handle<StandardMaterial>,
}

// Convenience: quick srgb color.
#[inline]
pub fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::srgb(r, g, b)
}
