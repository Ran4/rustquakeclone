//! The campaign's level registry. Each level is a `build(&mut Build)` fn in its
//! own module; `LEVEL_META` names them and picks their theme, and `build_index`
//! dispatches to the right one.

use bevy::prelude::*;

use crate::common::{rgb, NUM_LEVELS};
use crate::level::{Build, ItemKind, MonsterKind::*, ThemeId, Wall};

pub mod level1;
pub mod level2;
pub mod level3;
pub mod level4;
pub mod level5;
pub mod level6;
pub mod level7;
pub mod level8;
pub mod blackvein;
pub mod brine;
pub mod shatterglass;

// NOTE: the campaign was expanded from 8 to 11 levels by interspersing three
// new mine-themed maps WITHOUT renaming the original files. So the module file
// number (`levelN`) no longer equals its campaign position. The file → campaign
// index mapping is:
//
//   campaign idx  module          name                       (file moved to)
//   ------------  --------------  -------------------------   --------------
//    0            level1          Dimension of the Doomed     (unchanged)
//    1            level2          Frostspire Keep             (unchanged)
//    2            blackvein       Blackvein Deep              NEW (Mine)
//    3            level3          The Brass Leviathan         (was idx 2)
//    4            level4          Tomb of the Sunken King     (was idx 3)
//    5            brine           The Brine Gallery           NEW (Brine)
//    6            level5          The Verdant Rot             (was idx 4)
//    7            level6          The Salt Wraith             (was idx 5)
//    8            shatterglass    Shatterglass Vein           NEW (Crystal)
//    9            level7          Sanctum of the Void         (was idx 6)
//   10            level8          The Drowned Colossus        (was idx 7, finale)

/// Display name + theme for each level, indexed by level number.
pub const LEVEL_META: [(&str, ThemeId); NUM_LEVELS] = [
    ("Dimension of the Doomed", ThemeId::Doomed),
    ("Frostspire Keep", ThemeId::Frost),
    ("Blackvein Deep", ThemeId::Mine),
    ("The Brass Leviathan", ThemeId::Brass),
    ("Tomb of the Sunken King", ThemeId::Tomb),
    ("The Brine Gallery", ThemeId::Brine),
    ("The Verdant Rot", ThemeId::Hive),
    ("The Salt Wraith", ThemeId::Pirate),
    ("Shatterglass Vein", ThemeId::Crystal),
    ("Sanctum of the Void", ThemeId::Void),
    ("The Drowned Colossus", ThemeId::Dam),
];

/// Build the level with the given index into the supplied context.
pub fn build_index(idx: usize, b: &mut Build) {
    match idx {
        0 => level1::build(b),
        1 => level2::build(b),
        2 => blackvein::build(b),
        3 => level3::build(b),
        4 => level4::build(b),
        5 => brine::build(b),
        6 => level5::build(b),
        7 => level6::build(b),
        8 => shatterglass::build(b),
        9 => level7::build(b),
        10 => level8::build(b),
        _ => level1::build(b),
    }
}

/// A simple, fully-playable linear level used as a placeholder until each
/// themed level is authored. Spawn → corridor → key room (locked door) → exit,
/// rendered entirely with the active theme's textures/fog so each placeholder
/// still looks distinct.
pub fn stub_level(b: &mut Build) {
    b.start.pos = Vec3::new(0.0, 1.0, 8.0);
    b.start.yaw = 0.0;

    // Start room (open north to the corridor).
    b.room(-6.0, 6.0, 0.0, 12.0, 0.0, 5.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 11.2), Vec3::new(2.0, 4.0, 11.6));
    b.item(ItemKind::Shells(20), Vec3::new(3.0, 0.6, 6.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(-3.0, 0.6, 6.0));

    // Corridor north.
    b.corridor_z(-2.0, 2.0, -8.0, 0.0, 0.0, 4.0);

    // Key chamber (open south to corridor, north to locked door).
    b.room(-8.0, 8.0, -24.0, -8.0, 0.0, 6.0, &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);
    b.monster(Knight, Vec3::new(-4.0, 1.0, -16.0));
    b.monster(Grunt, Vec3::new(4.0, 1.0, -18.0));
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 1.0, -20.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 0.6, -12.0));
    b.item(ItemKind::Nails(30), Vec3::new(6.0, 0.6, -12.0));
    b.hazard(-2.0, 2.0, -23.0, -21.0, 0.0);
    b.ambush(Ogre, Vec3::new(0.0, 1.0, -22.0));

    // Locked door on the north wall (z = -24).
    b.door(Vec3::new(-2.0, 0.0, -24.3), Vec3::new(2.0, 5.0, -23.7), Vec3::new(0.0, 5.2, 0.0));

    // Corridor to the exit.
    b.corridor_z(-2.0, 2.0, -32.0, -24.0, 0.0, 4.0);

    // Exit chamber.
    b.room(-8.0, 8.0, -44.0, -32.0, 0.0, 6.0, &[Wall::S((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-3.0, 0.1, -43.6), Vec3::new(3.0, 4.0, -43.4));
    b.exit(Vec3::new(0.0, 1.5, -42.0));
    b.monster(Enforcer, Vec3::new(0.0, 1.0, -38.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 0.6, -34.0));

    // Lights.
    b.sun(Vec3::new(10.0, 30.0, 5.0), Vec3::new(0.0, 0.0, -20.0), rgb(0.7, 0.7, 0.8), 3000.0);
    b.light(Vec3::new(0.0, 4.0, 6.0), rgb(1.0, 0.9, 0.7), 500_000.0, 40.0);
    b.light(Vec3::new(0.0, 5.0, -16.0), rgb(1.0, 0.85, 0.65), 900_000.0, 45.0);
    b.light(Vec3::new(0.0, 4.0, -38.0), rgb(0.8, 0.85, 1.0), 700_000.0, 40.0);
}
