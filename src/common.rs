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
    /// Hold Shift to run: scales ground wish-speed (and accel, so the higher
    /// cap is still reached promptly). No stamina — just a flat multiplier.
    pub const RUN_MULTIPLIER: f32 = 4.0;
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

    // --- Wall-run (feature 37) ----------------------------------------------
    // A *sustained* sibling of the wall-jump tap: while airborne, carrying speed,
    // holding a move key, and pressed against a wall, you latch flat to the face
    // and sprint along it (a touch upward) for a short stamina window instead of
    // peeling away. The wall-jump tap stays live — tapping Space mid-run kicks you
    // off exactly as before. Everything writes to `Player.vel` and is handed to
    // `move_and_slide` as normal locomotion; nothing new touches the collider list.
    //
    /// How close the AABB must be to a wall's surface (m) to latch a wall-run.
    /// A touch more generous than `WALLJUMP_REACH` so you can latch by leaning in
    /// without grinding the brush, but still "you must be against the wall".
    pub const WALLRUN_REACH: f32 = 0.45;
    /// Minimum horizontal speed (m/s) needed to latch / stay latched. Below this
    /// you're not "running" — you slide off. Set ABOVE the 9.0 walk speed so a
    /// plain stroll-jump never latches by accident in tight corridor fighting,
    /// but still comfortably under a strafe-jump / bunny-hop so it stays a
    /// momentum verb you can reach on purpose.
    pub const WALLRUN_MIN_SPEED: f32 = 10.5;
    /// How much of gravity survives while latched (the rest is cancelled so you
    /// stick to the face). 0.18 → almost all of gravity is held off, and against
    /// the up-bias below the net vertical drift is a touch UPWARD — you "run flat
    /// along the brickwork (and a touch upward)" per the feature, not plummet.
    /// (See WALLRUN_UP_MAX.)
    pub const WALLRUN_GRAVITY_FRAC: f32 = 0.18;
    /// Along-the-wall acceleration (1/s, Quake-accelerate style) added in the
    /// direction you're already travelling. Brisk enough to *feel* like a sprint
    /// boost, capped by `WALLRUN_MAX_SPEED` so it can't run away.
    pub const WALLRUN_ACCEL: f32 = 30.0;
    /// Speed cap (m/s) the along-wall accelerate chases — the wall-run sprint
    /// ceiling. Well above ground run so blurring a span feels earned, but finite.
    pub const WALLRUN_MAX_SPEED: f32 = 16.0;
    /// Rate (1/s) the wall-run eases `vel.y` toward `WALLRUN_UP_MAX`, so a fall
    /// you latch from arrests quickly instead of plummeting — it catches you,
    /// then holds you near level.
    pub const WALLRUN_UP_BIAS: f32 = 2.2;
    /// Ceiling (m/s) the assisted-hold eases `vel.y` toward. Against the surviving
    /// gravity fraction this settles the net vertical drift at ~+0.2 m/s — a gentle
    /// "touch upward" so a run reads as flat-to-slightly-climbing (per the feature
    /// brief), letting you wall-run to build a horizontal line and wall-jump off its
    /// end for the real height. Keep modest: a much larger value (3.5+ gives ~+1.4
    /// m/s) turns the run into a free ascent (see WALLRUN_GRAVITY_FRAC).
    pub const WALLRUN_UP_MAX: f32 = 2.4;
    /// Stamina window (s) a single latch lasts before it forces a peel-off. Tuned
    /// so one run crosses a long span but not the whole map; refills on the ground.
    pub const WALLRUN_STAMINA: f32 = 1.6;
    /// Seconds of grounded contact it takes to recharge a full `WALLRUN_STAMINA`
    /// from empty (linear). Refilling is gradual, not instant, so a single
    /// grazing-floor frame mid-run (or a one-frame bunny-hop touch between runs)
    /// can't top you back up — you must actually return to the ground to chain
    /// another full run. A touch slower than the burn so back-to-back runs cost.
    pub const WALLRUN_RECHARGE: f32 = 2.0;
    /// Grounded dwell (s) required before stamina starts recharging at all. A
    /// single floor-graze frame during a wall-run (a wall that meets the floor)
    /// trips `on_ground` for a frame or two; requiring a brief dwell first means
    /// such a graze can't refill the run into an infinite grind.
    pub const WALLRUN_GROUND_DWELL: f32 = 0.1;
    /// Seconds of grace the latch survives with NO wall detected (e.g. crossing a
    /// convex brush seam where the swept solver flickers its normal). The smoothed
    /// normal is kept through the gap so you flow across a join instead of
    /// stuttering off it. A couple frames at 60fps.
    pub const WALLRUN_COYOTE: f32 = 0.08;
    /// 1/s the latched wall normal low-passes toward the freshly probed one, so a
    /// seam where two faces hand back competing normals blends smoothly rather
    /// than snapping the run sideways.
    pub const WALLRUN_NORMAL_SMOOTH: f32 = 18.0;
    /// Wall-run scuff loop volume at the latch speed floor (`WALLRUN_MIN_SPEED`).
    pub const WALLRUN_SCUFF_MIN_VOL: f32 = 0.12;
    /// Wall-run scuff loop volume at full sprint (`WALLRUN_MAX_SPEED`).
    pub const WALLRUN_SCUFF_MAX_VOL: f32 = 0.5;
    /// 1/s the scuff loop's volume chases its speed-driven target (snappier than
    /// the engine so it bites in fast on latch and drops on peel-off).
    pub const WALLRUN_SCUFF_SMOOTH: f32 = 8.0;

    /// Cap on downward fall speed (m/s). A fall accelerates up to this and then
    /// holds, so a long drop reads as a steady, trackable plunge instead of
    /// runaway acceleration. Set well above any normal-gameplay fall (you only
    /// reach it after dropping ~20m), so ordinary jumps/ledges are unaffected —
    /// it only bites on the deep void falls (e.g. level 3's foundry shaft).
    pub const TERMINAL_VELOCITY: f32 = 32.0;

    /// Player AABB half-extents and eye offset from the AABB center.
    pub const PLAYER_HALF: [f32; 3] = [0.4, 0.9, 0.4];
    pub const EYE_OFFSET: f32 = 0.65; // eye sits near the top of the box

    // --- Grapnel whip (Whip alt-fire pendulum grapple) -----------------------
    /// Max hitscan reach of the latch ray (m). Long enough to Spider-Man across a
    /// level-8 dam span, short of "anchor the whole map".
    pub const GRAPPLE_RANGE: f32 = 60.0;
    /// Rope-length floor (m). Keeps the player AABB clear of the anchored face so
    /// the constraint and move_and_slide can't deadlock at the anchor.
    pub const GRAPPLE_MIN_LEN: f32 = 2.0;
    /// Slack tolerance (m): treat the rope as taut only once you're this far past
    /// `len`, so micro-jitter at exactly `len` doesn't toggle the constraint.
    pub const GRAPPLE_SLACK: f32 = 0.05;
    /// Reel-in speed while the reel key is held (m/s the rope shortens). About run
    /// speed: a powerful but earned hand-over-hand climb / arc-tightening pump.
    pub const GRAPPLE_REEL_SPEED: f32 = 9.0;
    /// Distance below which the constraint is skipped (degenerate dir / NaN guard).
    pub const GRAPPLE_EPS: f32 = 0.05;
    /// Auto-detach after this many consecutive frames pinned taut against a wall
    /// (scraping a corner) with almost no speed — the anti solver-fight backstop.
    pub const GRAPPLE_STUCK_FRAMES: u32 = 12;

    // --- Ragdoll corpses (feature 17) ----------------------------------------
    // A cleanly-killed monster's SKELETON is driven by a Verlet/PBD ragdoll
    // (`corpse.rs`): one particle per bone joint, bone lengths as distance
    // constraints, light bend-stiffness so the trunk stays semi-rigid while limbs
    // flop, and per-particle world collision so the body drapes over geometry. To
    // OTHER actors the body is a single tracking AABB (reusing the slot pool).
    /// How many corpses present a SOLID tracking box to other actors at once. A
    /// hard cap so a massacre can't spawn dozens of swept boxes; pool slots are
    /// reserved at level build time and recycled by a runtime free-list. Past the
    /// cap a fresh kill still ragdolls (the visual + world-drape needs no slot) —
    /// it just isn't solid to the player/monsters until a slot frees up.
    pub const CORPSE_CAP: usize = 6;
    /// Radius (m) of each ragdoll particle when collided against the world — the
    /// half-extent handed to `physics::depenetrate` so a joint can't sink into a
    /// floor/wall. Roughly a limb's thickness so the body rests on its surface.
    pub const RAGDOLL_PARTICLE_RADIUS: f32 = 0.12;
    /// PBD constraint-solve iterations per frame (distance + bend passes). More =
    /// stiffer/less stretchy at linear cost; 8 keeps a ~30-bone rig taut.
    pub const RAGDOLL_ITERS: usize = 8;
    /// Per-frame Verlet velocity retention (1 = frictionless). Slightly <1 bleeds
    /// solver energy so a settling body doesn't jitter forever.
    pub const RAGDOLL_DAMPING: f32 = 0.98;
    /// Bend (skip-one) constraint stiffness along the TRUNK chain
    /// (pelvis→torso→chest→head): high, so the spine stays near-rigid and the body
    /// tips OVER as a stiff unit (slow plank-fall) rather than crumpling into a puddle.
    pub const RAGDOLL_BEND_TRUNK: f32 = 0.85;
    /// Bend (skip-one) constraint stiffness across LIMB chains: modest, so arms/legs
    /// still flop and trail but don't go instantly limp the frame the body dies.
    pub const RAGDOLL_BEND_LIMB: f32 = 0.18;
    /// Angular rate (rad/s) of the seeded topple impulse — how hard a fresh corpse
    /// is thrown into a fall-OVER rotation about its base (vs. slumping straight
    /// down). Scaled by the upper/lower bias below.
    pub const RAGDOLL_TOPPLE_OMEGA: f32 = 3.0;
    /// Topple-impulse weighting: upper-body particles get the full throw, lower
    /// (feet/shins) barely move — the differential IS the angular momentum that
    /// rotates the body over its planted feet.
    pub const RAGDOLL_UPPER_BIAS: f32 = 1.0;
    pub const RAGDOLL_LOWER_BIAS: f32 = 0.35;
    /// Fraction of an incoming `Knockback` impulse injected into the ragdoll
    /// (splash / Whip fling / truck ram still throw the body, now as a flop).
    pub const RAGDOLL_KNOCK_SCALE: f32 = 1.0;
    /// Speed (m/s) below which a grounded ragdoll is snapped to rest (`prev=pos`)
    /// — kills resting micro-jitter that would otherwise creep the tracking box.
    pub const RAGDOLL_SLEEP_SPEED: f32 = 0.4;
    /// Clamp (m) on the tracking box's half-extents, so even a wide sprawl reads as
    /// low cover and never approaches half a 4m corridor (can't pin the player).
    pub const RAGDOLL_BODYBOX_MAX_HALF: f32 = 0.45;
    /// Slip (m/s) at which a skidding ragdoll plays a faint settle/scrape cue.
    pub const CORPSE_SCRAPE_SPEED: f32 = 2.0;
    /// `Dying.t` (s) at which a corpse stops being a SOLID collider and the ragdoll
    /// freezes; `animate_death` then sinks the held pose into the floor. MUST stay
    /// coupled with `CORPSE_DESPAWN_AT` (sink → vanish) and the matching `t`
    /// thresholds in `monster_model::animate_death`.
    pub const CORPSE_SINK_BEGINS: f32 = 7.0;
    /// `Dying.t` (s) at which a sunk corpse despawns (frees the entity). Two
    /// seconds of sink ramp after `CORPSE_SINK_BEGINS`.
    pub const CORPSE_DESPAWN_AT: f32 = 9.0;

    // --- Resonant brushwork / sonic demolition (feature 29) ------------------
    /// Charge a single Lightning beam PULSE pours into the resonant brush it lands
    /// on. The beam fires one pulse per its `0.06s` cooldown (not once per frame),
    /// so we charge per pulse — frame-rate-independent. The shatter threshold is
    /// normalised to 1.0, so `0.030 * (1/0.06) ≈ 0.5/s` cracks a brush in ~2.2s of
    /// held beam — a tense fistful of Cells under fire ("~1.5-3s of Cells" bet).
    pub const RESONANT_CHARGE_PER_PULSE: f32 = 0.030;
    /// Charge decay per second once the beam leaves a brush. Only applied after a
    /// short grace (`RESONANT_DECAY_GRACE`) so the inter-pulse gap frames (the beam
    /// pulses every ~0.06s, i.e. several frames apart) don't bleed off the charge
    /// you just deposited. Faster than it charges, so you must COMMIT: glance away
    /// and the note sags back down, costing you the progress (and the Cells).
    pub const RESONANT_DECAY_RATE: f32 = 0.65;
    /// Grace window (s) after a brush's last beam pulse before decay kicks in. Must
    /// comfortably exceed the Lightning cooldown (`0.06s`) so a continuously-held
    /// beam never decays between its pulses, yet be short enough that releasing the
    /// beam sags the note within a blink.
    pub const RESONANT_DECAY_GRACE: f32 = 0.12;
    /// Charge (normalised) at which the brush hits its shatter note and detonates.
    pub const RESONANT_SHATTER: f32 = 1.0;
    /// Impact-ring telegraph: a struck (pellet/nail/body) resonant brush briefly
    /// rings at its tone so it's audibly "an instrument". This is the loudest such
    /// ring (scaled down by how soft the hit was); kept modest to avoid spam.
    pub const RESONANT_RING_VOL: f32 = 0.45;

    // --- Whip parry / bat-the-bolt-back (feature 39) -------------------------
    // A left-click Whip swing opens a brief parry window. While it's open, the
    // Whip's existing melee arc/range is swept against every ENEMY-owned
    // projectile body; one caught in the arc is REFLECTED back along its incoming
    // path and FLIPPED enemy->player (a clean parry), or just POPPED out of the
    // air harmlessly (a late/sloppy swing). Either way the player eats nothing.
    //
    /// Total active parry window (s) from the moment the Whip swings. Generous
    /// enough to feel read-and-react, short enough that the Whip isn't a passive
    /// bullet shield (it's a fraction of the 0.75s whip refire cooldown).
    pub const PARRY_WINDOW: f32 = 0.16;
    /// The PERFECT sub-window (s) at the very START of the parry window. A parry
    /// landed inside this leading slice gets the speed + homing bonus; after it
    /// (but still within `PARRY_WINDOW`) is a normal "late" parry that still
    /// deflects but with no bonus.
    pub const PARRY_PERFECT_WINDOW: f32 = 0.06;
    /// Extra padding (m) added to the Whip's melee reach for the parry sweep, so a
    /// bolt skimming just past the lash tip still counts as batted. Kept small so
    /// the parry isn't a giant invisible net — it tracks the visible lash.
    pub const PARRY_REACH_PAD: f32 = 1.0;
    /// Lateral catch radius (m) around the swing ray a projectile must be within to
    /// count as "in the arc". The Whip is a sweep, not a pin-prick, so this is a
    /// fat cylinder along the aim, not the projectile's own tiny radius.
    pub const PARRY_CATCH_RADIUS: f32 = 1.6;
    /// Speed multiplier applied to a PERFECT-parried projectile's returned velocity
    /// (a late parry returns it at 1.0x). Makes a perfectly-timed return scream.
    pub const PARRY_PERFECT_SPEED: f32 = 1.6;
    /// Homing strength (0..1) of a PERFECT parry: how far the reflected velocity is
    /// re-aimed from its pure mirror toward the original shooter. 0 = pure reflect,
    /// 1 = dead-on at the shooter. A nudge, not a guided missile.
    pub const PARRY_HOMING: f32 = 0.5;
    /// Homing strength (0..1) of a LATE parry. A pure mirror amplifies your aim
    /// error ~2x and a late-parried bolt usually whiffs the shooter; a small nudge
    /// gives it a fighting chance to connect while keeping the PERFECT parry clearly
    /// better (it both speeds up AND homes harder). Smaller than `PARRY_HOMING`.
    pub const PARRY_HOMING_LATE: f32 = 0.2;
    /// Damage multiplier on a returned projectile from a LATE parry. A bare deflect
    /// (you take no damage) is good, but at 1.0x a returned 10-dmg bolt can never
    /// threaten the shooter, so the parry reads as purely defensive. A modest bump
    /// makes even a late return a real chip without one-shotting anything.
    pub const PARRY_RETURN_DAMAGE: f32 = 2.0;
    /// Damage multiplier on a returned projectile from a PERFECT parry — the kill
    /// window. Concentrated here so a clean read actually turns the tables (e.g. a
    /// perfect-parried bolt drops a Scrag/Enforcer in two), still gated by the ~21%
    /// shield uptime and the requirement to aim back at the shooter.
    pub const PARRY_PERFECT_DAMAGE: f32 = 3.0;

    // --- Nail pin / stake-them-to-the-wall (feature 41) ----------------------
    // A player nail that strikes a monster sandwiched against world geometry
    // drives through and STAKES it there: it roots the monster for a few seconds
    // (movement zeroed, Chase/Attack overridden) while it twitches to tear free.
    /// Extra reach (m) the pin probe casts PAST the monster's body half-width
    /// along the nail's travel. This is also the slam distance: a nail that
    /// catches a monster merely *near* a surface (within this margin) slams it
    /// the rest of the way flush. Kept short so a wall far behind never pins.
    pub const PIN_REACH: f32 = 0.6;
    /// Base pin duration (s) from a single nail. A few seconds — long enough to
    /// reload, nudge it off a ledge, or cook it in lava.
    pub const PIN_DURATION: f32 = 3.0;
    /// Cap (s) on the accumulated pin timer: a 2nd/3rd nail REFRESHES/EXTENDS the
    /// root (additive) but never past this, so a clump can be sewn down for good
    /// without rooting anything literally forever (the timer always frees the AI).
    pub const PIN_MAX: f32 = 6.0;
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
pub const NUM_LEVELS: usize = 11;

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

