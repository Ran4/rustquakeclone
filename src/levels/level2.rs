use bevy::prelude::*;
use crate::common::{rgb, FloorMaterial};
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

// FROSTSPIRE KEEP  (north is -Z, +Y up)  -- easy, 2nd level
//
//        -Z (north)
//   +------------------------------+
//   |   TOWER / KEEP  (exit)       |   x[-9..9]  z[-58..-40]
//   |   stairs up -> exit slipgate |
//   +-----------[locked door]------+
//   |   GREAT HALL (ice pillars)   |   x[-14..14] z[-40..-18]
//   |   SILVER KEY on dais + ambush|
//   +------------[gap]-------------+
//   |   FROZEN LAKE (hazard)       |   x[-13..13] z[-18..2]
//   |   ice-block stepping stones  |
//   +------------[gap]-------------+
//   |   C1 corridor                |   x[-2..2]   z[2..10]
//   +------------[gap]-------------+
//   |   SNOWY COURTYARD (roofless) |   x[-8..8]   z[10..24]
//   |   spawn slipgate             |
//   +------------------------------+
//        +Z (south)

pub fn build(b: &mut Build) {
    // --- spawn in the snowy courtyard, facing north (-Z) toward the keep ---
    b.start.pos = Vec3::new(0.0, 1.0, 20.0);
    b.start.yaw = 0.0;

    // theme handles
    let _metal = b.theme.metal.clone();
    let trim = b.theme.trim.clone();
    let ice = b.mat(rgb(0.72, 0.86, 0.98), LinearRgba::rgb(0.05, 0.12, 0.22), 0.15, 0.0);
    let crystal = b.mat(rgb(0.45, 0.85, 1.0), LinearRgba::rgb(0.5, 1.6, 3.0), 0.2, 0.0);

    // ========================================================================
    // SNOWY COURTYARD — roofless, high walls, open sky
    // ========================================================================
    b.roofless(-8.0, 8.0, 10.0, 24.0, 0.0, 7.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 23.2), Vec3::new(2.0, 4.2, 23.6));
    // a couple of decorative icicle pillars flanking the gate
    b.solid(Vec3::new(-7.0, 0.0, 11.0), Vec3::new(-6.0, 6.0, 12.0), ice.clone());
    b.solid(Vec3::new(6.0, 0.0, 11.0), Vec3::new(7.0, 6.0, 12.0), ice.clone());
    b.deco(Vec3::new(-1.2, 5.5, 14.0), Vec3::new(1.2, 7.0, 16.0), crystal.clone());
    // starting supplies
    b.item(ItemKind::ArmorGreen, Vec3::new(-5.0, 0.6, 14.0));
    b.item(ItemKind::Shells(20), Vec3::new(5.0, 0.6, 14.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 0.6, 12.0));
    b.monster(Grunt, Vec3::new(-4.0, 1.0, 18.0));
    b.monster(Grunt, Vec3::new(4.0, 1.0, 18.0));

    // ========================================================================
    // C1 — short corridor into the lake hall
    // ========================================================================
    b.corridor_z(-2.0, 2.0, 2.0, 10.0, 0.0, 4.5);

    // ========================================================================
    // FROZEN LAKE — cross on raised ice-block stepping platforms over hazard
    // ========================================================================
    b.room(-13.0, 13.0, -18.0, 2.0, 0.0, 8.0, &[Wall::S((-2.0, 2.0)), Wall::N((-3.0, 3.0))]);
    // the cracked frozen lake itself (chilling hazard) — sits low in the room
    b.hazard(-11.0, 11.0, -16.0, 0.0, 0.05);
    // raised ice-block stepping stones marching north across the lake. These are
    // genuinely slippery now (feature 47): tag them Ice so a hard charge skates
    // you past the next block — brake early or ride the slide on purpose. This is
    // the level-2 one-off ice promoted to a real movement material.
    b.floor_mat(FloorMaterial::Ice);
    b.solid(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 0.6, 1.5), ice.clone());
    b.solid(Vec3::new(-2.5, 0.0, -6.5), Vec3::new(1.5, 0.7, -3.5), ice.clone());
    b.solid(Vec3::new(-1.5, 0.0, -10.5), Vec3::new(2.5, 0.8, -7.5), ice.clone());
    b.solid(Vec3::new(-2.0, 0.0, -15.5), Vec3::new(2.0, 0.6, -11.5), ice.clone());
    b.floor_mat(FloorMaterial::Normal);
    // glowing ice crystals lighting the lake edges
    b.deco(Vec3::new(-12.5, 1.0, -8.0), Vec3::new(-11.5, 3.5, -6.0), crystal.clone());
    b.deco(Vec3::new(11.5, 1.0, -8.0), Vec3::new(12.5, 3.5, -6.0), crystal.clone());
    // flying scrags hovering over the lake + a knight on the far bank
    b.monster(Scrag, Vec3::new(-6.0, 4.0, -8.0));
    b.monster(Scrag, Vec3::new(6.0, 5.0, -10.0));
    b.monster(Knight, Vec3::new(0.0, 1.0, -15.0));
    // a weapon to pick up on the first big stepping stone
    b.item(ItemKind::WeaponSuperShotgun, Vec3::new(0.0, 1.0, -0.5));
    b.item(ItemKind::Shells(20), Vec3::new(0.5, 0.9, -9.0));

    // ========================================================================
    // GREAT HALL — ice pillars, the Silver Key dais
    // ========================================================================
    // North wall has TWO gaps: the central locked door (x[-3,3]) and an east
    // alcove (x[5,8]) plugged by a cracked-ice RESONANT panel (feature 29) — ring
    // the Lightning beam up to its shatter note and it cracks open, a key-skipping
    // shortcut straight into the tower (bypassing the locked door + the ambush).
    // The hall floor is a sheet of ice (feature 47) — fighting the Knight + ambush
    // here means negotiating the slick, where momentum overshoots your turns.
    b.floor_mat(FloorMaterial::Ice);
    b.room(-14.0, 14.0, -40.0, -18.0, 0.0, 9.0, &[Wall::S((-3.0, 3.0)), Wall::N((-3.0, 3.0)), Wall::N((5.0, 8.0))]);
    b.floor_mat(FloorMaterial::Normal);
    // rows of ice pillars
    for &x in &[-9.0_f32, 9.0] {
        for &z in &[-22.0_f32, -29.0, -36.0] {
            b.solid(Vec3::new(x - 1.0, 0.0, z - 1.0), Vec3::new(x + 1.0, 9.0, z + 1.0), ice.clone());
            b.deco(Vec3::new(x - 0.5, 7.5, z - 0.5), Vec3::new(x + 0.5, 8.7, z + 0.5), crystal.clone());
        }
    }
    // the key dais (raised trim platform) at the north end
    b.solid(Vec3::new(-3.0, 0.0, -39.0), Vec3::new(3.0, 0.8, -34.0), trim.clone());
    b.slab(-3.0, 3.0, -39.0, -34.0, 0.8, 0.1, ice.clone());
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 1.1, -36.5));
    // weapon + ammo + armor + health guarded here
    b.item(ItemKind::WeaponNailgun, Vec3::new(-10.0, 0.6, -25.0));
    b.item(ItemKind::Nails(60), Vec3::new(10.0, 0.6, -25.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(-10.0, 0.6, -33.0));
    b.item(ItemKind::Health(25), Vec3::new(10.0, 0.6, -33.0));
    // hall guards
    b.monster(Knight, Vec3::new(-6.0, 1.0, -26.0));
    b.monster(Grunt, Vec3::new(6.0, 1.0, -23.0));
    b.monster(Grunt, Vec3::new(-6.0, 1.0, -31.0));
    // classic key-grab ambush — teleport in when the key is taken
    b.ambush(Knight, Vec3::new(-3.0, 1.0, -22.0));
    b.ambush(Knight, Vec3::new(3.0, 1.0, -22.0));

    // ========================================================================
    // LOCKED DOOR — gates the tower (north wall of the great hall, z=-40)
    // ========================================================================
    b.door(Vec3::new(-3.0, 0.0, -40.3), Vec3::new(3.0, 6.0, -39.7), Vec3::new(0.0, 6.2, 0.0));

    // ========================================================================
    // RESONANT ICE PANEL — feature 29. A cracked-ice slab plugging the east gap
    // (x[5,8]) carved in BOTH the great-hall north wall and the tower south wall,
    // so shattering it leaves a real walk-through into the tower. It fills the gap
    // full wall depth (z[-40.3,-39.7]) and a doorway's height (y[0,4.5]). Hold the
    // Lightning beam on it and its hum sweeps up to the ice shatter note; it then
    // detonates into gibs, its collider degenerates and the panel vanishes — a
    // Cells-for-shortcut bet that skips the Silver Key, the door and the ambush.
    let cracked_ice = b.mat(rgb(0.62, 0.80, 0.95), LinearRgba::rgb(0.10, 0.22, 0.40), 0.18, 0.0);
    let panel_min = Vec3::new(5.0, 0.0, -40.3);
    let panel_max = Vec3::new(8.0, 4.5, -39.7);
    let panel = b.resonant(panel_min, panel_max, cracked_ice);
    // A glowing fracture seam telegraphing the panel (visual hint). Parented to the
    // panel so it vanishes with it when the wall shatters open (feature 29) rather
    // than hovering in the now-empty gap.
    b.deco_child(
        panel,
        (panel_min + panel_max) * 0.5,
        Vec3::new(5.4, 0.4, -39.68),
        Vec3::new(7.6, 4.1, -39.62),
        crystal.clone(),
    );

    // ========================================================================
    // TOWER / KEEP — stairs up to the exit slipgate
    // ========================================================================
    // South wall mirrors the great hall: the door gap (x[-3,3]) plus the resonant
    // panel gap (x[5,8]) so the shattered shortcut leads straight in here.
    b.room(-9.0, 9.0, -58.0, -40.0, 0.0, 12.0, &[Wall::S((-3.0, 3.0)), Wall::S((5.0, 8.0))]);
    // stairs climbing north up to a raised exit ledge
    b.stairs(-3.0, 3.0, -44.0, 3.0, 0.0, 6, Vec3::Z * -1.0);
    b.solid(Vec3::new(-9.0, 2.5, -57.0), Vec3::new(9.0, 3.0, -48.0), trim.clone());
    // exit slipgate atop the ledge
    b.slipgate(Vec3::new(-3.0, 3.1, -57.4), Vec3::new(3.0, 7.5, -57.0));
    b.exit(Vec3::new(0.0, 3.6, -55.5));
    // icicle crystals lighting the tower
    b.deco(Vec3::new(-8.5, 9.0, -52.0), Vec3::new(-7.5, 11.5, -50.0), crystal.clone());
    b.deco(Vec3::new(7.5, 9.0, -52.0), Vec3::new(8.5, 11.5, -50.0), crystal.clone());
    // last defenders + a final reward
    b.monster(Knight, Vec3::new(-5.0, 3.5, -52.0));
    b.monster(Grunt, Vec3::new(5.0, 3.5, -52.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 0.6, -42.0));
    b.item(ItemKind::Nails(40), Vec3::new(6.0, 0.6, -42.0));
    b.item(ItemKind::Shells(20), Vec3::new(0.0, 3.6, -53.0));

    // ========================================================================
    // LIGHTS — cool blue everywhere + a pale sun
    // ========================================================================
    b.sun(Vec3::new(-12.0, 36.0, 18.0), Vec3::new(0.0, 0.0, -20.0), rgb(0.72, 0.82, 0.95), 2600.0);
    b.light(Vec3::new(0.0, 5.5, 17.0), rgb(0.7, 0.85, 1.0), 700_000.0, 40.0);
    b.light(Vec3::new(0.0, 6.0, -8.0), rgb(0.55, 0.8, 1.0), 1_100_000.0, 42.0);
    b.light(Vec3::new(-9.0, 6.5, -28.0), rgb(0.6, 0.82, 1.0), 900_000.0, 38.0);
    b.light(Vec3::new(9.0, 6.5, -28.0), rgb(0.6, 0.82, 1.0), 900_000.0, 38.0);
    b.light(Vec3::new(0.0, 5.0, -36.0), rgb(0.7, 0.9, 1.0), 800_000.0, 36.0);
    b.light(Vec3::new(0.0, 8.0, -50.0), rgb(0.6, 0.85, 1.0), 1_200_000.0, 44.0);
}
