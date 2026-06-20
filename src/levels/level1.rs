//! Level 1 — "Dimension of the Doomed". The original mission, ported to the
//! `Build` API. This file doubles as the reference example for authoring levels:
//! spawn → corridors → key vault → locked door → exit slipgate.

use bevy::prelude::*;

use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

pub fn build(b: &mut Build) {
    // Player starts in the slipgate room facing north (-Z).
    b.start.pos = Vec3::new(0.0, 1.0, 2.0);
    b.start.yaw = 0.0;

    let metal = b.theme.metal.clone();
    let trim = b.theme.trim.clone();

    // === Start room S ===
    b.room(-6.0, 6.0, -6.0, 6.0, 0.0, 5.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 5.2), Vec3::new(2.0, 4.0, 5.6));
    b.item(ItemKind::ArmorGreen, Vec3::new(-4.0, 0.6, -3.0));
    b.item(ItemKind::Shells(15), Vec3::new(4.0, 0.6, -3.0));

    // === Corridor C1 ===
    b.corridor_z(-2.0, 2.0, -14.0, -6.0, 0.0, 4.0);

    // === Room A "Grunt Hall" ===
    b.room(-8.0, 8.0, -28.0, -14.0, 0.0, 6.0, &[Wall::S((-2.0, 2.0)), Wall::E((-25.0, -21.0))]);
    b.monster(Grunt, Vec3::new(-4.0, 1.0, -24.0));
    b.monster(Grunt, Vec3::new(4.0, 1.0, -25.0));
    b.monster(Grunt, Vec3::new(0.0, 1.0, -27.0));
    b.item(ItemKind::Shells(30), Vec3::new(-6.0, 0.6, -26.0));
    b.item(ItemKind::Health(25), Vec3::new(6.0, 0.6, -26.0));
    b.item(ItemKind::WeaponSuperShotgun, Vec3::new(0.0, 0.6, -18.0));
    b.item(ItemKind::Shells(15), Vec3::new(2.0, 0.6, -18.0));

    // === Corridor C2 with LAVA ===
    b.corridor_x(8.0, 20.0, -25.0, -21.0, 0.0, 5.0);
    b.hazard(12.0, 16.0, -25.0, -21.0, 0.0);

    // === Room B "Ogre Ledge" ===
    b.room(20.0, 38.0, -30.0, -14.0, 0.0, 9.0, &[Wall::W((-25.0, -21.0)), Wall::S((26.0, 30.0))]);
    b.solid(Vec3::new(20.0, 2.5, -30.0), Vec3::new(25.0, 3.0, -24.0), metal.clone());
    b.solid(Vec3::new(33.0, 2.5, -30.0), Vec3::new(38.0, 3.0, -24.0), metal.clone());
    b.stairs(31.0, 33.0, -16.0, 3.0, 0.0, 6, Vec3::Z * -1.0);
    b.monster(Ogre, Vec3::new(22.5, 3.6, -27.0));
    b.monster(Ogre, Vec3::new(35.5, 3.6, -27.0));
    b.monster(Knight, Vec3::new(29.0, 1.0, -20.0));
    b.item(ItemKind::WeaponNailgun, Vec3::new(35.5, 3.6, -27.0));
    b.item(ItemKind::Nails(75), Vec3::new(22.5, 3.6, -27.0));
    b.item(ItemKind::WeaponGrenade, Vec3::new(22.5, 3.6, -26.0));
    b.item(ItemKind::Rockets(15), Vec3::new(24.5, 3.6, -27.0));
    b.item(ItemKind::Health(25), Vec3::new(29.0, 0.6, -28.0));

    // === Corridor C3 ===
    b.corridor_z(26.0, 30.0, -14.0, -8.0, 0.0, 5.0);

    // === Atrium "The Pit" ===
    b.room(18.0, 40.0, -8.0, 14.0, 0.0, 12.0, &[Wall::N((26.0, 30.0)), Wall::E((0.0, 4.0)), Wall::S((27.0, 31.0))]);
    b.solid(Vec3::new(24.0, 0.0, 0.0), Vec3::new(26.0, 8.0, 2.0), trim.clone());
    b.solid(Vec3::new(32.0, 0.0, 4.0), Vec3::new(34.0, 8.0, 6.0), trim.clone());
    b.solid(Vec3::new(34.0, 3.0, -8.0), Vec3::new(40.0, 3.5, -2.0), metal.clone());
    b.stairs(30.0, 34.0, -3.0, 3.5, 0.0, 7, Vec3::X);
    b.monster(Knight, Vec3::new(22.0, 1.0, 6.0));
    b.monster(Scrag, Vec3::new(30.0, 5.0, 8.0));
    b.monster(Scrag, Vec3::new(36.0, 6.0, 10.0));
    b.monster(Enforcer, Vec3::new(20.0, 1.0, 12.0));
    b.item(ItemKind::WeaponRocket, Vec3::new(37.0, 4.1, -5.0));
    b.item(ItemKind::Rockets(15), Vec3::new(37.0, 4.1, -3.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(20.0, 0.6, -5.0));
    b.item(ItemKind::Shells(20), Vec3::new(22.0, 0.6, 8.0));
    b.item(ItemKind::Nails(30), Vec3::new(20.0, 0.6, 10.0));

    // === Corridor C4 to Key Vault ===
    b.corridor_x(40.0, 46.0, 0.0, 4.0, 0.0, 5.0);

    // === Key Vault ===
    b.room(46.0, 58.0, -4.0, 8.0, 0.0, 6.0, &[Wall::W((0.0, 4.0))]);
    b.slab(50.0, 54.0, 1.0, 5.0, 0.1, 0.1, trim.clone());
    b.monster(DeathKnight, Vec3::new(54.0, 1.0, 2.0));
    b.item(ItemKind::SilverKey, Vec3::new(52.0, 1.0, 2.0));
    b.item(ItemKind::MegaHealth, Vec3::new(48.0, 0.8, 6.0));
    b.item(ItemKind::Cells(50), Vec3::new(56.0, 0.8, 6.0));
    b.item(ItemKind::WeaponLightning, Vec3::new(48.0, 0.8, -2.0));

    // === Locked door on the Atrium south wall (z=14), gap x[27,31] ===
    b.door(Vec3::new(27.0, 0.0, 13.7), Vec3::new(31.0, 5.0, 14.3), Vec3::new(0.0, 5.2, 0.0));

    // === Corridor C5 ===
    b.corridor_z(27.0, 31.0, 14.0, 22.0, 0.0, 5.0);

    // === Final Chamber ===
    b.room(20.0, 38.0, 22.0, 40.0, 0.0, 9.0, &[Wall::N((27.0, 31.0))]);
    b.slipgate(Vec3::new(26.0, 0.1, 39.4), Vec3::new(32.0, 4.5, 39.8));
    b.exit(Vec3::new(29.0, 1.5, 38.5));
    b.monster(Ogre, Vec3::new(24.0, 1.0, 34.0));
    b.monster(Knight, Vec3::new(34.0, 1.0, 34.0));
    b.monster(Enforcer, Vec3::new(29.0, 1.0, 36.0));
    b.item(ItemKind::Health(25), Vec3::new(22.0, 0.6, 24.0));
    b.item(ItemKind::Rockets(15), Vec3::new(36.0, 0.6, 24.0));
    b.item(ItemKind::Shells(20), Vec3::new(29.0, 0.6, 24.0));

    // Reinforcements that burst in when the key is taken (classic Quake ambush).
    b.ambush(Knight, Vec3::new(47.0, 1.0, 1.5));
    b.ambush(Knight, Vec3::new(47.0, 1.0, 2.5));
    b.ambush(Ogre, Vec3::new(50.0, 1.0, 6.0));

    // ---- lights ----
    b.sun(Vec3::new(10.0, 30.0, 5.0), Vec3::new(20.0, 0.0, 0.0), rgb(0.6, 0.6, 0.75), 2700.0);
    b.light(Vec3::new(0.0, 4.0, 0.0), rgb(1.0, 0.8, 0.6), 600_000.0, 40.0);
    b.light(Vec3::new(0.0, 4.5, -22.0), rgb(1.0, 0.7, 0.5), 800_000.0, 40.0);
    b.light(Vec3::new(14.0, 3.5, -23.0), rgb(1.0, 0.4, 0.1), 500_000.0, 40.0);
    b.light(Vec3::new(29.0, 6.0, -22.0), rgb(0.9, 0.8, 0.7), 1_200_000.0, 40.0);
    b.light(Vec3::new(29.0, 8.0, 3.0), rgb(0.8, 0.85, 1.0), 1_500_000.0, 40.0);
    b.light(Vec3::new(52.0, 4.0, 2.0), rgb(0.9, 0.85, 0.5), 800_000.0, 40.0);
    b.light(Vec3::new(29.0, 6.0, 32.0), rgb(0.7, 0.6, 0.9), 1_000_000.0, 40.0);
}
