use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

// THE SALT WRAITH — a sky-pirate galleon strung across the clouds.
// Travel runs NORTH (-Z). Separate floating hulls are joined by narrow planks
// over a glowing plasma-engine exhaust hazard far below.
//
//                              (stern, +Z back .......... bow, -Z front)
//
//   [WEATHER DECK]==plank==[CANNON DECK]==plank==[CAPTAIN'S CABIN VAULT]==hatch==[STERN EXIT]
//     masts+sails            brass cannons          Silver Key + ambush         slipgate
//     crow's-nest(^stairs)   plasma gap below       (locked brass door)
//        spawn
//
//   Scrags wheel overhead the whole flight; Enforcer gunners man the rails,
//   Knight cutlass-boarders rush the planks, an Ogre brute guards the cabin.

pub fn build(b: &mut Build) {
    // ---- player spawn: aft of the weather deck, facing the bow (-Z) ----
    // (z=11.5 keeps the spawn box clear of the aft mast at z[9.0,9.8] — spawning
    // at z=10 overlapped it and wedged the player on level start.)
    b.start.pos = Vec3::new(0.0, 1.0, 11.5);
    b.start.yaw = 0.0;

    // handy material clones / customs
    let _metal = b.theme.metal.clone();
    let trim = b.theme.trim.clone();
    let wood = b.theme.floor.clone();
    let brass = b.theme.door.clone();
    // glowing energy sail + plasma glow materials
    let sail = b.mat(rgb(0.45, 0.75, 1.0), LinearRgba::rgb(0.5, 1.4, 3.0), 0.4, 0.1);
    let plasma = b.mat(rgb(0.3, 0.6, 1.0), LinearRgba::rgb(0.6, 1.8, 3.5), 0.3, 0.0);

    // bright daytime sky + a warm-cool light mix
    b.sun(Vec3::new(-20.0, 40.0, 20.0), Vec3::new(0.0, 0.0, -20.0), rgb(1.0, 0.95, 0.85), 3200.0);

    // ======================================================================
    // 1) WEATHER DECK  (open sky)  x[-9,9]  z[-4,14]
    //    high railings (h=4) with a bow doorway gap to the first plank.
    // ======================================================================
    b.roofless(-9.0, 9.0, -4.0, 14.0, 0.0, 4.0, &[Wall::N((-2.0, 2.0))]);

    // spawn slipgate at the stern rail
    b.slipgate(Vec3::new(-2.5, 0.1, 13.5), Vec3::new(2.5, 3.6, 13.9));

    // two tall masts with crossed energy sails
    b.solid(Vec3::new(-0.4, 0.0, 4.0), Vec3::new(0.4, 12.0, 4.8), trim.clone());
    b.solid(Vec3::new(-0.4, 0.0, 9.0), Vec3::new(0.4, 13.0, 9.8), trim.clone());
    // sail quads (deco, big emissive)
    b.deco(Vec3::new(-5.0, 5.0, 4.3), Vec3::new(5.0, 10.5, 4.5), sail.clone());
    b.deco(Vec3::new(-6.0, 5.5, 9.3), Vec3::new(6.0, 11.5, 9.5), sail.clone());
    // yard-arms (cross spars)
    b.deco(Vec3::new(-5.2, 9.8, 4.1), Vec3::new(5.2, 10.2, 4.7), trim.clone());
    b.deco(Vec3::new(-6.2, 10.8, 9.1), Vec3::new(6.2, 11.2, 9.5), trim.clone());

    // crow's-nest: a railed platform up the bow mast, climbed by stairs, holds ammo
    // stairs up the west side from deck to the nest platform at y=4.0
    b.stairs(-8.5, -6.5, 12.5, 4.0, 0.0, 8, Vec3::Z * -1.0);
    // nest platform (a small floating crate-walk along the west rail)
    b.solid(Vec3::new(-9.0, 3.5, 2.0), Vec3::new(-6.0, 4.0, 8.0), wood.clone());
    // nest railing nub so you feel perched
    b.solid(Vec3::new(-9.0, 4.0, 2.0), Vec3::new(-8.6, 5.2, 8.0), trim.clone());
    b.item(ItemKind::Nails(75), Vec3::new(-7.5, 4.6, 4.0));
    b.item(ItemKind::Shells(20), Vec3::new(-7.5, 4.6, 6.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(-7.5, 4.6, 7.0));

    // deck loot + early extra weapon
    b.item(ItemKind::WeaponSuperShotgun, Vec3::new(4.0, 0.6, 7.0));
    b.item(ItemKind::Shells(20), Vec3::new(6.0, 0.6, 6.0));
    b.item(ItemKind::Health(25), Vec3::new(6.0, 0.6, 10.0));

    // boarders + a sky-pirate gunner + wheeling scrags
    b.monster(Knight, Vec3::new(2.0, 1.0, 2.0));
    b.monster(Enforcer, Vec3::new(-4.0, 1.0, 0.0));
    b.monster(Scrag, Vec3::new(3.0, 5.0, 1.0));
    b.monster(Scrag, Vec3::new(-3.0, 6.0, 6.0));

    b.light(Vec3::new(0.0, 6.0, 8.0), rgb(1.0, 0.8, 0.55), 900_000.0, 40.0);
    b.light(Vec3::new(0.0, 6.0, 0.0), rgb(0.6, 0.85, 1.0), 700_000.0, 36.0);

    // ======================================================================
    // 2) PLANK BRIDGE #1  over the plasma engine exhaust  z[-12,-4]
    //    narrow walkway in the x[-2,2] corridor; hazard glowing below.
    // ======================================================================
    // the plank (narrow solid, no rails — deliberate edges)
    b.solid(Vec3::new(-1.6, 0.0, -12.0), Vec3::new(1.6, 0.4, -4.0), wood.clone());
    // plasma exhaust far below the gap
    b.hazard(-8.0, 8.0, -12.0, -4.0, -8.0);
    // a faint glow box to read the engine
    b.deco(Vec3::new(-7.0, -8.5, -11.0), Vec3::new(7.0, -7.5, -5.0), plasma.clone());
    b.light(Vec3::new(0.0, -3.0, -8.0), rgb(0.4, 0.8, 1.0), 1_200_000.0, 30.0);

    // a scrag harasses the crossing
    b.monster(Scrag, Vec3::new(5.0, 4.0, -8.0));

    // ======================================================================
    // 3) CANNON DECK  (open sky)  x[-10,10]  z[-26,-12]
    //    brass cannons along the rails; gunners; second weapon + ammo + armor.
    // ======================================================================
    b.roofless(-10.0, 10.0, -26.0, -12.0, 0.0, 4.0,
        &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);

    // brass cannons (barrel + carriage) along both rails, pointing outward
    let cannon = |b: &mut Build, x: f32, z: f32, m: &Handle<StandardMaterial>| {
        b.solid(Vec3::new(x - 0.4, 0.0, z - 0.6), Vec3::new(x + 0.4, 0.7, z + 0.6), trim.clone());
        b.solid(Vec3::new(x - 0.3, 0.5, z - 0.4), Vec3::new(x + 0.3, 1.0, z + 0.4), m.clone());
        // barrel poking over the rail (deco)
        b.deco(Vec3::new(x - 0.18, 0.6, z - 1.6), Vec3::new(x + 0.18, 0.9, z - 0.2), m.clone());
    };
    cannon(b, -8.5, -15.0, &brass);
    cannon(b, -8.5, -20.0, &brass);
    cannon(b, 8.5, -15.0, &brass);
    cannon(b, 8.5, -20.0, &brass);

    // a powder-store of loot amidships
    b.item(ItemKind::WeaponGrenade, Vec3::new(0.0, 0.6, -16.0));
    b.item(ItemKind::Rockets(10), Vec3::new(-2.0, 0.6, -16.0));
    b.item(ItemKind::WeaponRocket, Vec3::new(2.0, 0.6, -22.0));
    b.item(ItemKind::Rockets(15), Vec3::new(0.0, 0.6, -22.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(-6.0, 0.6, -24.0));
    b.item(ItemKind::Health(25), Vec3::new(6.0, 0.6, -24.0));
    b.item(ItemKind::Shells(20), Vec3::new(-6.0, 0.6, -14.0));

    // defenders: gunners + boarders + an overhead scrag
    b.monster(Enforcer, Vec3::new(-6.0, 1.0, -19.0));
    b.monster(Enforcer, Vec3::new(6.0, 1.0, -19.0));
    b.monster(Knight, Vec3::new(0.0, 1.0, -19.0));
    b.monster(Scrag, Vec3::new(0.0, 5.0, -20.0));

    b.light(Vec3::new(0.0, 6.0, -16.0), rgb(1.0, 0.78, 0.5), 1_000_000.0, 42.0);
    b.light(Vec3::new(0.0, 6.0, -24.0), rgb(0.6, 0.85, 1.0), 800_000.0, 38.0);

    // ======================================================================
    // 4) PLANK BRIDGE #2  over plasma  z[-34,-26]
    // ======================================================================
    b.solid(Vec3::new(-1.6, 0.0, -34.0), Vec3::new(1.6, 0.4, -26.0), wood.clone());
    b.hazard(-8.0, 8.0, -34.0, -26.0, -8.0);
    b.deco(Vec3::new(-7.0, -8.5, -33.0), Vec3::new(7.0, -7.5, -27.0), plasma.clone());
    b.light(Vec3::new(0.0, -3.0, -30.0), rgb(0.4, 0.8, 1.0), 1_200_000.0, 30.0);

    // ======================================================================
    // 5) CAPTAIN'S CABIN VAULT  (enclosed room)  x[-9,9]  z[-50,-34]
    //    holds the Silver Key, guarded by an Ogre brute + key-grab ambush.
    //    South doorway gap takes the plank; east gap leads to the stern hatch.
    // ======================================================================
    b.room(-9.0, 9.0, -50.0, -34.0, 0.0, 7.0,
        &[Wall::S((-2.0, 2.0)), Wall::E((-44.0, -40.0))]);

    // captain's dais where the key rests
    b.solid(Vec3::new(-2.5, 0.0, -47.0), Vec3::new(2.5, 0.6, -43.0), trim.clone());
    b.slab(-2.5, 2.5, -47.0, -43.0, 0.6, 0.06, sail.clone());
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 1.0, -45.0));

    // vault loot
    b.item(ItemKind::MegaHealth, Vec3::new(-6.0, 0.8, -47.0));
    b.item(ItemKind::Cells(40), Vec3::new(6.0, 0.8, -47.0));
    b.item(ItemKind::Rockets(10), Vec3::new(-6.0, 0.8, -37.0));
    b.item(ItemKind::Nails(50), Vec3::new(6.0, 0.8, -37.0));

    // brute guarding the key
    b.monster(Ogre, Vec3::new(0.0, 1.0, -48.0));
    b.monster(Knight, Vec3::new(-6.0, 1.0, -41.0));

    // classic key-grab ambush: boarders teleport in behind you
    b.ambush(Knight, Vec3::new(-4.0, 1.0, -36.0));
    b.ambush(Enforcer, Vec3::new(4.0, 1.0, -36.0));

    b.light(Vec3::new(0.0, 5.5, -42.0), rgb(1.0, 0.8, 0.55), 1_100_000.0, 44.0);
    b.light(Vec3::new(0.0, 4.0, -45.0), rgb(0.6, 0.85, 1.0), 600_000.0, 30.0);

    // ======================================================================
    // 6) LOCKED BRASS HATCH + STERN CORRIDOR + EXIT
    //    The east wall gap z[-44,-40] is sealed by a brass door that slides up
    //    once the Silver Key is held. A short corridor leads east to the exit.
    // ======================================================================
    // locked brass hatch (thin slab filling the gap; room h=7 > door h=5)
    b.door(Vec3::new(8.7, 0.0, -44.0), Vec3::new(9.3, 5.0, -40.0), Vec3::new(0.0, 5.4, 0.0));

    // stern gangway running east to the exit slab
    b.corridor_x(9.0, 22.0, -44.0, -40.0, 0.0, 5.0);

    // ======================================================================
    // 7) STERN EXIT PLATFORM (open sky)  x[22,34]  z[-48,-36]
    // ======================================================================
    b.roofless(22.0, 34.0, -48.0, -36.0, 0.0, 4.0, &[Wall::W((-44.0, -40.0))]);
    b.slipgate(Vec3::new(26.0, 0.1, -47.6), Vec3::new(31.0, 3.6, -47.2));
    b.exit(Vec3::new(28.5, 1.5, -46.0));

    // a final scrag and parting gunner
    b.monster(Scrag, Vec3::new(28.0, 5.0, -42.0));
    b.monster(Enforcer, Vec3::new(31.0, 1.0, -38.0));
    b.item(ItemKind::Health(25), Vec3::new(24.0, 0.6, -38.0));
    b.item(ItemKind::Shells(20), Vec3::new(32.0, 0.6, -46.0));

    b.light(Vec3::new(28.0, 6.0, -42.0), rgb(0.7, 0.9, 1.0), 900_000.0, 40.0);
    b.light(Vec3::new(15.0, 4.0, -42.0), rgb(1.0, 0.8, 0.55), 600_000.0, 28.0);
}