/// A severable / individually-tracked limb group on a monster rig. Every bone of
/// a monster maps to exactly one group; pouring enough damage into a group's
/// hurtboxes severs it (the bone subtree detaches and the AI is crippled). The
/// right arm is the weapon arm on every kind, so `ArmR` sever = disarm.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LimbGroup {
    Head,
    Torso, // pelvis/torso/chest/etc — never severs (centre-mass hit surface)
    ArmL,
    ArmR, // the weapon arm on every kind here
    LegL,
    LegR,
    WingL, // Scrag only
}
impl LimbGroup {
    pub const COUNT: usize = 7;
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn severable(self) -> bool {
        !matches!(self, LimbGroup::Torso)
    }
    pub fn is_weapon(self) -> bool {
        matches!(self, LimbGroup::ArmR)
    }
}

// ----------------------------------------------------------------------------
// Messages (buffered events — Bevy 0.19 renamed Event->Message for these)
// ----------------------------------------------------------------------------

/// Apply damage to a target entity. `limb` routes the hit to a specific limb
/// group's pool (for dismemberment) as well as the body Health; `None` is the
/// legacy body-only hit (player damage, splash body pool, enemy fire).
#[derive(Message)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: f32,
    pub source: Option<Entity>,
    pub knockback: Vec3,
    pub limb: Option<LimbGroup>,
}
impl DamageEvent {
    /// A body hit (no specific limb) — the legacy behaviour.
    pub fn body(target: Entity, amount: f32, source: Option<Entity>, knockback: Vec3) -> Self {
        Self { target, amount, source, knockback, limb: None }
    }
    /// A limb-targeted hit: damages Health *and* the limb's sever pool.
    pub fn limb(target: Entity, amount: f32, source: Option<Entity>, knockback: Vec3, limb: LimbGroup) -> Self {
        Self { target, amount, source, knockback, limb: Some(limb) }
    }
}

