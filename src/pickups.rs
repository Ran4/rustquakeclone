//! Pickups: health, armor, ammo, weapons and the silver key.

use bevy::prelude::*;

use crate::common::*;
use crate::level::{ItemKind, SpawnPlan};
use crate::player::Player;
use crate::weapons::{Inventory, WeaponKind};

pub struct PickupsPlugin;
impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (animate_items, pickup_items).run_if(in_state(GameState::Playing)));
    }
}

#[derive(Component)]
pub struct Item {
    pub kind: ItemKind,
    pub base_y: f32,
    pub bob: f32,
}

fn item_look(kind: ItemKind) -> (Color, LinearRgba, Vec3) {
    use ItemKind::*;
    match kind {
        Health(_) => (rgb(0.2, 0.9, 0.3), LinearRgba::rgb(0.1, 1.2, 0.2), Vec3::new(0.4, 0.4, 0.4)),
        MegaHealth => (rgb(0.3, 0.5, 1.0), LinearRgba::rgb(0.3, 0.6, 2.5), Vec3::new(0.5, 0.5, 0.5)),
        ArmorGreen => (rgb(0.2, 0.8, 0.3), LinearRgba::rgb(0.05, 0.5, 0.1), Vec3::new(0.5, 0.6, 0.3)),
        ArmorYellow => (rgb(0.9, 0.8, 0.2), LinearRgba::rgb(0.6, 0.5, 0.05), Vec3::new(0.5, 0.6, 0.3)),
        Shells(_) => (rgb(0.8, 0.7, 0.3), LinearRgba::rgb(0.2, 0.15, 0.0), Vec3::new(0.45, 0.3, 0.3)),
        Nails(_) => (rgb(0.7, 0.7, 0.8), LinearRgba::rgb(0.15, 0.15, 0.2), Vec3::new(0.45, 0.3, 0.3)),
        Rockets(_) => (rgb(0.8, 0.3, 0.2), LinearRgba::rgb(0.3, 0.05, 0.0), Vec3::new(0.45, 0.4, 0.3)),
        Cells(_) => (rgb(0.4, 0.6, 1.0), LinearRgba::rgb(0.1, 0.3, 1.0), Vec3::new(0.4, 0.4, 0.3)),
        WeaponSuperShotgun => (rgb(0.25, 0.25, 0.28), LinearRgba::BLACK, Vec3::new(0.7, 0.25, 0.3)),
        WeaponNailgun => (rgb(0.45, 0.47, 0.52), LinearRgba::BLACK, Vec3::new(0.6, 0.25, 0.3)),
        WeaponGrenade => (rgb(0.3, 0.5, 0.25), LinearRgba::rgb(0.05, 0.2, 0.02), Vec3::new(0.7, 0.3, 0.3)),
        WeaponRocket => (rgb(0.55, 0.2, 0.15), LinearRgba::rgb(0.3, 0.05, 0.0), Vec3::new(0.8, 0.3, 0.3)),
        WeaponLightning => (rgb(0.35, 0.5, 0.8), LinearRgba::rgb(0.2, 0.6, 1.5), Vec3::new(0.7, 0.25, 0.3)),
        SilverKey => (rgb(0.85, 0.85, 0.95), LinearRgba::rgb(1.5, 1.5, 2.0), Vec3::new(0.3, 0.5, 0.15)),
    }
}

