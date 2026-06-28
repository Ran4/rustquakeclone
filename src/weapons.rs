//! Player weapons: inventory, firing (hitscan + projectile), switching, the
//! first-person view-model, muzzle flash and recoil.

use bevy::audio::Volume;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::common::tune::*;
use crate::common::*;
use crate::effects::{spawn_muzzle_flash, Lifetime};
use crate::monster_model::nearest_limb_hit;
use crate::mount::{ActiveMount, WhipHit};
use crate::physics::{line_of_sight, ray_aabb, raycast_world, raycast_world_indexed, Aabb};
use crate::pickups::{ball, cube, part_material, prim_mesh, skin, spike_fwd, tube_x, tube_z, Part};
use crate::player::{Player, PlayerCamera};
use crate::projectiles::{spawn_projectile, ProjKind, WhipParry};
use crate::vehicle::{ActiveGunner, ActiveVehicle};

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewKick>()
            .add_systems(
                Update,
                (switch_weapon, fire_weapon, update_viewmodel, grapple_audio_lifecycle)
                    .run_if(in_state(GameState::Playing)),
            )
            // grapple_cast does the discrete edges (latch / detach / range / LoS).
            // It runs after look (aim is fresh) and after vehicle_activate (a
            // same-frame mount detaches), and before player_move so the velocity
            // constraint inside player_move sees a same-frame latch.
            .add_systems(
                Update,
                grapple_cast
                    .after(crate::player::player_look)
                    .after(crate::vehicle::vehicle_activate)
                    .before(crate::player::player_move)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                update_rope_visual.after(grapple_cast).run_if(in_state(GameState::Playing)),
            )
            // Tear down the rope sound + visual on death/victory/level-change (the
            // run_if(Playing) systems can't fire the normal release path there).
            .add_systems(OnExit(GameState::Playing), cleanup_grapple);
    }
}

// ----------------------------------------------------------------------------
// Grapnel whip — the Whip's right-mouse alt-fire: a hitscan latch onto world
// geometry that hangs the player on a pendulum rope. The rope is a velocity-only
// constraint applied inside `player_move` (see `rope_constrain`); this module
// owns the latch/detach edges, the rope visual and the taut-rope audio loop. The
// left-click melee whip (`fire_weapon` / `melee_strike`) is untouched.
// ----------------------------------------------------------------------------

/// Held to reel the rope in (shorten it) while swinging — a hand-over-hand climb
/// and arc-tightener, and the way to get a vertical yank off a ceiling anchor.
pub const GRAPPLE_REEL_KEY: KeyCode = KeyCode::ControlLeft;

/// Active grapnel-whip rope state, on the player entity. `None` = not grappling.
#[derive(Component, Default)]
pub struct Grapple {
    pub hook: Option<GrappleHook>,
}

#[derive(Clone)]
pub struct GrappleHook {
    /// Fixed world latch point, captured at latch time (a moving anchor degrades
    /// gracefully via the line-of-sight auto-detach rather than tracking it).
    pub anchor: Vec3,
    /// Current rope length / constraint radius (>= GRAPPLE_MIN_LEN).
    pub len: f32,
    /// Consecutive frames pinned taut against a wall with ~no speed (watchdog).
    pub stuck_frames: u32,
}

/// The single looping taut-rope audio entity (present only while latched).
#[derive(Component)]
struct RopeSound;

/// The single persistent rope-line mesh (present only while latched).
#[derive(Component)]
struct RopeVisual;

/// One frame of the rope constraint. Pure + headless-testable (no ECS access).
/// `center` = player AABB center; `vel` = `Player.vel` after gravity, pre-sweep.
/// Cancels ONLY the outward radial velocity when the rope is taut; it never adds
/// energy and never writes position, so the swept solver runs exactly as before
/// and the pendulum is driven entirely by the gravity/air-accel already in `vel`.
pub fn rope_constrain(center: Vec3, vel: Vec3, anchor: Vec3, len: f32) -> Vec3 {
    let to_anchor = anchor - center;
    let dist = to_anchor.length();
    if dist < GRAPPLE_EPS || len < GRAPPLE_EPS {
        return vel; // degenerate / NaN guard
    }
    if dist <= len + GRAPPLE_SLACK {
        return vel; // slack: a rope only pulls, never pushes
    }
    let radial = to_anchor / dist; // unit, points center -> anchor (inward)
    let along = vel.dot(radial); // > 0 = moving toward the anchor (inward)
    if along < 0.0 {
        vel - radial * along // remove only the outward part
    } else {
        vel // inward motion (a fall, or the reel-in pull injected upstream) is free
    }
}

