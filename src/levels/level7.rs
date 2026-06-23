use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

// ============================================================================
// SANCTUM OF THE VOID  —  the final level (very hard)
//
// Floating obsidian islands suspended in a starfield void. Crystal bridges
// (narrow violet-glowing spans) cross the lethal void-rift below. Fall = death.
//
// Top-down sketch (north = -Z, up the page). '#'=island, '='=crystal bridge,
// '~~~'=void rift far below everything:
//
//                          [ EXIT  ISLE ]            z = -70..-56
//                                ||  (final slipgate, WIN)
//                         [== locked gate ==]        z = -54
//                          [ RUNE  SANCTUM ]         z = -52..-34
//                          key altar + DeathKnight
//                                ||  bridge
//        [OGRE ISLE]======[ HUB  ATRIUM ]======[ENFORCER ISLE]
//         x -40..-26        x -8..8  z -24..-6      x 26..40
//                                ||  bridge
//                          [ START  ISLE ]          z = 6..20
//                          slipgate spawn
//
//   ~~~~~~~~~~~~~~~ VOID RIFT (instant death) far below at y = -12 ~~~~~~~~~~~~
// ============================================================================

pub fn build(b: &mut Build) {
    // Player spawns on the start island, facing north (toward the hub).
    b.start.pos = Vec3::new(0.0, 1.0, 16.0);
    b.start.yaw = 0.0;

    // Handles.
    let metal = b.theme.metal.clone();
    let trim = b.theme.trim.clone();
    let floor = b.theme.floor.clone();
    let wall = b.theme.wall.clone();

    // Custom emissive materials: violet crystal (bridges & clusters) and pale
    // starlight pinpoints for the void backdrop.
    let crystal = b.mat(rgb(0.55, 0.3, 0.95), LinearRgba::rgb(1.4, 0.4, 3.0), 0.25, 0.1);
    let crystal_dim = b.mat(rgb(0.4, 0.25, 0.7), LinearRgba::rgb(0.6, 0.2, 1.4), 0.3, 0.1);
    let star = b.mat(rgb(0.9, 0.9, 1.0), LinearRgba::rgb(2.0, 2.0, 3.0), 0.4, 0.0);
    let obsidian = b.mat(rgb(0.05, 0.04, 0.08), LinearRgba::rgb(0.0, 0.0, 0.05), 0.35, 0.2);

    // ---- The bottomless VOID RIFT --------------------------------------
    // One huge hazard plane far below the whole map; any fall lands lethally
    // in it. The islands float above it; the gaps between them are the void.
    b.hazard(-60.0, 60.0, -84.0, 36.0, -12.0);

    // Starfield: scatter tiny glowing star cubes deep in the void as backdrop.
    let stars = [
        (-50.0, -30.0, -70.0), (45.0, -22.0, -60.0), (-30.0, -40.0, 20.0),
        (20.0, -34.0, -10.0), (-15.0, -28.0, -50.0), (38.0, -45.0, 10.0),
        (-48.0, -18.0, -20.0), (10.0, -50.0, -78.0), (52.0, -36.0, -40.0),
        (-22.0, -55.0, 0.0), (5.0, -26.0, 30.0), (-40.0, -48.0, -55.0),
    ];
    for (sx, sy, sz) in stars {
        b.deco(Vec3::new(sx - 0.3, sy - 0.3, sz - 0.3), Vec3::new(sx + 0.3, sy + 0.3, sz + 0.3), star.clone());
    }

    // ===================================================================
    // START ISLAND  (spawn)  x[-8,8] z[6,20]
    // ===================================================================
    b.room(-8.0, 8.0, 6.0, 20.0, 0.0, 6.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 19.4), Vec3::new(2.0, 4.0, 19.8));
    // crystal trim along the island rim.
    b.slab(-8.0, 8.0, 6.0, 6.4, 0.0, 0.25, crystal_dim.clone());
    b.item(ItemKind::ArmorGreen, Vec3::new(-5.0, 0.7, 9.0));
    b.item(ItemKind::Shells(30), Vec3::new(5.0, 0.7, 9.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 0.7, 9.0));
    b.monster(Knight, Vec3::new(-4.0, 1.0, 14.0));
    b.monster(Knight, Vec3::new(4.0, 1.0, 14.0));
    crystal_cluster(b, &crystal, -7.5, 7.0, 18.0);
    crystal_cluster(b, &crystal, 7.5, 7.0, 18.0);

    // ---- crystal bridge: start -> hub (gap x[-2,2], z from 6 down to -6) ----
    crystal_bridge_z(b, &crystal, &obsidian, -2.0, 2.0, -6.0, 6.0);

    // ===================================================================
    // HUB ATRIUM  (central crossroads)  x[-8,8] z[-24,-6]
    // ===================================================================
    b.room(-8.0, 8.0, -24.0, -6.0, 0.0, 9.0, &[
        Wall::S((-2.0, 2.0)),    // back to start
        Wall::N((-2.0, 2.0)),    // forward to sanctum
        Wall::W((-18.0, -12.0)), // to ogre isle
        Wall::E((-18.0, -12.0)), // to enforcer isle
    ]);
    // central rune pedestal (deco) and rim crystals.
    b.slab(-4.0, 4.0, -18.0, -12.0, 0.05, 0.06, crystal_dim.clone());
    b.solid(Vec3::new(-1.5, 0.0, -16.5), Vec3::new(1.5, 1.2, -13.5), obsidian.clone());
    crystal_cluster(b, &crystal, 0.0, 1.2, -15.0);
    // overhead light pillars (deco) framing the hub.
    b.deco(Vec3::new(-7.5, 0.0, -23.5), Vec3::new(-6.5, 9.0, -22.5), crystal_dim.clone());
    b.deco(Vec3::new(6.5, 0.0, -23.5), Vec3::new(7.5, 9.0, -22.5), crystal_dim.clone());
    b.monster(Enforcer, Vec3::new(-6.0, 1.0, -20.0));
    b.monster(Enforcer, Vec3::new(6.0, 1.0, -20.0));
    b.monster(Scrag, Vec3::new(0.0, 5.0, -19.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 0.7, -9.0));
    b.item(ItemKind::Health(25), Vec3::new(6.0, 0.7, -9.0));

    // ===================================================================
    // OGRE ISLE  (west)  x[-44,-26] z[-22,-8]  — strong weapon: Rocket
    // ===================================================================
    // A WEAVER silk strand replaces the crystal bridge to this optional loot
    // ledge: a walkable span you can only use while the spider guarding it lives.
    // Kill the Weaver and the strand drops — so reaching the Rocket Launcher and
    // getting back out is a contested tightrope (the ogre isle is off the
    // key/exit critical path, so a severed strand never blocks progression).
    // Anchors are the ogre-isle edge (x=-26) and the hub west floor edge (x=-8),
    // top y=0 — spanning the full void gap so the strand is flush with both island
    // floors (mirrors the east side's full-gap crystal_bridge_x 8->26). The hub-ward
    // end sits in the hub's west doorway (z=-15 lies inside its Wall::W z[-18,-12]).
    b.weaver(
        Vec3::new(-28.5, 1.0, -15.0),       // spider on the ogre-isle ledge near anchor a
        Vec3::new(-26.0, 0.0, -15.0),       // anchor a (ogre isle edge)
        Vec3::new(-8.0, 0.0, -15.0),        // anchor b (hub west floor edge)
    );
    b.room(-44.0, -26.0, -22.0, -8.0, 0.0, 8.0, &[Wall::E((-18.0, -12.0))]);
    b.slab(-44.0, -26.0, -22.0, -21.6, 0.0, 0.25, crystal_dim.clone());
    // raised obsidian ledge with the rocket launcher (ogre perch).
    b.solid(Vec3::new(-42.0, 0.0, -21.0), Vec3::new(-36.0, 2.5, -16.0), obsidian.clone());
    b.monster(Ogre, Vec3::new(-39.0, 3.1, -18.5));
    b.monster(Ogre, Vec3::new(-30.0, 1.0, -12.0));
    b.item(ItemKind::WeaponRocket, Vec3::new(-39.0, 3.2, -18.5));
    b.item(ItemKind::Rockets(25), Vec3::new(-37.0, 3.2, -18.5));
    b.item(ItemKind::Rockets(25), Vec3::new(-30.0, 0.7, -10.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(-41.0, 0.7, -10.0));
    crystal_cluster(b, &crystal, -43.0, 2.0, -21.0);
    crystal_cluster(b, &crystal, -27.5, 0.0, -9.0);

    // ===================================================================
    // ENFORCER ISLE  (east)  x[26,44] z[-22,-8]  — strong weapon: Lightning
    // ===================================================================
    crystal_bridge_x(b, &crystal, &obsidian, 8.0, 26.0, -18.0, -12.0);
    b.room(26.0, 44.0, -22.0, -8.0, 0.0, 8.0, &[Wall::W((-18.0, -12.0))]);
    b.slab(26.0, 44.0, -22.0, -21.6, 0.0, 0.25, crystal_dim.clone());
    // stair up to a small crystal shrine holding the Lightning Gun.
    b.stairs(38.0, 42.0, -10.0, 2.5, 0.0, 5, Vec3::Z * -1.0);
    b.solid(Vec3::new(36.0, 0.0, -21.0), Vec3::new(42.0, 2.5, -16.0), obsidian.clone());
    b.monster(Enforcer, Vec3::new(30.0, 1.0, -12.0));
    b.monster(Enforcer, Vec3::new(38.0, 1.0, -11.0));
    b.monster(Scrag, Vec3::new(33.0, 5.0, -16.0));
    b.item(ItemKind::WeaponLightning, Vec3::new(39.0, 3.2, -18.5));
    b.item(ItemKind::Cells(75), Vec3::new(37.5, 3.2, -18.5));
    b.item(ItemKind::Cells(50), Vec3::new(28.0, 0.7, -10.0));
    b.item(ItemKind::Nails(60), Vec3::new(42.0, 0.7, -10.0));
    crystal_cluster(b, &crystal, 27.0, 2.0, -21.0);
    crystal_cluster(b, &crystal, 43.0, 0.0, -9.0);

    // ---- crystal bridge: hub -> sanctum (gap x[-2,2], z from -24 to -34) ----
    crystal_bridge_z(b, &crystal, &obsidian, -2.0, 2.0, -34.0, -24.0);

    // ===================================================================
    // RUNE SANCTUM  (the grand center)  x[-16,16] z[-52,-34]
    // Silver Key on a raised altar, guarded by the DeathKnight boss.
    // North wall has the LOCKED GATE gap (x[-3,3]).
    // ===================================================================
    b.room(-16.0, 16.0, -52.0, -34.0, 0.0, 12.0, &[
        Wall::S((-2.0, 2.0)),  // back to hub bridge
        Wall::N((-3.0, 3.0)),  // locked gate to exit
    ]);
    // Rune circle on the floor.
    b.slab(-10.0, 10.0, -49.0, -39.0, 0.05, 0.06, crystal_dim.clone());
    // Central altar: stepped obsidian dais holding the key.
    b.solid(Vec3::new(-4.0, 0.0, -47.0, ), Vec3::new(4.0, 1.0, -41.0), obsidian.clone());
    b.solid(Vec3::new(-2.5, 1.0, -45.5), Vec3::new(2.5, 1.8, -42.5), obsidian.clone());
    crystal_cluster(b, &crystal, 0.0, 1.8, -44.0);
    // Crystal monoliths in the four corners.
    crystal_monolith(b, &crystal, &obsidian, -14.0, -50.0);
    crystal_monolith(b, &crystal, &obsidian, 14.0, -50.0);
    crystal_monolith(b, &crystal, &obsidian, -14.0, -36.0);
    crystal_monolith(b, &crystal, &obsidian, 14.0, -36.0);

    // The boss + supporting guard.
    b.monster(DeathKnight, Vec3::new(0.0, 2.0, -44.0));
    b.monster(Knight, Vec3::new(-10.0, 1.0, -38.0));
    b.monster(Knight, Vec3::new(10.0, 1.0, -38.0));

    // The Silver Key on the altar.
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 2.0, -44.0));
    // Strongest support loadout near the fight.
    b.item(ItemKind::MegaHealth, Vec3::new(-12.0, 0.8, -42.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(12.0, 0.8, -42.0));
    b.item(ItemKind::Health(25), Vec3::new(-12.0, 0.8, -48.0));
    b.item(ItemKind::Health(25), Vec3::new(12.0, 0.8, -48.0));
    b.item(ItemKind::Rockets(15), Vec3::new(-6.0, 0.8, -50.0));
    b.item(ItemKind::Cells(50), Vec3::new(6.0, 0.8, -50.0));

    // ---- BIG AMBUSH: reinforcements that teleport in on key grab. ----
    b.ambush(Ogre, Vec3::new(-12.0, 1.0, -50.0));
    b.ambush(Ogre, Vec3::new(12.0, 1.0, -50.0));
    b.ambush(Knight, Vec3::new(-12.0, 1.0, -36.0));
    b.ambush(Knight, Vec3::new(12.0, 1.0, -36.0));
    b.ambush(Scrag, Vec3::new(-6.0, 5.0, -40.0));
    b.ambush(Scrag, Vec3::new(6.0, 5.0, -40.0));
    b.ambush(Enforcer, Vec3::new(0.0, 1.0, -50.0));

    // ===================================================================
    // LOCKED OBSIDIAN GATE  (north wall of sanctum, gap x[-3,3], z=-52)
    // Slides up out of the way once the Silver Key is held.
    // ===================================================================
    b.door(Vec3::new(-3.0, 0.0, -52.3), Vec3::new(3.0, 6.0, -51.7), Vec3::new(0.0, 6.4, 0.0));

    // ---- crystal bridge: gate -> exit isle (gap x[-3,3], z -56 to -52) ----
    crystal_bridge_z(b, &crystal, &obsidian, -3.0, 3.0, -56.0, -52.0);

    // ===================================================================
    // EXIT ISLE  (final)  x[-10,10] z[-70,-56]  — the void-rift slipgate.
    // Reaching it WINS the whole game.
    // ===================================================================
    b.room(-10.0, 10.0, -70.0, -56.0, 0.0, 7.0, &[Wall::S((-3.0, 3.0))]);
    b.slab(-10.0, 10.0, -69.6, -69.2, 0.0, 0.25, crystal_dim.clone());
    b.slipgate(Vec3::new(-3.0, 0.1, -69.8), Vec3::new(3.0, 5.0, -69.4));
    b.exit(Vec3::new(0.0, 1.5, -68.0));
    crystal_cluster(b, &crystal, -9.0, 2.0, -57.5);
    crystal_cluster(b, &crystal, 9.0, 2.0, -57.5);
    // A last cache for the finish line.
    b.item(ItemKind::Health(25), Vec3::new(-7.0, 0.7, -58.0));
    b.item(ItemKind::Health(25), Vec3::new(7.0, 0.7, -58.0));

    // suppress unused-handle warnings for the shared theme handles we kept.
    let _ = (&metal, &trim, &floor, &wall);

    // ===================================================================
    // LIGHTS  (eerie violet + cold starlight — every island lit)
    // ===================================================================
    b.sun(Vec3::new(-20.0, 40.0, -30.0), Vec3::new(0.0, 0.0, -40.0), rgb(0.45, 0.35, 0.7), 1400.0);

    b.light(Vec3::new(0.0, 4.5, 13.0), rgb(0.7, 0.45, 1.0), 700_000.0, 36.0);   // start
    b.light(Vec3::new(0.0, 7.0, -15.0), rgb(0.65, 0.4, 1.0), 1_300_000.0, 44.0); // hub
    b.light(Vec3::new(-35.0, 5.0, -15.0), rgb(0.8, 0.4, 1.0), 1_000_000.0, 40.0); // ogre isle
    b.light(Vec3::new(35.0, 5.0, -15.0), rgb(0.55, 0.55, 1.0), 1_000_000.0, 40.0); // enforcer isle
    b.light(Vec3::new(0.0, 9.0, -43.0), rgb(0.85, 0.45, 1.0), 1_900_000.0, 55.0); // sanctum (boss)
    b.light(Vec3::new(0.0, 3.0, -44.0), rgb(1.0, 0.5, 1.0), 500_000.0, 18.0);     // altar glow
    b.light(Vec3::new(0.0, 5.0, -62.0), rgb(0.7, 0.4, 1.0), 900_000.0, 40.0);     // exit
}