pub fn spawn_items(
    mut commands: Commands,
    plan: Res<SpawnPlan>,
    gfx: Res<GfxAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for it in &plan.items {
        let (color, emissive, scale) = item_look(it.kind);
        let mat = materials.add(StandardMaterial { base_color: color, emissive, perceptual_roughness: 0.4, metallic: 0.3, ..default() });
        commands.spawn((
            Mesh3d(gfx.unit_cube.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(it.pos).with_scale(scale),
            Item { kind: it.kind, base_y: it.pos.y, bob: 0.0 },
            LevelEntity,
        ));
    }
}

fn animate_items(time: Res<Time>, mut q: Query<(&mut Transform, &mut Item)>) {
    let dt = time.delta_secs();
    for (mut tf, mut item) in &mut q {
        item.bob += dt * 2.0;
        tf.translation.y = item.base_y + (item.bob).sin() * 0.12 + 0.2;
        tf.rotation = Quat::from_rotation_y(item.bob);
    }
}

#[allow(clippy::too_many_arguments)]
fn pickup_items(
    mut commands: Commands,
    q_items: Query<(Entity, &Transform, &Item)>,
    mut q_player: Query<(&Transform, &mut Health, &mut Armor, &mut Inventory), With<Player>>,
    mut mission: ResMut<Mission>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    let Ok((ptf, mut health, mut armor, mut inv)) = q_player.single_mut() else { return };
    let ppos = ptf.translation;
    for (e, itf, item) in &q_items {
        if itf.translation.distance(ppos) > 1.7 {
            continue;
        }
        if apply_item(item.kind, &mut health, &mut armor, &mut inv, &mut mission, &mut sfx, &mut notify) {
            flash.write(ScreenFlash { color: rgb(0.6, 0.55, 0.2), strength: 0.18 });
            commands.entity(e).despawn();
        }
    }
}

fn apply_item(
    kind: ItemKind,
    health: &mut Health,
    armor: &mut Armor,
    inv: &mut Inventory,
    mission: &mut Mission,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
) -> bool {
    use ItemKind::*;
    let cap = 200;
    match kind {
        Health(n) => {
            if health.current >= health.max {
                return false;
            }
            health.current = (health.current + n as f32).min(health.max);
            sfx.write(Sfx::global(Sound::PickupHealth));
            notify.write(Notify::new(format!("+{n} Health")));
        }
        MegaHealth => {
            if health.current >= health.max {
                return false;
            }
            health.current = health.max;
            sfx.write(Sfx::global(Sound::PickupHealth));
            notify.write(Notify::new("Mega Health!"));
        }
        ArmorGreen => {
            if armor.points >= 100.0 {
                return false;
            }
            armor.points = armor.points.max(100.0);
            armor.absorb = armor.absorb.max(0.5);
            sfx.write(Sfx::global(Sound::PickupArmor));
            notify.write(Notify::new("Green Armor"));
        }
        ArmorYellow => {
            if armor.points >= 150.0 && armor.absorb >= 0.75 {
                return false;
            }
            armor.points = armor.points.max(150.0);
            armor.absorb = armor.absorb.max(0.75);
            sfx.write(Sfx::global(Sound::PickupArmor));
            notify.write(Notify::new("Yellow Armor"));
        }
        Shells(n) => return give_ammo(inv, 0, n, cap, sfx, notify, "Shells"),
        Nails(n) => return give_ammo(inv, 1, n, cap, sfx, notify, "Nails"),
        Rockets(n) => return give_ammo(inv, 2, n, cap, sfx, notify, "Rockets"),
        Cells(n) => return give_ammo(inv, 3, n, cap, sfx, notify, "Cells"),
        WeaponSuperShotgun => return give_weapon(inv, WeaponKind::SuperShotgun, 0, 5, sfx, notify),
        WeaponNailgun => return give_weapon(inv, WeaponKind::Nailgun, 1, 30, sfx, notify),
        WeaponGrenade => return give_weapon(inv, WeaponKind::Grenade, 2, 5, sfx, notify),
        WeaponRocket => return give_weapon(inv, WeaponKind::Rocket, 2, 5, sfx, notify),
        WeaponLightning => return give_weapon(inv, WeaponKind::Lightning, 3, 15, sfx, notify),
        SilverKey => {
            mission.has_key = true;
            sfx.write(Sfx::global(Sound::KeyPickup));
            notify.write(Notify::new("Picked up the Silver Key!"));
        }
    }
    true
}

fn give_ammo(
    inv: &mut Inventory,
    idx: usize,
    n: u32,
    cap: i32,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
    label: &str,
) -> bool {
    if inv.ammo[idx] >= cap {
        return false;
    }
    inv.ammo[idx] = (inv.ammo[idx] + n as i32).min(cap);
    sfx.write(Sfx::global(Sound::PickupAmmo));
    notify.write(Notify::new(format!("+{n} {label}")));
    true
}

fn give_weapon(
    inv: &mut Inventory,
    w: WeaponKind,
    ammo_idx: usize,
    ammo: i32,
    sfx: &mut MessageWriter<Sfx>,
    notify: &mut MessageWriter<Notify>,
) -> bool {
    let i = w.index();
    let had = inv.owned[i];
    inv.owned[i] = true;
    inv.ammo[ammo_idx] = (inv.ammo[ammo_idx] + ammo).min(200);
    inv.current = w;
    sfx.write(Sfx::global(Sound::PickupWeapon));
    notify.write(Notify::new(if had { format!("{}", w.name()) } else { format!("Got the {}!", w.name()) }));
    true
}