/// A weapon-material albedo skin, keyed by material role rather than by weapon —
/// a handful of textures skin every part of every gun (and the first-person
/// view-models). `Painted` is a neutral grey sheet meant to be tinted per weapon
/// (green grenade launcher, red rocket launcher, blue lightning gun, …). The
/// handles are pulled straight from the `AssetServer` at point of use (it dedupes
/// by path), which sidesteps any startup-vs-OnEnter load ordering. Primitive
/// meshes carry 0..1 UVs, so the default (clamp) sampler is correct.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WeaponTex {
    Gunmetal,
    Brass,
    Steel,
    Wood,
    Painted,
}
impl WeaponTex {
    pub fn file(self) -> &'static str {
        match self {
            WeaponTex::Gunmetal => "textures/weapons/gunmetal.png",
            WeaponTex::Brass => "textures/weapons/brass.png",
            WeaponTex::Steel => "textures/weapons/steel.png",
            WeaponTex::Wood => "textures/weapons/wood.png",
            WeaponTex::Painted => "textures/weapons/painted.png",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponKind {
    Shotgun,
    SuperShotgun,
    Nailgun,
    Grenade,
    Rocket,
    Lightning,
    Whip,
}
impl WeaponKind {
    pub const ALL: [WeaponKind; 7] = [
        WeaponKind::Shotgun,
        WeaponKind::SuperShotgun,
        WeaponKind::Nailgun,
        WeaponKind::Grenade,
        WeaponKind::Rocket,
        WeaponKind::Lightning,
        WeaponKind::Whip,
    ];
    pub fn index(self) -> usize {
        WeaponKind::ALL.iter().position(|&w| w == self).unwrap()
    }
    pub fn name(self) -> &'static str {
        match self {
            WeaponKind::Shotgun => "Shotgun",
            WeaponKind::SuperShotgun => "Super Shotgun",
            WeaponKind::Nailgun => "Nailgun",
            WeaponKind::Grenade => "Grenade Launcher",
            WeaponKind::Rocket => "Rocket Launcher",
            WeaponKind::Lightning => "Lightning Gun",
            WeaponKind::Whip => "Whip",
        }
    }
    pub fn ammo(self) -> usize {
        match self {
            WeaponKind::Shotgun | WeaponKind::SuperShotgun => 0, // Shells
            WeaponKind::Nailgun => 1,                            // Nails
            WeaponKind::Grenade | WeaponKind::Rocket => 2,       // Rockets
            WeaponKind::Lightning => 3,                          // Cells
            WeaponKind::Whip => 0,                               // none (cost 0)
        }
    }
    /// True for weapons that never consume ammo (the HUD shows ∞ for these).
    pub fn infinite(self) -> bool {
        matches!(self, WeaponKind::Whip)
    }
    fn stats(self) -> Stats {
        match self {
            WeaponKind::Shotgun => Stats { cooldown: 0.55, cost: 1, sound: Sound::Shotgun, shake: 0.12, kick: 0.5, mode: Mode::Hitscan { pellets: 6, spread: 0.045, damage: 5.0 } },
            WeaponKind::SuperShotgun => Stats { cooldown: 0.85, cost: 2, sound: Sound::SuperShotgun, shake: 0.22, kick: 1.0, mode: Mode::Hitscan { pellets: 14, spread: 0.075, damage: 5.0 } },
            WeaponKind::Nailgun => Stats { cooldown: 0.1, cost: 1, sound: Sound::Nailgun, shake: 0.04, kick: 0.18, mode: Mode::Projectile(ProjKind::Nail) },
            WeaponKind::Grenade => Stats { cooldown: 0.7, cost: 1, sound: Sound::GrenadeFire, shake: 0.1, kick: 0.5, mode: Mode::Projectile(ProjKind::Grenade) },
            WeaponKind::Rocket => Stats { cooldown: 0.85, cost: 1, sound: Sound::RocketFire, shake: 0.18, kick: 0.9, mode: Mode::Projectile(ProjKind::Rocket) },
            WeaponKind::Lightning => Stats { cooldown: 0.06, cost: 1, sound: Sound::Lightning, shake: 0.05, kick: 0.12, mode: Mode::Beam { damage: 8.0 } },
            // Free melee fallback: short reach, no ammo, big knockback that flings
            // monsters away (cost 0 so the ammo check always passes).
            WeaponKind::Whip => Stats { cooldown: 0.75, cost: 0, sound: Sound::Whip, shake: 0.1, kick: 0.75, mode: Mode::Melee { damage: 20.0, range: 4.5, knockback: 30.0 } },
        }
    }
}

struct Stats {
    cooldown: f32,
    cost: i32,
    sound: Sound,
    shake: f32,
    kick: f32,
    mode: Mode,
}
enum Mode {
    Hitscan { pellets: u32, spread: f32, damage: f32 },
    Projectile(ProjKind),
    Beam { damage: f32 },
    Melee { damage: f32, range: f32, knockback: f32 },
}

#[derive(Component)]
pub struct Inventory {
    pub owned: [bool; 7],
    pub ammo: [i32; 4],
    pub current: WeaponKind,
    pub cooldown: f32,
}
impl Default for Inventory {
    fn default() -> Self {
        let mut owned = [false; 7];
        owned[WeaponKind::Shotgun.index()] = true;
        owned[WeaponKind::Whip.index()] = true; // melee fallback, always available
        Self { owned, ammo: [35, 0, 0, 0], current: WeaponKind::Shotgun, cooldown: 0.0 }
    }
}

#[derive(Resource, Default)]
pub struct ViewKick {
    pub amount: f32,
}

/// The first-person view-model root (a child of the camera). Its children are
/// the weapon's little-primitive model parts, rebuilt whenever the held weapon
/// changes; `built` tracks which weapon's parts are currently spawned.
#[derive(Component)]
pub struct ViewModel {
    built: Option<WeaponKind>,
}

/// Prebuilt view-model geometry + per-part materials, one entry per weapon
/// (indexed by `WeaponKind::index`). Built once so a weapon switch just respawns
/// cheap child meshes against cached material handles.
#[derive(Resource)]
pub struct WeaponVis {
    parts: Vec<Vec<Part>>,
    mats: Vec<Vec<Handle<StandardMaterial>>>,
}

/// Build every weapon's view-model parts + materials once (idempotent). Runs at
/// the front of the OnEnter(Playing) chain so it exists before any weapon system
/// needs it. The geometry is the same little-primitive system the ground pickups
/// use; the materials cache here so per-switch rebuilds allocate nothing.
pub fn create_weapon_vis(
    mut commands: Commands,
    existing: Option<Res<WeaponVis>>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if existing.is_some() {
        return;
    }
    let mut parts = Vec::with_capacity(WeaponKind::ALL.len());
    let mut mats = Vec::with_capacity(WeaponKind::ALL.len());
    for kind in WeaponKind::ALL {
        let p = viewmodel_parts(kind);
        let m = p.iter().map(|part| part_material(part, &asset_server, &mut materials)).collect();
        parts.push(p);
        mats.push(m);
    }
    commands.insert_resource(WeaponVis { parts, mats });
}

/// Spawn the cached view-model parts for weapon `idx` as children of `root`.
fn build_viewmodel(commands: &mut Commands, root: Entity, vis: &WeaponVis, gfx: &GfxAssets, idx: usize) {
    commands.entity(root).with_children(|p| {
        for (part, mat) in vis.parts[idx].iter().zip(vis.mats[idx].iter()) {
            p.spawn((
                Mesh3d(prim_mesh(part.prim, gfx)),
                MeshMaterial3d(mat.clone()),
                Transform { translation: part.pos, rotation: part.rot, scale: part.size },
            ));
        }
    });
}

/// The first-person view-model geometry for a weapon — a parented set of
/// little low-poly primitives (the same system the ground pickups use), built
/// muzzle-forward (-Z) and skinned with weapon-material albedo textures. The
/// glowing accents (muzzle rings, warheads, the lightning core) stay untextured
/// so they bloom in the dark.
pub(crate) fn viewmodel_parts(kind: WeaponKind) -> Vec<Part> {
    match kind {
        WeaponKind::Shotgun => vm_shotgun(),
        WeaponKind::SuperShotgun => vm_super_shotgun(),
        WeaponKind::Nailgun => vm_nailgun(),
        WeaponKind::Grenade => vm_grenade_launcher(),
        WeaponKind::Rocket => vm_rocket_launcher(),
        WeaponKind::Lightning => vm_lightning_gun(),
        WeaponKind::Whip => vm_whip(),
    }
}

/// First-person view-model: Shotgun.
fn vm_shotgun() -> Vec<Part> {
    let w = Color::WHITE;
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    let steel_e = LinearRgba::rgb(0.07, 0.08, 0.10);
    let wood_e = LinearRgba::rgb(0.06, 0.025, 0.008);
    let muzzle = rgb(0.95, 0.72, 0.22);
    let muzzle_e = LinearRgba::rgb(2.0, 1.2, 0.16);
    let bead = rgb(1.0, 0.32, 0.18);
    let bead_e = LinearRgba::rgb(2.6, 0.5, 0.22);
    vec![
        // gunmetal receiver — the main body, near the origin
        skin(cube(Vec3::new(0.16, 0.16, 0.30), Vec3::new(0.0, 0.0, 0.0), w, metal_e), WeaponTex::Gunmetal),
        // single steel barrel running forward (-Z)
        skin(tube_z(0.10, 0.50, Vec3::new(0.0, 0.045, -0.28), w, steel_e), WeaponTex::Steel),
        // tube magazine slung directly under the barrel
        skin(tube_z(0.07, 0.42, Vec3::new(0.0, -0.02, -0.26), w, metal_e), WeaponTex::Gunmetal),
        // sliding wood pump fore-grip wrapping the magazine tube
        skin(tube_z(0.12, 0.15, Vec3::new(0.0, -0.02, -0.30), w, wood_e), WeaponTex::Wood),
        // wood buttstock at the back (+Z)
        skin(cube(Vec3::new(0.10, 0.15, 0.20), Vec3::new(0.0, -0.02, 0.23), w, wood_e), WeaponTex::Wood),
        // wood pistol grip hanging down (-Y)
        skin(cube(Vec3::new(0.08, 0.20, 0.10), Vec3::new(0.0, -0.14, 0.05), w, wood_e), WeaponTex::Wood),
        // thin metal trigger guard under the receiver
        skin(cube(Vec3::new(0.07, 0.04, 0.14), Vec3::new(0.0, -0.10, -0.04), w, metal_e), WeaponTex::Gunmetal),
        // rear sight block on top of the receiver
        skin(cube(Vec3::new(0.06, 0.04, 0.05), Vec3::new(0.0, 0.10, 0.10), w, metal_e), WeaponTex::Gunmetal),
        // front sight post on top of the barrel near the muzzle
        skin(cube(Vec3::new(0.02, 0.06, 0.03), Vec3::new(0.0, 0.12, -0.46), w, metal_e), WeaponTex::Gunmetal),
        // glowing front-sight bead (untextured accent)
        ball(0.035, Vec3::new(0.0, 0.155, -0.46), bead, bead_e),
        // subtle muzzle ring at the tip (untextured glow accent)
        tube_z(0.12, 0.04, Vec3::new(0.0, 0.045, -0.52), muzzle, muzzle_e),
    ]
}

/// First-person view-model: Super Shotgun.
fn vm_super_shotgun() -> Vec<Part> {
    let w = Color::WHITE;
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    let brass_e = LinearRgba::rgb(0.28, 0.18, 0.04);
    let wood_e = LinearRgba::rgb(0.06, 0.025, 0.008);
    let steel_e = LinearRgba::rgb(0.06, 0.07, 0.1);
    let glow_c = rgb(0.95, 0.72, 0.22);
    let glow_e = LinearRgba::rgb(2.4, 1.4, 0.18);
    vec![
        // gunmetal receiver / breech body, sat near the origin
        skin(cube(Vec3::new(0.26, 0.18, 0.34), Vec3::new(0.0, 0.0, 0.04), w, metal_e), WeaponTex::Gunmetal),
        // raised breech block where the action breaks open to load shells
        skin(cube(Vec3::new(0.22, 0.1, 0.13), Vec3::new(0.0, 0.07, 0.1), w, metal_e), WeaponTex::Gunmetal),
        // twin fat brass barrels, side-by-side, muzzle toward -Z
        skin(tube_z(0.11, 0.52, Vec3::new(-0.07, 0.03, -0.24), w, brass_e), WeaponTex::Brass),
        skin(tube_z(0.11, 0.52, Vec3::new(0.07, 0.03, -0.24), w, brass_e), WeaponTex::Brass),
        // ventilated steel top rib running between the two barrels
        skin(cube(Vec3::new(0.2, 0.03, 0.5), Vec3::new(0.0, 0.09, -0.24), w, steel_e), WeaponTex::Steel),
        // break-action hinge cross-pin (its heads poke out each side)
        skin(tube_x(0.05, 0.3, Vec3::new(0.0, -0.02, -0.02), w, steel_e), WeaponTex::Steel),
        // wood butt-stock toward the player
        skin(cube(Vec3::new(0.13, 0.17, 0.22), Vec3::new(0.0, -0.04, 0.32), w, wood_e), WeaponTex::Wood),
        // wood pistol grip hanging down
        skin(cube(Vec3::new(0.1, 0.22, 0.12), Vec3::new(0.0, -0.16, 0.1), w, wood_e), WeaponTex::Wood),
        // trigger + under-receiver guard bar
        skin(cube(Vec3::new(0.02, 0.06, 0.02), Vec3::new(0.0, -0.12, 0.06), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.04, 0.02, 0.14), Vec3::new(0.0, -0.15, 0.06), w, metal_e), WeaponTex::Gunmetal),
        // rear sight notch on top of the breech
        skin(cube(Vec3::new(0.05, 0.05, 0.04), Vec3::new(0.0, 0.14, 0.15), w, metal_e), WeaponTex::Gunmetal),
        // bright glowing muzzle rings at the twin mouths (untextured glow)
        tube_z(0.13, 0.05, Vec3::new(-0.07, 0.03, -0.49), glow_c, glow_e),
        tube_z(0.13, 0.05, Vec3::new(0.07, 0.03, -0.49), glow_c, glow_e),
        // glowing front bead sight perched on the rib near the muzzle
        ball(0.035, Vec3::new(0.0, 0.11, -0.46), rgb(1.0, 0.85, 0.4), LinearRgba::rgb(2.2, 1.6, 0.5)),
    ]
}

