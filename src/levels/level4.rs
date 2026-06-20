// Tomb of the Sunken King — descends north (-Z) into the earth.
//
//   [ENTRANCE  roofless desert sky]   z 6..20   y=0
//        ||  obelisks ||                spawn @ z18, faces -Z
//        \/  (gap x[-2,2] @ z6)
//   [DESCENT STAIRS]  z 0..6  -> drops to y=-4
//        ||
//   [BURIAL HALL]  z -22..0  y=-4   pillars + gold sarcophagi
//        ||  (gap @ z-22)
//   [TRAP CORRIDOR]  z -34..-22  y=-4   sand pits L & R
//        ||
//   [KING'S VAULT]  z -52..-34  y=-4   raised gold dais + Silver Key
//        ||  locked stone DOOR (gap x[-2,2] @ z-52)
//   [EXIT TOMB]  z -64..-52  y=-4   exit slipgate

use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

pub fn build(b: &mut Build) {
    // Player starts in the sun-baked entrance facing north (-Z), down into the tomb.
    b.start.pos = Vec3::new(0.0, 1.0, 16.0);
    b.start.yaw = 0.0;

    let trim = b.theme.trim.clone();
    let metal = b.theme.metal.clone();
    let wall = b.theme.wall.clone();
    let accent = b.theme.accent.clone();

    // Warm emissive "torch" material for little wall sconces beside lights.
    let torch = b.mat(rgb(1.0, 0.55, 0.18), LinearRgba::rgb(6.0, 2.2, 0.4), 0.7, 0.0);
    // Lapis-blue glow for the dais and key shrine accents.
    let lapis = b.mat(rgb(0.2, 0.35, 0.9), LinearRgba::rgb(0.4, 0.9, 3.0), 0.4, 0.2);
    // Gold for sarcophagus lids / dais trim.
    let gold = b.mat(rgb(0.95, 0.8, 0.35), LinearRgba::rgb(0.6, 0.45, 0.1), 0.35, 0.85);

    // ============================================================
    // ENTRANCE — open desert sky, two great obelisks
    // ============================================================
    b.roofless(-9.0, 9.0, 6.0, 20.0, 0.0, 7.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-3.0, 0.1, 19.4), Vec3::new(3.0, 4.5, 19.8));

    // Two big obelisks flanking the descent (tall thin solids, tapering deco caps).
    b.solid(Vec3::new(-7.5, 0.0, 8.0), Vec3::new(-6.0, 9.0, 9.5), trim.clone());
    b.deco(Vec3::new(-7.2, 9.0, 8.3), Vec3::new(-6.3, 10.2, 9.2), gold.clone());
    b.solid(Vec3::new(6.0, 0.0, 8.0), Vec3::new(7.5, 9.0, 9.5), trim.clone());
    b.deco(Vec3::new(6.3, 9.0, 8.3), Vec3::new(7.2, 10.2, 9.2), gold.clone());

    // A low altar slab and starting supplies on the sand.
    b.slab(-2.0, 2.0, 14.0, 18.0, 0.0, 0.12, gold.clone());
    b.item(ItemKind::ArmorGreen, Vec3::new(-6.0, 0.6, 16.0));
    b.item(ItemKind::Shells(20), Vec3::new(6.0, 0.6, 16.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 0.6, 12.0));
    b.monster(Grunt, Vec3::new(-5.0, 1.0, 9.0));
    b.monster(Grunt, Vec3::new(5.0, 1.0, 9.0));

    // ============================================================
    // DESCENT — stair shaft dropping from y=0 to y=-4
    // ============================================================
    // Enclosed shaft over the stairs so we don't leak to the void.
    b.corridor_z(-2.0, 2.0, 0.0, 6.0, -4.0, 9.0);
    // Stairs marching north (-Z), treads span x[-2,2], from z=6 down to y=-4.
    b.stairs(-2.0, 2.0, 5.5, 0.0, -4.0, 8, Vec3::Z * -1.0);
    b.light(Vec3::new(0.0, 3.0, 3.0), rgb(1.0, 0.8, 0.5), 300_000.0, 22.0);

    // ============================================================
    // BURIAL HALL — torch-lit, pillars + gold sarcophagi
    // ============================================================
    b.room(-12.0, 12.0, -22.0, 0.0, -4.0, 8.0, &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);

    // Four sandstone pillars.
    for &(px, pz) in &[(-7.0, -6.0), (7.0, -6.0), (-7.0, -16.0), (7.0, -16.0)] {
        b.solid(Vec3::new(px - 1.0, -4.0, pz - 1.0), Vec3::new(px + 1.0, 4.0, pz + 1.0), trim.clone());
        // gold capital
        b.deco(Vec3::new(px - 1.2, 3.6, pz - 1.2), Vec3::new(px + 1.2, 4.0, pz + 1.2), gold.clone());
    }

    // Two gold-trim sarcophagi against the walls.
    b.solid(Vec3::new(-11.5, -4.0, -10.0), Vec3::new(-9.5, -2.6, -6.0), trim.clone());
    b.deco(Vec3::new(-11.6, -2.6, -10.1), Vec3::new(-9.4, -2.3, -5.9), gold.clone());
    b.solid(Vec3::new(9.5, -4.0, -10.0), Vec3::new(11.5, -2.6, -6.0), trim.clone());
    b.deco(Vec3::new(9.4, -2.6, -5.9), Vec3::new(11.6, -2.3, -10.1), gold.clone());

    // Torch sconces beside the lights.
    b.deco(Vec3::new(-11.8, -1.0, -3.0), Vec3::new(-11.4, 0.0, -2.4), torch.clone());
    b.deco(Vec3::new(11.4, -1.0, -3.0), Vec3::new(11.8, 0.0, -2.4), torch.clone());
    b.deco(Vec3::new(-11.8, -1.0, -19.0), Vec3::new(-11.4, 0.0, -18.4), torch.clone());
    b.deco(Vec3::new(11.4, -1.0, -19.0), Vec3::new(11.8, 0.0, -18.4), torch.clone());

    b.light(Vec3::new(-11.0, 0.5, -3.0), rgb(1.0, 0.6, 0.25), 360_000.0, 26.0);
    b.light(Vec3::new(11.0, 0.5, -3.0), rgb(1.0, 0.6, 0.25), 360_000.0, 26.0);
    b.light(Vec3::new(-11.0, 0.5, -19.0), rgb(1.0, 0.6, 0.25), 360_000.0, 26.0);
    b.light(Vec3::new(11.0, 0.5, -19.0), rgb(1.0, 0.6, 0.25), 360_000.0, 26.0);
    b.light(Vec3::new(0.0, 2.5, -11.0), rgb(1.0, 0.75, 0.45), 500_000.0, 30.0);

    // Mummy guards + a flying scarab-spirit.
    b.monster(Knight, Vec3::new(-4.0, -3.0, -11.0));
    b.monster(Knight, Vec3::new(4.0, -3.0, -11.0));
    b.monster(Grunt, Vec3::new(0.0, -3.0, -18.0));
    b.monster(Scrag, Vec3::new(-6.0, 1.0, -14.0));

    // Loot: Super Shotgun + shells here.
    b.item(ItemKind::WeaponSuperShotgun, Vec3::new(0.0, -2.9, -5.0));
    b.item(ItemKind::Shells(20), Vec3::new(-2.0, -3.4, -5.0));
    b.item(ItemKind::Health(25), Vec3::new(2.0, -3.4, -18.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(-9.0, -3.4, -18.0));

    // ============================================================
    // TRAP CORRIDOR — narrow safe path flanked by cursed sand pits
    // ============================================================
    // Wide chamber so the side pits are visible; only a central ledge is safe.
    b.room(-10.0, 10.0, -34.0, -22.0, -4.0, 7.0, &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);

    // Lower the side floors into pits and fill with cursed quicksand.
    // (room already laid a floor; drop deco hazard surfaces over carved pits)
    // Left pit
    b.solid(Vec3::new(-10.0, -7.0, -33.0), Vec3::new(-3.0, -6.5, -23.0), trim.clone());
    b.hazard(-10.0, -3.0, -33.0, -23.0, -5.6);
    // Right pit
    b.solid(Vec3::new(3.0, -7.0, -33.0), Vec3::new(10.0, -6.5, -23.0), trim.clone());
    b.hazard(3.0, 10.0, -33.0, -23.0, -5.6);

    // Central safe causeway (raised stone ledge) the player edges across.
    b.solid(Vec3::new(-3.0, -4.5, -34.0), Vec3::new(3.0, -4.0, -22.0), trim.clone());

    // Scarab-spirits harass from above the pits.
    b.monster(Scrag, Vec3::new(-6.0, -1.0, -28.0));
    b.monster(Scrag, Vec3::new(6.0, -1.0, -28.0));

    b.item(ItemKind::Nails(40), Vec3::new(0.0, -3.4, -28.0));
    b.light(Vec3::new(0.0, 2.0, -28.0), rgb(0.95, 0.85, 0.5), 600_000.0, 30.0);
    b.light(Vec3::new(-6.0, -1.0, -28.0), rgb(1.0, 0.7, 0.3), 250_000.0, 16.0);
    b.light(Vec3::new(6.0, -1.0, -28.0), rgb(1.0, 0.7, 0.3), 250_000.0, 16.0);

    // ============================================================
    // KING'S VAULT — raised gold dais, Silver Key, guarded
    // ============================================================
    b.room(-14.0, 14.0, -52.0, -34.0, -4.0, 9.0, &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);

    // Stepped raised dais in the center.
    b.solid(Vec3::new(-6.0, -4.0, -48.0), Vec3::new(6.0, -3.0, -40.0), trim.clone());
    b.solid(Vec3::new(-4.0, -3.0, -47.0), Vec3::new(4.0, -2.2, -41.0), trim.clone());
    b.deco(Vec3::new(-4.2, -2.2, -47.2), Vec3::new(4.2, -2.0, -40.8), gold.clone());
    // Stairs up onto the dais from the south.
    b.stairs(-4.0, 4.0, -38.5, -2.2, -4.0, 4, Vec3::Z * -1.0);

    // Lapis shrine pylons behind the key.
    b.deco(Vec3::new(-3.5, -2.0, -46.5), Vec3::new(-2.8, 1.0, -45.8), lapis.clone());
    b.deco(Vec3::new(2.8, -2.0, -46.5), Vec3::new(3.5, 1.0, -45.8), lapis.clone());

    // The Silver Key on the dais — guarded by the Sunken King (DeathKnight).
    b.item(ItemKind::SilverKey, Vec3::new(0.0, -1.9, -44.0));
    b.monster(DeathKnight, Vec3::new(0.0, -3.9, -37.0));

    // Side alcoves with loot and a Rocket Launcher to deal with the boss.
    b.item(ItemKind::WeaponRocket, Vec3::new(-11.0, -3.4, -44.0));
    b.item(ItemKind::Rockets(15), Vec3::new(-11.0, -3.4, -42.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(11.0, -3.4, -44.0));
    b.item(ItemKind::MegaHealth, Vec3::new(11.0, -3.4, -48.0));
    b.item(ItemKind::Nails(40), Vec3::new(-11.0, -3.4, -48.0));

    // Torches around the vault.
    b.deco(Vec3::new(-13.8, -1.0, -42.0), Vec3::new(-13.4, 0.0, -41.4), torch.clone());
    b.deco(Vec3::new(13.4, -1.0, -42.0), Vec3::new(13.8, 0.0, -41.4), torch.clone());
    b.light(Vec3::new(-12.0, 1.0, -43.0), rgb(1.0, 0.6, 0.25), 450_000.0, 28.0);
    b.light(Vec3::new(12.0, 1.0, -43.0), rgb(1.0, 0.6, 0.25), 450_000.0, 28.0);
    b.light(Vec3::new(0.0, 2.5, -44.0), rgb(0.85, 0.85, 1.0), 800_000.0, 34.0);

    // KEY-GRAB AMBUSH — guards burst from the vault corners.
    b.ambush(Ogre, Vec3::new(-10.0, -3.0, -36.0));
    b.ambush(Knight, Vec3::new(10.0, -3.0, -36.0));

    // ============================================================
    // LOCKED STONE DOOR + EXIT TOMB
    // ============================================================
    // Door fills the north gap of the vault (z=-52, gap x[-2,2]). Vault h=9 so
    // it can slide up out of the way.
    b.door(Vec3::new(-2.0, -4.0, -52.3), Vec3::new(2.0, 1.0, -51.7), Vec3::new(0.0, 5.5, 0.0));

    // Final tomb chamber with the exit slipgate.
    b.room(-8.0, 8.0, -64.0, -52.0, -4.0, 7.0, &[Wall::S((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-3.0, -3.9, -63.6), Vec3::new(3.0, 1.0, -63.2));
    b.exit(Vec3::new(0.0, -3.3, -62.5));

    // A last guard and a little parting health.
    b.monster(Grunt, Vec3::new(-5.0, -3.0, -58.0));
    b.item(ItemKind::Health(25), Vec3::new(5.0, -3.4, -58.0));
    b.item(ItemKind::Shells(20), Vec3::new(0.0, -3.4, -56.0));

    b.deco(Vec3::new(-7.8, -1.0, -60.0), Vec3::new(-7.4, 0.0, -59.4), torch.clone());
    b.deco(Vec3::new(7.4, -1.0, -60.0), Vec3::new(7.8, 0.0, -59.4), torch.clone());
    b.light(Vec3::new(0.0, 1.0, -58.0), rgb(1.0, 0.7, 0.35), 450_000.0, 26.0);
    b.light(Vec3::new(0.0, 0.0, -63.0), rgb(1.0, 0.85, 0.45), 500_000.0, 22.0);

    // Decorative slab trim using the accent on the entrance gateway floor.
    let _ = (metal, wall, accent);

    // ============================================================
    // SUN — harsh desert directional light
    // ============================================================
    b.sun(Vec3::new(14.0, 36.0, 24.0), Vec3::new(0.0, 0.0, 10.0), rgb(1.0, 0.92, 0.72), 3200.0);
}