/// Sever a limb group on a monster rig (detach the bone subtree + spawn a gib +
/// flag the AI). Emitted by `apply_damage` when a limb's pool empties.
#[derive(Message)]
pub struct SeverEvent {
    pub root: Entity,
    pub limb: LimbGroup,
    pub at: Vec3,
    pub dir: Vec3,
}

/// Per-frame flat list of live limb hitboxes, shared by every weapon so all
/// damage paths (hitscan, projectile, melee, splash) are limb-aware. Rebuilt
/// once per frame from live bone `GlobalTransform`s.
#[derive(Resource, Default)]
pub struct LimbBoxes {
    /// (root enemy entity, group, world-space AABB) for present, un-severed limbs.
    pub boxes: Vec<(Entity, LimbGroup, Aabb)>,
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
    /// Inverts the blast into a gravity well (the Lodestone): pull targets inward
    /// for zero damage instead of flinging them out. `push` is then the per-frame
    /// inward velocity impulse magnitude (PULL_ACCEL * dt) so it's framerate-independent.
    pub implode: bool,
    /// If this blast came from a projectile that struck a specific limb directly,
    /// the (target, limb) it hit — so splash focuses that limb.
    pub direct_limb: Option<(Entity, LimbGroup)>,
    /// True when this blast came from a Whip-parried (returned) projectile (feature
    /// 39). The projectile is now player-owned, but the player who batted it back
    /// must NEVER be hurt by its splash: `handle_explosions` skips the explosion's
    /// `source` (the player) entirely for a returned blast. Makes the parry's
    /// self-immunity airtight regardless of blast geometry.
    pub returned: bool,
    /// True for the mounted pintle cannon's shell (feature 46): like `returned`, it
    /// makes `handle_explosions` skip the blast's `source` entirely, so a manned deck
    /// gun can never fling or gib its own gunner with its own splash (the muzzle is
    /// fixed to the deck, decoupled from where the gunner stands).
    pub no_self_blast: bool,
}