/// First-person view-model: Nailgun.
fn vm_nailgun() -> Vec<Part> {
    let w = Color::WHITE;
    // A touch brighter than pitch-black so the gunmetal body reads in shadow.
    let metal_e = LinearRgba::rgb(0.13, 0.14, 0.18);
    let steel_e = LinearRgba::rgb(0.4, 0.45, 0.7);
    let nail = rgb(0.85, 0.9, 1.0);
    let nail_e = LinearRgba::rgb(1.2, 1.4, 2.0);
    let glow = rgb(0.7, 0.82, 1.0);
    let glow_e = LinearRgba::rgb(1.4, 1.9, 2.9);
    vec![
        // receiver — main gunmetal body near the origin
        skin(cube(Vec3::new(0.22, 0.2, 0.32), Vec3::new(0.0, 0.0, -0.02), w, metal_e), WeaponTex::Gunmetal),
        // top feed cover / rail on the receiver
        skin(cube(Vec3::new(0.16, 0.07, 0.3), Vec3::new(0.0, 0.11, -0.05), w, metal_e), WeaponTex::Gunmetal),
        // twin thin steel spike-barrels, spread apart so the pair reads as two
        skin(tube_z(0.06, 0.5, Vec3::new(-0.085, 0.05, -0.27), w, steel_e), WeaponTex::Steel),
        skin(tube_z(0.06, 0.5, Vec3::new(0.085, 0.05, -0.27), w, steel_e), WeaponTex::Steel),
        // clamp band holding the barrel pair together near the front
        skin(tube_x(0.05, 0.26, Vec3::new(0.0, 0.05, -0.44), w, metal_e), WeaponTex::Gunmetal),
        // nail tips peeking out of each barrel mouth (bright steel, untextured glow)
        spike_fwd(0.05, 0.12, Vec3::new(-0.085, 0.05, -0.57), nail, nail_e),
        spike_fwd(0.05, 0.12, Vec3::new(0.085, 0.05, -0.57), nail, nail_e),
        // big round drum magazine slung under the receiver
        skin(tube_z(0.28, 0.13, Vec3::new(0.0, -0.1, 0.0), w, metal_e), WeaponTex::Gunmetal),
        // glowing loaded-nail core on the drum face (untextured accent)
        tube_z(0.11, 0.05, Vec3::new(0.0, -0.1, 0.07), glow, glow_e),
        // feed chute bridging the drum up into the barrel breech
        skin(cube(Vec3::new(0.12, 0.13, 0.12), Vec3::new(0.0, -0.03, -0.13), w, metal_e), WeaponTex::Gunmetal),
        // pistol grip hanging down toward the player
        skin(cube(Vec3::new(0.09, 0.22, 0.1), Vec3::new(0.0, -0.16, 0.1), w, metal_e), WeaponTex::Gunmetal),
        // trigger guard bar linking grip to receiver
        skin(tube_x(0.03, 0.1, Vec3::new(0.0, -0.1, 0.05), w, metal_e), WeaponTex::Gunmetal),
        // front sight blade standing on the clamp band
        skin(cube(Vec3::new(0.02, 0.08, 0.03), Vec3::new(0.0, 0.11, -0.44), w, metal_e), WeaponTex::Gunmetal),
    ]
}

/// First-person view-model: Grenade Launcher.
fn vm_grenade_launcher() -> Vec<Part> {
    let w = Color::WHITE;
    let body = rgb(0.32, 0.52, 0.22);
    let body_e = LinearRgba::rgb(0.06, 0.2, 0.03);
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    let bore = rgb(0.13, 0.14, 0.16);
    let bore_e = LinearRgba::rgb(0.02, 0.02, 0.03);
    let glow = rgb(0.35, 0.95, 0.28);
    let glow_e = LinearRgba::rgb(0.4, 2.4, 0.18);
    vec![
        // green receiver / main body
        skin(cube(Vec3::new(0.22, 0.2, 0.3), Vec3::new(0.0, 0.0, 0.06), body, body_e), WeaponTex::Painted),
        // big revolver drum bulging below/around the receiver
        skin(tube_z(0.3, 0.16, Vec3::new(0.0, -0.05, 0.05), body, body_e), WeaponTex::Painted),
        // ring of chamber bores around the drum (reads as a cylinder of chambers from the rear)
        skin(tube_z(0.07, 0.2, Vec3::new(0.0, 0.05, 0.05), bore, bore_e), WeaponTex::Gunmetal),    // top firing chamber (behind barrel)
        skin(tube_z(0.07, 0.2, Vec3::new(0.087, 0.0, 0.05), bore, bore_e), WeaponTex::Gunmetal),
        skin(tube_z(0.07, 0.2, Vec3::new(-0.087, 0.0, 0.05), bore, bore_e), WeaponTex::Gunmetal),
        skin(tube_z(0.07, 0.2, Vec3::new(0.087, -0.1, 0.05), bore, bore_e), WeaponTex::Gunmetal),
        skin(tube_z(0.07, 0.2, Vec3::new(-0.087, -0.1, 0.05), bore, bore_e), WeaponTex::Gunmetal),
        skin(tube_z(0.07, 0.2, Vec3::new(0.0, -0.15, 0.05), bore, bore_e), WeaponTex::Gunmetal),
        // a live round glowing green in one chamber, peeking out the rear face
        ball(0.06, Vec3::new(0.087, -0.1, 0.15), glow, glow_e),
        // fat short barrel (gunmetal), muzzle toward -Z
        skin(tube_z(0.2, 0.44, Vec3::new(0.0, 0.03, -0.2), w, metal_e), WeaponTex::Gunmetal),
        // bright glowing green muzzle ring + loaded grenade at the mouth (untextured glow)
        tube_z(0.25, 0.06, Vec3::new(0.0, 0.03, -0.45), glow, glow_e),
        ball(0.16, Vec3::new(0.0, 0.03, -0.47), glow, glow_e),
        // top sights: rear notch on the receiver + front post near the muzzle
        skin(cube(Vec3::new(0.06, 0.07, 0.05), Vec3::new(0.0, 0.13, 0.13), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.03, 0.07, 0.04), Vec3::new(0.0, 0.15, -0.38), w, metal_e), WeaponTex::Gunmetal),
        // pistol grip hanging down/back
        skin(cube(Vec3::new(0.1, 0.22, 0.12), Vec3::new(0.0, -0.18, 0.17), body, body_e), WeaponTex::Painted),
        // trigger + guard bar under the receiver, in front of the grip
        skin(cube(Vec3::new(0.03, 0.07, 0.03), Vec3::new(0.0, -0.1, 0.11), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.03, 0.03, 0.14), Vec3::new(0.0, -0.14, 0.1), w, metal_e), WeaponTex::Gunmetal),
    ]
}