// ---------------------------------------------------------------------------
// Helpers — reusable themed motifs.
// ---------------------------------------------------------------------------

/// A cluster of glowing violet crystal shards jutting up from (x,base_y,z).
fn crystal_cluster(b: &mut Build, crystal: &Handle<StandardMaterial>, x: f32, base_y: f32, z: f32) {
    b.deco(Vec3::new(x - 0.35, base_y, z - 0.35), Vec3::new(x + 0.35, base_y + 1.6, z + 0.35), crystal.clone());
    b.deco(Vec3::new(x - 0.6, base_y, z + 0.1), Vec3::new(x - 0.15, base_y + 1.0, z + 0.55), crystal.clone());
    b.deco(Vec3::new(x + 0.15, base_y, z - 0.55), Vec3::new(x + 0.55, base_y + 1.2, z - 0.1), crystal.clone());
}

/// A tall crystal-capped obsidian monolith standing at (x, z) on the floor.
fn crystal_monolith(
    b: &mut Build,
    crystal: &Handle<StandardMaterial>,
    obsidian: &Handle<StandardMaterial>,
    x: f32,
    z: f32,
) {
    b.solid(Vec3::new(x - 1.0, 0.0, z - 1.0), Vec3::new(x + 1.0, 4.5, z + 1.0), obsidian.clone());
    b.deco(Vec3::new(x - 0.7, 4.5, z - 0.7), Vec3::new(x + 0.7, 6.5, z + 0.7), crystal.clone());
}