/// A monster corpse left behind on death; fades out after the timer.
#[derive(Component)]
pub struct Corpse(pub f32);

/// Stake a monster to a surface (feature 41). Emitted by `projectile_move` when a
/// player nail strikes a monster sandwiched against world geometry; consumed by
/// `enemies::apply_pins`, which sets/refreshes the `Pinned` state and snaps the
/// rig flush to the wall. `anchor` is the snapped body centre to hold at; `normal`
/// is the surface normal; `dur` is the seconds of root this nail grants.
#[derive(Message)]
pub struct PinEvent {
    pub target: Entity,
    pub normal: Vec3,
    pub anchor: Vec3,
    pub dur: f32,
}

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

/// A hitscan ray ended on a world brush (feature 29). The resonance system reads
/// these to charge a resonant brush the Lightning beam is dwelling on (`beam`)
/// or to ring one that a pellet/nail just struck (a telegraph). One per ray that
/// terminates on geometry; non-resonant slots are ignored downstream.
#[derive(Message)]
pub struct BrushStrike {
    /// Index into `WorldColliders.solids`.
    pub slot: usize,
    pub point: Vec3,
    /// True for a Lightning-beam pulse (drives the sweep), false for a one-shot
    /// hit (drives the impact ring).
    pub beam: bool,
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
    RopeTaut,
    Sever,
    Lodestone,
    EngineLoop,
    EngineStart,
    TireScreech,
    Crash,
    RamHit,
    /// Faint meaty scrape/thud as a knocked corpse skids and settles (feature 17).
    CorpseSettle,
    /// Looping clean sine hum a resonant brush sings while the Lightning beam
    /// dwells on it; the game scrubs its pitch up toward the shatter note and its
    /// volume with dwell (feature 29).
    ResonantHum,
    /// Short, decaying one-shot "tink" a resonant brush rings with when struck by a
    /// pellet/nail/body (feature 29) — the percussive telegraph, distinct from the
    /// sustained `ResonantHum` loop the held beam drives.
    ResonantRing,
    /// The crack-and-pop when a resonant brush reaches its shatter note and
    /// detonates into gibs (feature 29).
    ResonantShatter,
    /// Looping gritty scuff/scrape of boots dragging along a wall while wall-running
    /// (feature 37); the game fades its volume in with wall-run speed and stops it
    /// the instant you peel off.
    WallrunScuff,
    /// Sharp metallic "ting" the Whip rings when it parries an incoming projectile
    /// out of the air (feature 39) — a struck-steel ping, perfect parries pitched up.
    MetallicTing,
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
            Sound::RopeTaut => "sounds/rope_taut.wav",
            Sound::Sever => "sounds/sever.wav",
            Sound::Lodestone => "sounds/lodestone.wav",
            // ElevenLabs-generated vehicle sounds (scripts/generate_sounds.py).
            Sound::EngineLoop => "sounds/engine_loop.wav",
            Sound::EngineStart => "sounds/engine_start.wav",
            Sound::TireScreech => "sounds/tire_screech.wav",
            Sound::Crash => "sounds/crash.wav",
            Sound::RamHit => "sounds/ram_hit.wav",
            Sound::CorpseSettle => "sounds/corpse_settle.wav",
            Sound::ResonantHum => "sounds/resonant_hum.wav",
            Sound::ResonantRing => "sounds/resonant_ring.wav",
            Sound::ResonantShatter => "sounds/resonant_shatter.wav",
            Sound::WallrunScuff => "sounds/wallrun_scuff.wav",
            Sound::MetallicTing => "sounds/metallic_ting.wav",
        }
    }
    pub fn all() -> [Sound; 39] {
        use Sound::*;
        [
            Shotgun, SuperShotgun, Nailgun, RocketFire, GrenadeFire, Explosion,
            GrenadeBounce, Impact, PickupHealth, PickupArmor, PickupAmmo,
            PickupWeapon, KeyPickup, Jump, Land, PlayerPain, PlayerDeath,
            EnemySight, EnemyPain, EnemyDeath, Door, Victory, Ambient, Lightning,
            Whip, RopeTaut, Sever, Lodestone, EngineLoop, EngineStart, TireScreech, Crash, RamHit,
            CorpseSettle, ResonantHum, ResonantRing, ResonantShatter, WallrunScuff,
            MetallicTing,
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
// Floor materials (feature 47): the surface underfoot rewrites how the GROUND
// branch of movement behaves. Each brush carries one, stored parallel to the
// collider list (see `WorldColliders.materials`). A material only ever SCALES the
// grounded friction + ground accel/cap (and adds a conveyor push); the airborne
// path never reads it, so the bunny-hop and air-strafe feel is untouched.
// ----------------------------------------------------------------------------
/// The movement personality of a floor brush. `Normal` is today's exact values
/// (all multipliers 1.0, no push), so an untagged brush behaves identically to
/// before this feature — existing levels are unchanged unless they opt in.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FloorMaterial {
    /// Standard footing — the global default (multipliers 1.0, no push).
    #[default]
    Normal,
    /// Near-frictionless: you carry speed and overshoot, and your grounded
    /// control is sluggish (low accel/cap) so you can't snap-stop or turn — the
    /// classic ice room, now a campaign-wide rule.
    Ice,
    /// Mud / tar: heavy drag (high friction) and bled acceleration, so a sprint
    /// becomes a wade and you can't dodge — a glue-trap kill-box.
    Tar,
    /// Conveyor grate / slick belt: standard friction + accel, plus a constant
    /// push that slides whoever stands on it along `dir` at `speed` (m/s).
    Conveyor { dir: Vec2, speed: f32 },
    /// Ringing steel plate: reserved for the foundry/dam palette. Movement-identical
    /// to `Normal` today — it carries no friction/accel/push difference. The intended
    /// distinction is a footstep-timbre variant, which is NOT wired yet (there is no
    /// footstep system), so right now this behaves exactly like `Normal`.
    Metal,
}
impl FloorMaterial {
    /// A conveyor pushing along the horizontal `dir` (x,z) at `speed` m/s.
    pub fn conveyor(dir: Vec2, speed: f32) -> Self {
        FloorMaterial::Conveyor { dir, speed }
    }

    /// Multiplier on the grounded friction term. <1 = slippery (ice), >1 = draggy
    /// (tar). Exactly 1.0 for `Normal`/`Metal`/`Conveyor`, so they bleed speed
    /// identically to the original constant.
    pub fn friction_mul(self) -> f32 {
        match self {
            FloorMaterial::Ice => 0.04,
            FloorMaterial::Tar => 2.4,
            _ => 1.0,
        }
    }

    /// Multiplier on the grounded accel AND wish-speed cap. <1 = mushy control
    /// (ice slides past, tar wades). Exactly 1.0 for `Normal`/`Metal`/`Conveyor`.
    pub fn accel_mul(self) -> f32 {
        match self {
            FloorMaterial::Ice => 0.35,
            FloorMaterial::Tar => 0.5,
            _ => 1.0,
        }
    }

    /// The constant per-second push this floor adds to a grounded rider (zero for
    /// everything but a conveyor). Horizontal; fed through the swept solver so it
    /// respects walls — the same idea as `vehicle_carry`'s rider delta.
    pub fn push(self) -> Vec3 {
        match self {
            FloorMaterial::Conveyor { dir, speed } => {
                let d = dir.normalize_or_zero();
                Vec3::new(d.x, 0.0, d.y) * speed
            }
            _ => Vec3::ZERO,
        }
    }
}

// ----------------------------------------------------------------------------
// World collision geometry (static level brushes). Doors are handled separately.
// ----------------------------------------------------------------------------
#[derive(Resource, Default)]
pub struct WorldColliders {
    pub solids: Vec<Aabb>,
    /// Floor material per solid, kept rigorously index-aligned with `solids`
    /// (feature 47). Build-time brushes push one here for every collider; the
    /// reserved door/vehicle/rail/corpse slots are `Normal`. The lookup is
    /// bounds-safe — any index past the end (e.g. a corpse-pool slot appended
    /// after the build) reads back as `Normal`.
    pub materials: Vec<FloorMaterial>,
    /// `true` iff the active level has at least one non-`Normal` floor material
    /// (feature 47). Computed once at build time. The per-frame movement code (the
    /// player AND every grounded enemy) skips the downward `ground_brush` material
    /// probe entirely when this is `false` — i.e. on the seven floors-are-just-floors
    /// levels (including the level-8 dam stress test) the feature costs nothing.
    pub has_floor_material: bool,
}
impl WorldColliders {
    /// Floor material of solid slot `i` (feature 47). Bounds-safe: an out-of-range
    /// index reads back as `Normal`, so a length mismatch can never panic — it just
    /// falls back to standard footing.
    pub fn material(&self, i: usize) -> FloorMaterial {
        self.materials.get(i).copied().unwrap_or_default()
    }
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