/// First-person view-model: Rocket Launcher.
fn vm_rocket_launcher() -> Vec<Part> {
    let w = Color::WHITE;
    // textured-body emissives (so they aren't pitch black in shadow)
    let tube_e = LinearRgba::rgb(0.06, 0.06, 0.08);
    // red painted casing / grip (tint multiplies the Painted sheet)
    let band = rgb(0.55, 0.16, 0.12);
    let band_e = LinearRgba::rgb(0.35, 0.05, 0.02);
    // glowing orange rocket poking out the muzzle (untextured, strong emissive)
    let head = rgb(1.0, 0.55, 0.15);
    let head_e = LinearRgba::rgb(3.2, 1.1, 0.18);
    // glowing exhaust throat in the rear venturi (untextured)
    let burn = rgb(0.95, 0.45, 0.15);
    let burn_e = LinearRgba::rgb(2.2, 0.8, 0.12);
    vec![
        // --- big gunmetal launch tube (muzzle toward -Z) ---
        skin(tube_z(0.27, 0.58, Vec3::new(0.0, 0.02, -0.04), w, tube_e), WeaponTex::Gunmetal),
        // red painted casing band near the rear of the tube
        skin(tube_z(0.31, 0.16, Vec3::new(0.0, 0.02, 0.08), band, band_e), WeaponTex::Painted),
        // rear exhaust / venturi ring + glowing throat (faces the player from 3/4 rear)
        skin(tube_z(0.32, 0.09, Vec3::new(0.0, 0.02, 0.27), w, tube_e), WeaponTex::Gunmetal),
        tube_z(0.17, 0.06, Vec3::new(0.0, 0.02, 0.30), burn, burn_e),
        // --- loaded rocket poking out the front: glowing body + warhead cone ---
        tube_z(0.13, 0.16, Vec3::new(0.0, 0.02, -0.41), head, head_e),
        spike_fwd(0.13, 0.15, Vec3::new(0.0, 0.02, -0.55), head, head_e),
        // --- top sight rail with front post + rear blade ---
        skin(cube(Vec3::new(0.05, 0.06, 0.3), Vec3::new(0.0, 0.18, -0.05), w, tube_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.04, 0.08, 0.04), Vec3::new(0.0, 0.21, -0.18), w, tube_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.08, 0.06, 0.04), Vec3::new(0.0, 0.21, 0.07), w, tube_e), WeaponTex::Gunmetal),
        // --- receiver / trigger housing bridging tube to grip ---
        skin(cube(Vec3::new(0.13, 0.12, 0.18), Vec3::new(0.0, -0.06, 0.04), w, tube_e), WeaponTex::Gunmetal),
        // trigger-guard bar
        skin(cube(Vec3::new(0.04, 0.03, 0.13), Vec3::new(0.0, -0.14, -0.04), w, tube_e), WeaponTex::Gunmetal),
        // --- red pistol grip hanging down ---
        skin(cube(Vec3::new(0.1, 0.24, 0.12), Vec3::new(0.0, -0.2, 0.05), band, band_e), WeaponTex::Painted),
    ]
}

/// First-person view-model: Lightning Gun.
fn vm_lightning_gun() -> Vec<Part> {
    let w = Color::WHITE;
    let body = rgb(0.22, 0.34, 0.6);
    let body_e = LinearRgba::rgb(0.07, 0.17, 0.42);
    let metal_e = LinearRgba::rgb(0.05, 0.05, 0.07);
    // Emissives are far lower than the ground-pickup lightning gun: a view-model
    // sits point-blank in front of the bloom-enabled camera, so the pickup's hot
    // values (core blue ~6.5) would blow out into a screen-filling white disc.
    let core = rgb(0.65, 0.92, 1.0);
    let core_e = LinearRgba::rgb(0.25, 0.9, 2.0);
    let ring = rgb(0.5, 0.86, 1.0);
    let ring_e = LinearRgba::rgb(0.2, 0.8, 1.8);
    let prong = rgb(0.82, 0.88, 0.97);
    let prong_e = LinearRgba::rgb(0.3, 0.8, 1.6);
    let arc = rgb(0.9, 0.97, 1.0);
    let arc_e = LinearRgba::rgb(0.5, 1.2, 2.4);
    vec![
        // --- blue painted receiver body (main bulk near origin) ---
        skin(cube(Vec3::new(0.22, 0.2, 0.32), Vec3::new(0.0, 0.0, 0.05), body, body_e), WeaponTex::Painted),
        // top sight rail + rear notch
        skin(cube(Vec3::new(0.05, 0.05, 0.2), Vec3::new(0.0, 0.13, 0.04), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.07, 0.04, 0.04), Vec3::new(0.0, 0.15, 0.13), w, metal_e), WeaponTex::Gunmetal),
        // gunmetal bolt pin through the receiver (mechanical detail)
        skin(tube_x(0.05, 0.26, Vec3::new(0.0, 0.0, 0.04), w, metal_e), WeaponTex::Gunmetal),
        // --- barrel shroud (blue painted) running forward (-Z) ---
        skin(tube_z(0.14, 0.4, Vec3::new(0.0, 0.02, -0.18), body, body_e), WeaponTex::Painted),
        // inner gunmetal barrel poking through the shroud to the emitter
        skin(tube_z(0.09, 0.46, Vec3::new(0.0, 0.02, -0.21), w, metal_e), WeaponTex::Gunmetal),
        // --- three glowing cyan coil rings around the shroud (untextured glow) ---
        tube_z(0.19, 0.04, Vec3::new(0.0, 0.02, -0.06), ring, ring_e),
        tube_z(0.19, 0.04, Vec3::new(0.0, 0.02, -0.19), ring, ring_e),
        tube_z(0.19, 0.04, Vec3::new(0.0, 0.02, -0.32), ring, ring_e),
        // --- emitter core ball at the muzzle ---
        ball(0.14, Vec3::new(0.0, 0.02, -0.46), core, core_e),
        // --- two emitter prongs extending forward past the core ---
        tube_z(0.055, 0.3, Vec3::new(-0.1, 0.02, -0.52), prong, prong_e),
        tube_z(0.055, 0.3, Vec3::new(0.1, 0.02, -0.52), prong, prong_e),
        // prong tip caps
        ball(0.06, Vec3::new(-0.1, 0.02, -0.67), arc, arc_e),
        ball(0.06, Vec3::new(0.1, 0.02, -0.67), arc, arc_e),
        // arc-gap crackle: a thin bright bar leaping the gap between the prong tips
        tube_x(0.025, 0.2, Vec3::new(0.0, 0.02, -0.6), arc, arc_e),
        ball(0.05, Vec3::new(0.0, 0.02, -0.6), arc, arc_e),
        // --- gunmetal pistol grip (hangs down) + trigger ---
        skin(cube(Vec3::new(0.09, 0.22, 0.11), Vec3::new(0.0, -0.13, 0.03), w, metal_e), WeaponTex::Gunmetal),
        skin(cube(Vec3::new(0.03, 0.08, 0.04), Vec3::new(0.0, -0.06, -0.05), w, metal_e), WeaponTex::Gunmetal),
    ]
}

/// First-person view-model: Whip.
fn vm_whip() -> Vec<Part> {
    let w = Color::WHITE;
    // dark brown leather tint for the lash so it reads distinct from the lighter wood grip
    let leather = rgb(0.5, 0.32, 0.18);
    let wood_e = LinearRgba::rgb(0.05, 0.02, 0.008);
    let lash_e = LinearRgba::rgb(0.04, 0.01, 0.0);
    // Muted so the steel pommel reads as metal, not a glowing white orb.
    let steel_e = LinearRgba::rgb(0.12, 0.14, 0.18);
    let brass_e = LinearRgba::rgb(0.25, 0.16, 0.03);
    vec![
        // --- handle (held in the hand) -------------------------------------
        // wood grip, gripped along Z, butt toward the player (+Z)
        skin(tube_z(0.09, 0.30, Vec3::new(0.0, -0.02, 0.09), w, wood_e), WeaponTex::Wood),
        // steel pommel ball capping the butt (sized to the handle, not oversized)
        skin(ball(0.095, Vec3::new(0.0, -0.02, 0.23), w, steel_e), WeaponTex::Steel),
        // brass collar / guard where the lash attaches to the front of the grip
        skin(tube_z(0.125, 0.06, Vec3::new(0.0, -0.02, -0.07), w, brass_e), WeaponTex::Brass),

        // --- leather lash: tapering beaded coil that dips down then curls up -
        // straight base section forward of the collar
        skin(tube_z(0.082, 0.12, Vec3::new(0.005, -0.05, -0.15), leather, lash_e), WeaponTex::Wood),
        // beaded coil, diameters shrinking toward the tip, y dips to a hanging
        // low point (~-0.16) then rises back up toward the muzzle end
        skin(ball(0.075, Vec3::new(0.01, -0.10, -0.24), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.067, Vec3::new(0.02, -0.14, -0.30), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.059, Vec3::new(0.03, -0.16, -0.36), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.051, Vec3::new(0.03, -0.15, -0.42), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.043, Vec3::new(0.02, -0.12, -0.47), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.036, Vec3::new(0.01, -0.08, -0.51), leather, lash_e), WeaponTex::Wood),
        skin(ball(0.030, Vec3::new(0.0, -0.05, -0.54), leather, lash_e), WeaponTex::Wood),

        // --- frayed metal tip + faint spark accent -------------------------
        // small steel cracker tip, apex pointing forward (-Z)
        skin(spike_fwd(0.038, 0.06, Vec3::new(0.0, -0.045, -0.575), w, steel_e), WeaponTex::Steel),
        // tiny untextured glow at the very tip (the whip's crack point)
        ball(0.022, Vec3::new(0.0, -0.045, -0.60), rgb(1.0, 0.85, 0.55), LinearRgba::rgb(2.2, 1.3, 0.5)),
    ]
}