/// A narrow glowing crystal bridge running along Z, spanning x[x0,x1] across
/// the void from z0 to z1. Obsidian deck + violet glowing rails/underside.
fn crystal_bridge_z(
    b: &mut Build,
    crystal: &Handle<StandardMaterial>,
    obsidian: &Handle<StandardMaterial>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
) {
    // deck (walkable, solid).
    b.solid(Vec3::new(x0, -0.5, z0), Vec3::new(x1, 0.0, z1), obsidian.clone());
    // glowing underside vein.
    b.deco(Vec3::new(x0 + 0.2, -0.7, z0), Vec3::new(x1 - 0.2, -0.5, z1), crystal.clone());
    // low glowing rails along both edges.
    b.deco(Vec3::new(x0 - 0.15, 0.0, z0), Vec3::new(x0 + 0.15, 0.5, z1), crystal.clone());
    b.deco(Vec3::new(x1 - 0.15, 0.0, z0), Vec3::new(x1 + 0.15, 0.5, z1), crystal.clone());
}

/// A narrow glowing crystal bridge running along X, spanning z[z0,z1] across
/// the void from x0 to x1.
fn crystal_bridge_x(
    b: &mut Build,
    crystal: &Handle<StandardMaterial>,
    obsidian: &Handle<StandardMaterial>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
) {
    b.solid(Vec3::new(x0, -0.5, z0), Vec3::new(x1, 0.0, z1), obsidian.clone());
    b.deco(Vec3::new(x0, -0.7, z0 + 0.2), Vec3::new(x1, -0.5, z1 - 0.2), crystal.clone());
    b.deco(Vec3::new(x0, 0.0, z0 - 0.15), Vec3::new(x1, 0.5, z0 + 0.15), crystal.clone());
    b.deco(Vec3::new(x0, 0.0, z1 - 0.15), Vec3::new(x1, 0.5, z1 + 0.15), crystal.clone());
}
