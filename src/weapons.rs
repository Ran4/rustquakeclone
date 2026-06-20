//! Player weapons: inventory, firing (hitscan + projectile), switching, the
//! first-person view-model, muzzle flash and recoil.

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::common::*;
use crate::effects::{spawn_muzzle_flash, Lifetime};
use crate::physics::{ray_aabb, raycast_world, Aabb};
use crate::player::{Player, PlayerCamera};
use crate::projectiles::{spawn_projectile, ProjKind};

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewKick>().add_systems(
            Update,
            (switch_weapon, fire_weapon, update_viewmodel)
                .run_if(in_state(GameState::Playing)),
        );
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
}
impl WeaponKind {
    pub const ALL: [WeaponKind; 6] = [
        WeaponKind::Shotgun,
        WeaponKind::SuperShotgun,
        WeaponKind::Nailgun,
        WeaponKind::Grenade,
        WeaponKind::Rocket,
        WeaponKind::Lightning,
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
        }
    }
    pub fn ammo(self) -> usize {
        match self {
            WeaponKind::Shotgun | WeaponKind::SuperShotgun => 0, // Shells
            WeaponKind::Nailgun => 1,                            // Nails
            WeaponKind::Grenade | WeaponKind::Rocket => 2,       // Rockets
            WeaponKind::Lightning => 3,                          // Cells
        }
    }
    fn stats(self) -> Stats {
        match self {
            WeaponKind::Shotgun => Stats { cooldown: 0.55, cost: 1, sound: Sound::Shotgun, shake: 0.12, kick: 0.5, mode: Mode::Hitscan { pellets: 6, spread: 0.045, damage: 5.0 } },
            WeaponKind::SuperShotgun => Stats { cooldown: 0.85, cost: 2, sound: Sound::SuperShotgun, shake: 0.22, kick: 1.0, mode: Mode::Hitscan { pellets: 14, spread: 0.075, damage: 5.0 } },
            WeaponKind::Nailgun => Stats { cooldown: 0.1, cost: 1, sound: Sound::Nailgun, shake: 0.04, kick: 0.18, mode: Mode::Projectile(ProjKind::Nail) },
            WeaponKind::Grenade => Stats { cooldown: 0.7, cost: 1, sound: Sound::GrenadeFire, shake: 0.1, kick: 0.5, mode: Mode::Projectile(ProjKind::Grenade) },
            WeaponKind::Rocket => Stats { cooldown: 0.85, cost: 1, sound: Sound::RocketFire, shake: 0.18, kick: 0.9, mode: Mode::Projectile(ProjKind::Rocket) },
            WeaponKind::Lightning => Stats { cooldown: 0.06, cost: 1, sound: Sound::Lightning, shake: 0.05, kick: 0.12, mode: Mode::Beam { damage: 8.0 } },
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
}

#[derive(Component)]
pub struct Inventory {
    pub owned: [bool; 6],
    pub ammo: [i32; 4],
    pub current: WeaponKind,
    pub cooldown: f32,
}
impl Default for Inventory {
    fn default() -> Self {
        let mut owned = [false; 6];
        owned[WeaponKind::Shotgun.index()] = true;
        Self { owned, ammo: [25, 0, 0, 0], current: WeaponKind::Shotgun, cooldown: 0.0 }
    }
}

#[derive(Resource, Default)]
pub struct ViewKick {
    pub amount: f32,
}

#[derive(Component)]
pub struct ViewModel;

#[derive(Resource)]
pub struct WeaponVis {
    mats: Vec<Handle<StandardMaterial>>,
    scales: Vec<Vec3>,
}

/// Build the weapon view-model materials once (idempotent). Runs at the front
/// of the OnEnter(Playing) chain so it exists before any weapon system needs it.
pub fn create_weapon_vis(
    mut commands: Commands,
    existing: Option<Res<WeaponVis>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if existing.is_some() {
        return;
    }
    let mut mat = |c: Color, e: LinearRgba| {
        materials.add(StandardMaterial { base_color: c, emissive: e, perceptual_roughness: 0.4, metallic: 0.6, ..default() })
    };
    let mats = vec![
        mat(rgb(0.30, 0.30, 0.33), LinearRgba::BLACK),  // shotgun
        mat(rgb(0.22, 0.22, 0.24), LinearRgba::BLACK),  // ssg
        mat(rgb(0.40, 0.42, 0.48), LinearRgba::BLACK),  // nailgun
        mat(rgb(0.25, 0.45, 0.2), LinearRgba::rgb(0.05, 0.2, 0.02)), // grenade
        mat(rgb(0.5, 0.18, 0.12), LinearRgba::rgb(0.3, 0.05, 0.0)),  // rocket
        mat(rgb(0.3, 0.45, 0.7), LinearRgba::rgb(0.2, 0.6, 1.5)),    // lightning
    ];
    let scales = vec![
        Vec3::new(0.14, 0.14, 0.6),
        Vec3::new(0.2, 0.16, 0.55),
        Vec3::new(0.16, 0.18, 0.7),
        Vec3::new(0.18, 0.2, 0.5),
        Vec3::new(0.2, 0.2, 0.75),
        Vec3::new(0.16, 0.16, 0.8),
    ];
    commands.insert_resource(WeaponVis { mats, scales });
}

/// Insert inventory on the player and attach a view-model to the camera.
/// Runs in the OnEnter(Playing) chain right after the player is spawned.
pub fn setup_player_weapons(
    mut commands: Commands,
    vis: Res<WeaponVis>,
    gfx: Res<GfxAssets>,
    q_player: Query<Entity, With<Player>>,
    q_cam: Query<Entity, With<PlayerCamera>>,
) {
    if let Ok(pe) = q_player.single() {
        commands.entity(pe).insert(Inventory::default());
    }
    if let Ok(ce) = q_cam.single() {
        commands.entity(ce).with_children(|p| {
            p.spawn((
                Mesh3d(gfx.unit_cube.clone()),
                MeshMaterial3d(vis.mats[0].clone()),
                Transform::from_xyz(0.32, -0.3, -0.75).with_scale(vis.scales[0]),
                ViewModel,
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
        KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
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
        for _ in 0..6 {
            idx = (idx + dir).rem_euclid(6);
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

#[allow(clippy::too_many_arguments)]
fn fire_weapon(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    colliders: Res<WorldColliders>,
    gfx: Res<GfxAssets>,
    mut rng_state: Local<u32>,
    mut nail_alt: Local<bool>,
    mut q_player: Query<(Entity, &mut Inventory)>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    mut dmg: MessageWriter<DamageEvent>,
    mut impact: MessageWriter<ImpactEvent>,
    mut sfx: MessageWriter<Sfx>,
    mut shake: MessageWriter<ScreenShake>,
    mut kick: ResMut<ViewKick>,
) {
    let dt = time.delta_secs();
    let Ok((pe, mut inv)) = q_player.single_mut() else { return };
    inv.cooldown = (inv.cooldown - dt).max(0.0);

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
    let muzzle = origin + forward * 0.6;

    spawn_muzzle_flash(&mut commands, &gfx, muzzle);
    sfx.write(Sfx::global(stats.sound));
    shake.write(ScreenShake { amount: stats.shake });
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
                hitscan(&mut commands, &colliders, &targets, &gfx, origin, d, damage, pe, false, &mut dmg, &mut impact);
            }
        }
        Mode::Beam { damage } => {
            let d = spread_dir(forward, 0.005, rand(), rand());
            hitscan(&mut commands, &colliders, &targets, &gfx, origin, d, damage, pe, true, &mut dmg, &mut impact);
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
    }
}

fn spread_dir(forward: Vec3, spread: f32, rx: f32, ry: f32) -> Vec3 {
    let up = if forward.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let right = forward.cross(up).normalize_or_zero();
    let up2 = right.cross(forward).normalize_or_zero();
    (forward + right * (rx * spread) + up2 * (ry * spread)).normalize_or_zero()
}

#[allow(clippy::too_many_arguments)]
fn hitscan(
    commands: &mut Commands,
    colliders: &WorldColliders,
    targets: &Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    gfx: &GfxAssets,
    origin: Vec3,
    dir: Vec3,
    damage: f32,
    source: Entity,
    beam: bool,
    dmg: &mut MessageWriter<DamageEvent>,
    impact: &mut MessageWriter<ImpactEvent>,
) {
    let max = 200.0;
    // Nearest monster.
    let mut best_t = max;
    let mut hit_enemy: Option<(Entity, Vec3, Vec3)> = None;
    for (e, gt, hb, fac) in targets.iter() {
        if *fac != Faction::Monster {
            continue;
        }
        let b = Aabb::from_center_half(gt.translation(), hb.half);
        if let Some((t, n)) = ray_aabb(origin, dir, best_t, &b) {
            best_t = t;
            hit_enemy = Some((e, origin + dir * t, n));
        }
    }
    // World (closer than enemy?).
    let mut world_hit: Option<(Vec3, Vec3)> = None;
    if let Some((t, pt, n)) = raycast_world(origin, dir, best_t, &colliders.solids) {
        best_t = t;
        world_hit = Some((pt, n));
        hit_enemy = None;
    }

    let end = origin + dir * best_t.min(max);
    if beam {
        draw_beam(commands, gfx, origin, end);
    }

    if let Some((e, pt, n)) = hit_enemy {
        dmg.write(DamageEvent { target: e, amount: damage, source: Some(source), knockback: dir * 1.5 });
        impact.write(ImpactEvent { pos: pt, normal: n, blood: true });
    } else if let Some((pt, n)) = world_hit {
        impact.write(ImpactEvent { pos: pt, normal: n, blood: false });
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
    time: Res<Time>,
    mut kick: ResMut<ViewKick>,
    vis: Res<WeaponVis>,
    q_player: Query<(&Inventory, &Player)>,
    mut q_vm: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), With<ViewModel>>,
) {
    let dt = time.delta_secs();
    kick.amount = (kick.amount - dt * 6.0).max(0.0);
    let Ok((inv, player)) = q_player.single() else { return };
    let Ok((mut tf, mut mat)) = q_vm.single_mut() else { return };
    let idx = inv.current.index();
    if mat.0 != vis.mats[idx] {
        mat.0 = vis.mats[idx].clone();
    }
    let bx = (player.bob).sin() * 0.012;
    let by = ((player.bob * 2.0).sin()).abs() * 0.012;
    let base = Vec3::new(0.32, -0.3, -0.75);
    tf.translation = base + Vec3::new(bx, by, kick.amount.min(1.0) * 0.18);
    tf.scale = vis.scales[idx];
    tf.rotation = Quat::from_rotation_x(-kick.amount.min(1.0) * 0.2);
}