/// Insert inventory on the player and attach a view-model to the camera.
/// Runs in the OnEnter(Playing) chain right after the player is spawned.
pub fn setup_player_weapons(
    mut commands: Commands,
    run: Res<RunState>,
    q_player: Query<Entity, With<Player>>,
    q_cam: Query<Entity, With<PlayerCamera>>,
) {
    if let Ok(pe) = q_player.single() {
        if run.carry_inventory {
            // Advancing between levels: keep weapons, ammo, health and armor.
            let c = &run.carry;
            commands.entity(pe).insert((
                Inventory { owned: c.owned, ammo: c.ammo, current: WeaponKind::ALL[c.current.min(6)], cooldown: 0.0 },
                Health { current: c.health.max(1.0), max: 100.0, dead: false },
                Armor { points: c.armor_points, absorb: c.armor_absorb },
            ));
        } else {
            commands.entity(pe).insert(Inventory::default());
        }
        // Every player carries grapnel state (the Whip's alt-fire rope).
        commands.entity(pe).insert(Grapple::default());
    }
    if let Ok(ce) = q_cam.single() {
        // The view-model is an empty root parented to the camera, held in the
        // lower-right. Its weapon parts are spawned by `update_viewmodel` on the
        // first frame (built: None forces an initial build for the current weapon).
        commands.entity(ce).with_children(|p| {
            p.spawn((
                Transform::from_xyz(0.32, -0.3, -0.75),
                Visibility::default(),
                ViewModel { built: None },
            ));
        });
    }
}

fn switch_weapon(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut q: Query<&mut Inventory>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Ok(mut inv) = q.single_mut() else { return };
    let keymap = [
        KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3,
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7,
    ];
    let mut target = None;
    for (i, k) in keymap.iter().enumerate() {
        if keys.just_pressed(*k) && inv.owned[i] {
            target = Some(WeaponKind::ALL[i]);
        }
    }
    // Mouse wheel cycles through owned weapons.
    if scroll.delta.y.abs() > 0.1 {
        let dir = if scroll.delta.y > 0.0 { 1i32 } else { -1 };
        let mut idx = inv.current.index() as i32;
        let n = WeaponKind::ALL.len() as i32;
        for _ in 0..n {
            idx = (idx + dir).rem_euclid(n);
            if inv.owned[idx as usize] {
                target = Some(WeaponKind::ALL[idx as usize]);
                break;
            }
        }
    }
    if let Some(w) = target {
        if w != inv.current {
            inv.current = w;
            inv.cooldown = inv.cooldown.max(0.12);
            sfx.write(Sfx::global(Sound::PickupAmmo));
        }
    }
}

/// The message writers `fire_weapon` emits through, bundled into one SystemParam
/// so the system stays under Bevy's 16-param ceiling.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FireWriters<'w> {
    dmg: MessageWriter<'w, DamageEvent>,
    impact: MessageWriter<'w, ImpactEvent>,
    whip_hit: MessageWriter<'w, WhipHit>,
    sfx: MessageWriter<'w, Sfx>,
    shake: MessageWriter<'w, ScreenShake>,
    /// Brush hits feeding the resonance system (feature 29).
    brush: MessageWriter<'w, BrushStrike>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fire_weapon(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    colliders: Res<WorldColliders>,
    limb_boxes: Res<LimbBoxes>,
    gfx: Res<GfxAssets>,
    active_mount: Res<ActiveMount>,
    active_gunner: Res<ActiveGunner>,
    mut rng_state: Local<u32>,
    mut nail_alt: Local<bool>,
    mut q_player: Query<(Entity, &mut Inventory)>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    mut w: FireWriters,
    mut kick: ResMut<ViewKick>,
    mut parry: ResMut<WhipParry>,
) {
    let dt = time.delta_secs();
    let Ok((pe, mut inv)) = q_player.single_mut() else { return };
    inv.cooldown = (inv.cooldown - dt).max(0.0);

    // While mounted on an Ogre your guns are stowed — the Ogre's chainsaw (run by
    // `mount::mount_fire`) is your only weapon. Suppress BOTH the normal-fire path
    // and the Lodestone alt-fire by bailing before either is reached.
    if active_mount.0.is_some() {
        return;
    }
    // Likewise while crewing the truck's pintle cannon: the deck gun (run by
    // `vehicle::gun_fire`) takes the left mouse, so the hand weapon is stowed and
    // both its primary and alt-fire are suppressed here.
    if active_gunner.0.is_some() {
        return;
    }

    // Alt-fire: Grenade Launcher right-click lobs a Lodestone gravity well.
    if inv.current == WeaponKind::Grenade && mouse.just_pressed(MouseButton::Right) {
        let ai = inv.current.ammo();
        const LODESTONE_COST: i32 = 1;
        if inv.cooldown <= 0.0 && inv.ammo[ai] >= LODESTONE_COST {
            if let Ok(cam_gt) = cam.single() {
                // Cooldown >= the fuse so a second well can't normally be live while
                // the first is still pulling — stacked wells sum their inward impulse
                // and would slingshot monsters through the clump and scatter them.
                inv.cooldown = 2.0;
                inv.ammo[ai] -= LODESTONE_COST;
                let origin = cam_gt.translation();
                let forward = cam_gt.forward().as_vec3();
                // Same gun-muzzle offset as primary fire (see below) so the well
                // ball leaves the lower-right, not through your face.
                let muzzle = origin + forward * 0.6 + cam_gt.right().as_vec3() * 0.16
                    - cam_gt.up().as_vec3() * 0.18;
                crate::projectiles::spawn_lodestone(&mut commands, &gfx, muzzle, forward, pe);
                w.sfx.write(Sfx::at(Sound::Lodestone, muzzle));
                w.shake.write(ScreenShake { amount: 0.1 });
                kick.amount = (kick.amount + 0.5).min(1.5);
            }
        }
        return; // RMB consumed by alt-fire; do not fall through to primary fire
    }

    if !mouse.pressed(MouseButton::Left) || inv.cooldown > 0.0 {
        return;
    }
    let stats = inv.current.stats();
    let ai = inv.current.ammo();
    if inv.ammo[ai] < stats.cost {
        return;
    }
    let Ok(cam_gt) = cam.single() else { return };

    inv.cooldown = stats.cooldown;
    inv.ammo[ai] -= stats.cost;

    let origin = cam_gt.translation();
    let forward = cam_gt.forward().as_vec3();
    // Projectiles, the muzzle flash and the whip draw leave from the gun muzzle —
    // offset down-and-right of the eye, not dead-centre on the crosshair ray — so
    // the slow "balls" don't spawn on your eyeline and fly through your view.
    // Hitscan still traces from `origin` (the eye) so it hits exactly the crosshair.
    let muzzle = origin + forward * 0.6 + cam_gt.right().as_vec3() * 0.16
        - cam_gt.up().as_vec3() * 0.18;

    // Melee swings have no muzzle flash (it's a whip crack, not a gunshot).
    let melee = matches!(stats.mode, Mode::Melee { .. });
    if !melee {
        spawn_muzzle_flash(&mut commands, &gfx, muzzle);
    }
    w.sfx.write(Sfx::global(stats.sound));
    w.shake.write(ScreenShake { amount: stats.shake });
    kick.amount = (kick.amount + stats.kick).min(1.5);

    if *rng_state == 0 {
        *rng_state = 0x1234_5678;
    }
    let mut rand = || {
        let mut x = *rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *rng_state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    };

    match stats.mode {
        Mode::Hitscan { pellets, spread, damage } => {
            for _ in 0..pellets {
                let d = spread_dir(forward, spread, rand(), rand());
                if let Some((slot, point)) = hitscan(&mut commands, &colliders, &targets, &limb_boxes, &gfx, origin, d, damage, pe, false, &mut w.dmg, &mut w.impact) {
                    // A struck brush may ring (resonance telegraph) — one-shot.
                    w.brush.write(BrushStrike { slot, point, beam: false });
                }
            }
        }
        Mode::Beam { damage } => {
            let d = spread_dir(forward, 0.005, rand(), rand());
            if let Some((slot, point)) = hitscan(&mut commands, &colliders, &targets, &limb_boxes, &gfx, origin, d, damage, pe, true, &mut w.dmg, &mut w.impact) {
                // The beam is dwelling on this brush this frame — feeds the sweep.
                w.brush.write(BrushStrike { slot, point, beam: true });
            }
        }
        Mode::Projectile(ProjKind::Nail) => {
            // Twin-barrel alternating nail stream.
            let right = forward.cross(Vec3::Y).normalize_or_zero();
            *nail_alt = !*nail_alt;
            let off = if *nail_alt { 0.13 } else { -0.13 };
            spawn_projectile(&mut commands, &gfx, ProjKind::Nail, muzzle + right * off, forward, true, Some(pe));
        }
        Mode::Projectile(kind) => {
            spawn_projectile(&mut commands, &gfx, kind, muzzle, forward, true, Some(pe));
        }
        Mode::Melee { damage, range, knockback } => {
            let (end, hit) = melee_strike(&colliders, &targets, &limb_boxes, origin, forward, range, damage, knockback, pe, &mut w.dmg, &mut w.impact);
            draw_whip(&mut commands, &gfx, muzzle, end);
            if inv.current == WeaponKind::Whip {
                // Open the parry window (feature 39): for the next brief moment the
                // Whip's SAME melee arc (origin/dir/range captured here) is swept
                // against enemy projectiles by `projectiles::whip_parry`. Reusing the
                // melee geometry means the parry tracks the visible lash, not a
                // second hitbox.
                parry.open(origin, forward, range, pe);
                // A Whip lash that connects with a monster flags it for the mount
                // system (which opens a Mountable window on Ogre targets).
                if let Some(target) = hit {
                    w.whip_hit.write(WhipHit { target });
                }
            }
        }
    }
}

/// Short-range whip lash: hit the nearest monster in front (within `range`,
/// not behind a wall), dealing `damage` and a strong knockback that flings the
/// monster away from the player and a little upward. Returns the lash endpoint
/// (the hit point, or the full reach on a miss) for the visual, plus the monster
/// entity struck (if any) so the caller can flag it (e.g. a mountable Ogre).
#[allow(clippy::too_many_arguments)]
fn melee_strike(
    colliders: &WorldColliders,
    targets: &Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    limb_boxes: &LimbBoxes,
    origin: Vec3,
    dir: Vec3,
    range: f32,
    damage: f32,
    knockback: f32,
    source: Entity,
    dmg: &mut MessageWriter<DamageEvent>,
    impact: &mut MessageWriter<ImpactEvent>,
) -> (Vec3, Option<Entity>) {
    // A wall between player and target stops the lash short.
    let wall_t = raycast_world(origin, dir, range, &colliders.solids).map(|(t, _, _)| t).unwrap_or(range);
    // Prefer a specific bone (padded reach so a glancing swing still connects);
    // fall back to the broad body box (a padded torso swing) when no bone is hit.
    let limb = nearest_limb_hit(origin, dir, wall_t, &limb_boxes.boxes, 0.2);
    let mut body_t = wall_t;
    let mut body_hit: Option<(Entity, Vec3, Vec3)> = None;
    for (e, gt, hb, fac) in targets.iter() {
        if *fac != Faction::Monster {
            continue;
        }
        let b = Aabb::from_center_half(gt.translation(), hb.half + Vec3::splat(0.2));
        if let Some((t, n)) = ray_aabb(origin, dir, body_t, &b) {
            body_t = t;
            body_hit = Some((e, origin + dir * t, n));
        }
    }
    // Push horizontally away from the player plus an upward lift, so the monster
    // flies back regardless of the aim pitch.
    let horiz = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    let push = horiz * knockback + Vec3::Y * knockback * 0.36;
    if let Some((e, g, t, n)) = limb {
        dmg.write(DamageEvent::limb(e, damage, Some(source), push, g));
        impact.write(ImpactEvent { pos: origin + dir * t, normal: n, blood: true });
        (origin + dir * t.min(range), Some(e))
    } else if let Some((e, pt, n)) = body_hit {
        dmg.write(DamageEvent::body(e, damage, Some(source), push));
        impact.write(ImpactEvent { pos: pt, normal: n, blood: true });
        (origin + dir * body_t.min(range), Some(e))
    } else {
        (origin + dir * wall_t.min(range), None)
    }
}

/// Draw the whip lash as a couple of thin segments that droop slightly, with a
/// brief lifetime so it reads as a crack of the whip.
fn draw_whip(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3) {
    let len = a.distance(b).max(0.05);
    let dir = (b - a).normalize_or_zero();
    let right = dir.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(dir).normalize_or_zero();
    let mid = a.lerp(b, 0.5) - up * (len * 0.1);
    whip_segment(commands, gfx, a, mid, 0.04);
    whip_segment(commands, gfx, mid, b, 0.03);
}

fn whip_segment(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3, w: f32) {
    let mid = (a + b) * 0.5;
    let len = a.distance(b).max(0.02);
    let dir = (b - a).normalize_or_zero();
    let tf = Transform::from_translation(mid)
        .looking_to(dir, Vec3::Y)
        .with_scale(Vec3::new(w, w, len));
    commands.spawn((
        Mesh3d(gfx.unit_cube.clone()),
        MeshMaterial3d(gfx.muzzle.clone()),
        tf,
        Lifetime(0.07),
        LevelEntity,
    ));
}

/// Acquire / maintain / release the grapnel latch (the discrete edges). Runs
/// after `player_look` + `vehicle_activate` and before `player_move`, so a latch
/// taken this frame is honored by the same-frame rope constraint and a same-frame
/// vehicle mount forces a detach first. The continuous swing math lives in
/// `player_move` via [`rope_constrain`].
#[allow(clippy::too_many_arguments)]
fn grapple_cast(
    mouse: Res<ButtonInput<MouseButton>>,
    colliders: Res<WorldColliders>,
    active: Res<ActiveVehicle>,
    gunner: Res<ActiveGunner>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<(&Transform, &Inventory, &mut Grapple), With<Player>>,
    mut sfx: MessageWriter<Sfx>,
    mut commands: Commands,
    gfx: Res<GfxAssets>,
) {
    let Ok((ptf, inv, mut grap)) = q.single_mut() else { return };
    let center = ptf.translation;

    // Want to be (or stay) latched: RMB held, Whip equipped, on foot — and NOT
    // crewing the pintle gun (the hand weapon, grapnel included, is stowed while
    // you're behind the cannon, matching the `ActiveGunner` guards in `fire_weapon`
    // / `update_viewmodel`; a hook held when you board the gun detaches here).
    let want = mouse.pressed(MouseButton::Right)
        && inv.current == WeaponKind::Whip
        && active.0.is_none()
        && gunner.0.is_none();

    // --- maintain / detach an existing hook --------------------------------
    if let Some(hook) = grap.hook.as_ref() {
        let mut detach = !want;
        // Flung clean out of range.
        if !detach && hook.anchor.distance(center) > GRAPPLE_RANGE * 1.15 {
            detach = true;
        }
        // Line of sight to the anchor broken by geometry (also covers a truck
        // driving out from under the anchor). The anchor sits on a solid, so test
        // to a point pulled slightly off the surface toward the player.
        if !detach {
            let pulled = hook.anchor + (center - hook.anchor).normalize_or_zero() * 0.2;
            if !line_of_sight(center, pulled, &colliders.solids) {
                detach = true;
            }
        }
        // Stuck watchdog (incremented by player_move while pinned taut + slow).
        if !detach && hook.stuck_frames > GRAPPLE_STUCK_FRAMES {
            detach = true;
        }
        if detach {
            grap.hook = None;
        }
        return; // already latched (or just detached) — never re-cast this frame
    }

    // --- acquire a new hook, only on the press edge ------------------------
    if !want || !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Ok(cam_gt) = cam.single() else { return };
    let origin = cam_gt.translation();
    let dir = cam_gt.forward().as_vec3();
    if let Some((t, point, normal)) = raycast_world(origin, dir, GRAPPLE_RANGE, &colliders.solids) {
        if t < GRAPPLE_EPS {
            return; // latched a face we're already flush against
        }
        let anchor = point + normal * 0.05; // nudge off the surface
        let len = (anchor - center).length().max(GRAPPLE_MIN_LEN);
        grap.hook = Some(GrappleHook { anchor, len, stuck_frames: 0 });
        sfx.write(Sfx::global(Sound::Whip)); // reuse the existing crack
        draw_whip(&mut commands, &gfx, origin + dir * 0.6, point); // one-shot lash
    }
}

/// Spawn the looping taut-rope sound on first latch, despawn it on release, and
/// while latched scrub its volume toward the swing speed. Mirrors the truck's
/// `vehicle_audio` three-state `AudioSink` handling (the sink goes live a frame
/// or two after the entity spawns).
fn grapple_audio_lifecycle(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<Sounds>,
    q_player: Query<(&Player, &Grapple)>,
    mut q_rope: Query<Option<&mut AudioSink>, With<RopeSound>>,
    loops: Query<Entity, With<RopeSound>>,
) {
    let dt = time.delta_secs();
    let latched = q_player.single().map(|(_, g)| g.hook.is_some()).unwrap_or(false);
    if !latched {
        for e in &loops {
            commands.entity(e).despawn();
        }
        return;
    }
    // Tension volume scrubs with swing speed (idle hum -> straining cable).
    let speed = q_player.single().map(|(p, _)| p.vel.length()).unwrap_or(0.0);
    let target = 0.12 + 0.4 * (speed / 12.0).clamp(0.0, 1.0);
    match q_rope.single_mut() {
        Ok(Some(mut sink)) => {
            let k = 1.0 - (-8.0 * dt).exp();
            let cv = sink.volume().to_linear();
            sink.set_volume(Volume::Linear(cv + (target - cv) * k));
        }
        Ok(None) => {} // entity spawned, sink not live yet
        Err(_) => {
            commands.spawn((
                AudioPlayer::new(sounds.get(Sound::RopeTaut)),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
                RopeSound,
                Name::new("RopeSound"),
            ));
        }
    }
}

/// Keep one persistent rope-line mesh stretched from the whip muzzle to the
/// anchor while latched; despawn it when the rope drops.
fn update_rope_visual(
    mut commands: Commands,
    gfx: Res<GfxAssets>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_player: Query<&Grapple, With<Player>>,
    mut q_vis: Query<&mut Transform, With<RopeVisual>>,
    existing: Query<Entity, With<RopeVisual>>,
) {
    let hook = q_player.single().ok().and_then(|g| g.hook.clone());
    let Some(hook) = hook else {
        for e in &existing {
            commands.entity(e).despawn();
        }
        return;
    };
    let Ok(cam_gt) = cam.single() else { return };
    let muzzle = cam_gt.translation() + cam_gt.forward().as_vec3() * 0.6;
    let anchor = hook.anchor;
    let mid = (muzzle + anchor) * 0.5;
    let len = muzzle.distance(anchor).max(0.05);
    let dir = (anchor - muzzle).normalize_or_zero();
    // Pick an up vector not parallel to the rope so a near-vertical (ceiling)
    // grapple doesn't make `looking_to` degenerate.
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let tf = Transform::from_translation(mid)
        .looking_to(dir, up)
        .with_scale(Vec3::new(0.03, 0.03, len));
    if let Ok(mut t) = q_vis.single_mut() {
        *t = tf;
    } else {
        commands.spawn((
            Mesh3d(gfx.unit_cube.clone()),
            MeshMaterial3d(gfx.muzzle.clone()),
            tf,
            RopeVisual,
            LevelEntity,
        ));
    }
}

/// On leaving `Playing` (death / victory / level change): tear down both halves
/// of an active grapple — silence the looping rope sound so it doesn't drone over
/// the death/victory screen, and despawn the rope-line mesh so it doesn't hang
/// frozen behind the (semi-transparent) overlay. The Playing-gated systems that
/// normally do this can't run once the state has flipped, so it must happen here.
fn cleanup_grapple(
    mut commands: Commands,
    loops: Query<Entity, With<RopeSound>>,
    visuals: Query<Entity, With<RopeVisual>>,
) {
    for e in &loops {
        commands.entity(e).despawn();
    }
    for e in &visuals {
        commands.entity(e).despawn();
    }
}

fn spread_dir(forward: Vec3, spread: f32, rx: f32, ry: f32) -> Vec3 {
    let up = if forward.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let right = forward.cross(up).normalize_or_zero();
    let up2 = right.cross(forward).normalize_or_zero();
    (forward + right * (rx * spread) + up2 * (ry * spread)).normalize_or_zero()
}

/// Fire one hitscan ray. Returns the world collider slot the ray TERMINATED on
/// (plus the hit point) when it ended on geometry rather than a monster — so the
/// caller can tie that slot to a resonant brush (feature 29: beam dwell + the
/// struck-brush ring telegraph). `None` when the ray hit a monster or nothing.
#[allow(clippy::too_many_arguments)]
fn hitscan(
    commands: &mut Commands,
    colliders: &WorldColliders,
    targets: &Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    limb_boxes: &LimbBoxes,
    gfx: &GfxAssets,
    origin: Vec3,
    dir: Vec3,
    damage: f32,
    source: Entity,
    beam: bool,
    dmg: &mut MessageWriter<DamageEvent>,
    impact: &mut MessageWriter<ImpactEvent>,
) -> Option<(usize, Vec3)> {
    let max = 200.0;
    // Nearest wall first, so monster/limb hits only count in front of it.
    let wall = raycast_world_indexed(origin, dir, max, &colliders.solids);
    let wall_t = wall.map(|(_, t, _, _)| t).unwrap_or(max);
    // Prefer a specific bone box (delivers the per-bone "lead the head" skill);
    // every awake, in-range monster's silhouette is covered by bone boxes.
    let limb = nearest_limb_hit(origin, dir, wall_t, &limb_boxes.boxes, 0.0);
    // Broad body box: the fallback when no bone box is hit (a torso-gap shot, or a
    // sleeping/distant monster that contributes no limb boxes) — no hit regression.
    let mut body_t = wall_t;
    let mut body_hit: Option<(Entity, Vec3, Vec3)> = None;
    for (e, gt, hb, fac) in targets.iter() {
        if *fac != Faction::Monster {
            continue;
        }
        let b = Aabb::from_center_half(gt.translation(), hb.half);
        if let Some((t, n)) = ray_aabb(origin, dir, body_t, &b) {
            body_t = t;
            body_hit = Some((e, origin + dir * t, n));
        }
    }

    let hit_t = if let Some((_, _, t, _)) = limb {
        t
    } else if body_hit.is_some() {
        body_t
    } else {
        wall_t
    };
    if beam {
        draw_beam(commands, gfx, origin, origin + dir * hit_t.min(max));
    }

    if let Some((e, g, t, n)) = limb {
        dmg.write(DamageEvent::limb(e, damage, Some(source), dir * 1.5, g));
        impact.write(ImpactEvent { pos: origin + dir * t, normal: n, blood: true });
        None
    } else if let Some((e, pt, n)) = body_hit {
        dmg.write(DamageEvent::body(e, damage, Some(source), dir * 1.5));
        impact.write(ImpactEvent { pos: pt, normal: n, blood: true });
        None
    } else if let Some((slot, _, pt, n)) = wall {
        impact.write(ImpactEvent { pos: pt, normal: n, blood: false });
        Some((slot, pt))
    } else {
        None
    }
}

fn draw_beam(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3) {
    let dir = (b - a).normalize_or_zero();
    let len = a.distance(b).max(0.05);
    let right = dir.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(dir).normalize_or_zero();
    // Bright core.
    beam_segment(commands, gfx, a, b, 0.06);
    // Jittered crackle strands.
    let h = (len * 97.0) as i32 as u32 ^ 0x2545;
    let j1 = ((h & 7) as f32 - 3.5) * 0.025;
    let j2 = (((h >> 3) & 7) as f32 - 3.5) * 0.025;
    let m1 = a.lerp(b, 0.33) + right * j1 + up * j2;
    let m2 = a.lerp(b, 0.66) + right * j2 - up * j1;
    beam_segment(commands, gfx, a, m1, 0.03);
    beam_segment(commands, gfx, m1, m2, 0.03);
    beam_segment(commands, gfx, m2, b, 0.03);
}

fn beam_segment(commands: &mut Commands, gfx: &GfxAssets, a: Vec3, b: Vec3, w: f32) {
    let mid = (a + b) * 0.5;
    let len = a.distance(b).max(0.02);
    let dir = (b - a).normalize_or_zero();
    let tf = Transform::from_translation(mid)
        .looking_to(dir, Vec3::Y)
        .with_scale(Vec3::new(w, w, len));
    commands.spawn((
        Mesh3d(gfx.unit_cube.clone()),
        MeshMaterial3d(gfx.plasma.clone()),
        tf,
        Lifetime(0.05),
        LevelEntity,
    ));
}

fn update_viewmodel(
    mut commands: Commands,
    time: Res<Time>,
    mut kick: ResMut<ViewKick>,
    vis: Res<WeaponVis>,
    gfx: Res<GfxAssets>,
    active_gunner: Res<ActiveGunner>,
    q_player: Query<(&Inventory, &Player)>,
    mut q_vm: Query<(Entity, &mut Transform, &mut Visibility, &mut ViewModel, Option<&Children>)>,
) {
    let dt = time.delta_secs();
    kick.amount = (kick.amount - dt * 6.0).max(0.0);
    let Ok((inv, player)) = q_player.single() else { return };
    let Ok((root, mut tf, mut vmvis, mut vm, children)) = q_vm.single_mut() else { return };
    // Stow the first-person hand weapon entirely while crewing the pintle cannon —
    // the deck gun is the active weapon, so hide the view-model and skip its bob/kick.
    let stow = active_gunner.0.is_some();
    let want = if stow { Visibility::Hidden } else { Visibility::Inherited };
    if *vmvis != want {
        *vmvis = want;
    }
    if stow {
        return;
    }
    // Rebuild the model whenever the held weapon changes: despawn the old parts and
    // spawn the new weapon's cached parts. Each weapon is a full multi-part gun.
    if vm.built != Some(inv.current) {
        if let Some(children) = children {
            for &c in children {
                commands.entity(c).despawn();
            }
        }
        build_viewmodel(&mut commands, root, &vis, &gfx, inv.current.index());
        vm.built = Some(inv.current);
    }
    let bx = (player.bob).sin() * 0.012;
    let by = ((player.bob * 2.0).sin()).abs() * 0.012;
    let base = Vec3::new(0.32, -0.3, -0.75);
    tf.translation = base + Vec3::new(bx, by, kick.amount.min(1.0) * 0.18);
    tf.rotation = Quat::from_rotation_x(-kick.amount.min(1.0) * 0.2);
}

#[cfg(test)]
mod tests {
    use super::*;

    // The rope is a one-sided distance constraint: slack does nothing, taut
    // cancels only the OUTWARD radial velocity. `len = 4.9` puts a point at
    // distance 5.0 firmly past `len + GRAPPLE_SLACK`, so the taut branch fires.

    /// At (or inside) the anchor the constraint is a finite no-op (NaN guard).
    #[test]
    fn at_anchor_is_noop_and_finite() {
        let v = rope_constrain(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0), Vec3::ZERO, 5.0);
        assert!(v.is_finite());
        assert_eq!(v, Vec3::new(3.0, 0.0, 0.0));
    }

    /// Within the rope length, velocity is untouched (a rope only pulls).
    #[test]
    fn slack_is_free() {
        let v = rope_constrain(Vec3::new(0.0, -3.0, 0.0), Vec3::new(0.0, -9.0, 0.0), Vec3::ZERO, 5.0);
        assert_eq!(v, Vec3::new(0.0, -9.0, 0.0));
    }

    /// Taut: the outward radial part is removed, the tangential part survives —
    /// this is exactly what lets air-strafe pump the swing wider.
    #[test]
    fn taut_cancels_outward_keeps_tangential() {
        let v = rope_constrain(Vec3::new(5.0, 0.0, 0.0), Vec3::new(4.0, 6.0, 0.0), Vec3::ZERO, 4.9);
        assert!(v.x.abs() < 1e-5, "outward x should be cancelled, got {}", v.x);
        assert!((v.y - 6.0).abs() < 1e-5, "tangential y should survive, got {}", v.y);
    }

    /// Taut but moving inward (a fall toward the anchor, or reel-in): free.
    #[test]
    fn inward_is_free() {
        let v = rope_constrain(Vec3::new(5.0, 0.0, 0.0), Vec3::new(-4.0, 0.0, 0.0), Vec3::ZERO, 4.9);
        assert_eq!(v.x, -4.0);
    }

    /// The projection only ever subtracts — it can never add energy (no runaway).
    #[test]
    fn never_adds_energy() {
        let inp = Vec3::new(4.0, 6.0, 2.0);
        let v = rope_constrain(Vec3::new(5.0, 0.0, 0.0), inp, Vec3::ZERO, 4.9);
        assert!(v.length() <= inp.length() + 1e-5);
    }

    /// Integration test of the real swing loop (the `player_move` velocity
    /// pipeline minus collision): gravity → rope_constrain → integrate, with the
    /// production GRAVITY and a fixed 60 Hz step. A pushed-from-the-bottom rope
    /// must arc to BOTH sides (a genuine pendulum, not a stuck yo-yo) while the
    /// rope holds the radius (no Euler blow-up / NaN). This is the math the
    /// screenshot can't show.
    #[test]
    fn swings_like_a_pendulum_without_blowing_up() {
        use crate::common::tune::GRAVITY;
        let anchor = Vec3::ZERO;
        let len = 8.0;
        let mut pos = Vec3::new(0.0, -len, 0.0); // hanging straight down, taut
        let mut vel = Vec3::new(12.0, 0.0, 0.0); // shoved sideways
        let dt = 1.0 / 60.0;
        let (mut min_x, mut max_x, mut max_dist) = (f32::MAX, f32::MIN, 0.0f32);
        for _ in 0..600 {
            // 10 seconds
            vel.y -= GRAVITY * dt;
            vel = rope_constrain(pos, vel, anchor, len);
            pos += vel * dt;
            assert!(pos.is_finite() && vel.is_finite(), "NaN/inf in swing");
            min_x = min_x.min(pos.x);
            max_x = max_x.max(pos.x);
            max_dist = max_dist.max((pos - anchor).length());
        }
        assert!(max_x > 3.0, "never swung to the +x side: max_x={max_x}");
        assert!(min_x < -3.0, "never swung to the -x side: min_x={min_x}");
        // The pure one-sided velocity projection leaves a small, BOUNDED Euler
        // drift (~9% over a full 10s continuous swing) rather than a blow-up
        // (which would be 10x+ or NaN). It's imperceptible over real 1-4s swings,
        // and the rope mesh always draws to the true anchor, so there's no visible
        // stretch. We assert it stays within 15% — proof the rope holds.
        assert!(max_dist < len * 1.15, "rope radius drifted too far: max_dist={max_dist}");
    }

    /// Reel-in must actually pull the player toward the anchor (regression guard:
    /// the first cut only shrank `hook.len`, which — because rope_constrain is
    /// one-sided and never adds inward velocity — moved the player 0m). Mirrors
    /// the player_move reel loop: inject inward velocity, track len to distance,
    /// then constrain. A held reel on a dead ceiling-hang must close the distance.
    #[test]
    fn reel_in_pulls_the_player_toward_the_anchor() {
        use crate::common::tune::{GRAPPLE_EPS, GRAPPLE_MIN_LEN, GRAPPLE_REEL_SPEED, GRAVITY};
        let anchor = Vec3::ZERO;
        let mut pos = Vec3::new(0.0, -10.0, 0.0); // hanging straight down, at rest
        let mut vel = Vec3::ZERO;
        let mut len = 10.0_f32;
        let dt = 1.0 / 60.0;
        let start = (pos - anchor).length();
        for _ in 0..120 {
            // 2s of holding reel
            vel.y -= GRAVITY * dt;
            let to = anchor - pos;
            let dist = to.length();
            if dist > GRAPPLE_EPS {
                let inward = to / dist;
                if dist > GRAPPLE_MIN_LEN {
                    let cur_in = vel.dot(inward);
                    if cur_in < GRAPPLE_REEL_SPEED {
                        vel += inward * (GRAPPLE_REEL_SPEED - cur_in);
                    }
                    len = (dist - GRAPPLE_REEL_SPEED * dt).max(GRAPPLE_MIN_LEN);
                } else {
                    let cur_in = vel.dot(inward);
                    if cur_in > 0.0 {
                        vel -= inward * cur_in;
                    }
                }
            }
            vel = rope_constrain(pos, vel, anchor, len);
            pos += vel * dt;
            assert!(pos.is_finite());
        }
        let end = (pos - anchor).length();
        // Reeled a long way in, then SETTLED at the min-length floor (didn't yank
        // on through to the anchor) — free-space robustness without a wall to stop it.
        assert!(end < start - 5.0, "reel-in failed to pull the player in: {start} -> {end}");
        assert!(
            (GRAPPLE_MIN_LEN - 0.5..=GRAPPLE_MIN_LEN + 1.0).contains(&end),
            "reel-in didn't settle at the min-length floor: end={end}"
        );
    }
}
