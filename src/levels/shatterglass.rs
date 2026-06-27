//! Campaign level 9 — **Shatterglass Vein** (`ThemeId::Crystal`).
//!
//! A radiant crystal mine: near-black rock blazing with glassy cyan and magenta
//! veins, geodes and spires. Like Blackvein Deep it descends in stages — an entry
//! geode-cavern cleared on foot, a stepped descent through two crystal galleries,
//! and then the headline **mine-cart line** (feature 45): a single rail spine that
//! arcs between towering crystal spires **over a lethal radiant ENERGY RIFT** —
//! a glowing magenta chasm splitting a vast cavern — all the way to the vault
//! floor. You ride the cart down fighting from the deck and *shoot the world to
//! steer*:
//!   - a **junction switch** part-way out throws the cart onto a high eastern
//!     branch that runs the deck straight through a **crystal nest** (Ogres +
//!     Scrags, with the branch's reward stash) instead of the low main line, and
//!   - a **weak rail joint** hung out over the open rift can be shot to **derail**
//!     — but here that is a pure gamble of nerve: the cart tumbles off the spline
//!     into free physics with nothing under it but the rift, so derailing here
//!     plunges the rider into the radiant chasm and kills them. The bold flex of a
//!     high-speed run; a trap for the panicked trigger finger.
//!
//! The Silver Key sits on the crystal vault floor where every line comes to rest;
//! a Death Knight holds it, a locked door gates the exit slipgate beyond. Unlike
//! its purple neighbour Sanctum of the Void, this cavern is a LUMINOUS crystal
//! mine — self-lit cyan and magenta crystal clusters, glowing veins and a brilliant
//! rift throw colored light up the near-black rock so the two crystalline maps read
//! apart at a glance. Falling off the cart into the rift is fatal: the radiant glow
//! burns and the kill plane far below catches any fast faller clean.

use crate::common::rgb;
use crate::level::{Build, ItemKind, MonsterKind::*, Wall};
use crate::physics::Aabb;
use bevy::prelude::*;

/// The "fell out of the world" kill plane: anything whose center drops below this
/// dies instantly. Set well below the vault floor (y=0) and the rift's radiant
/// surface so a fall off the cart into the energy rift is unconditionally fatal,
/// even for a fast faller that would otherwise tunnel a thin damage volume.
const VOID_KILL_Y: f32 = -9.0;

pub fn build(b: &mut Build) {
    // Player spawns on the high entry-cavern floor, facing north (-Z) into the mine.
    b.start.pos = Vec3::new(0.0, 19.0, 20.0);
    b.start.yaw = 0.0;

    // Theme handles + a set of self-lit crystal surfaces. The crystals are the
    // whole point — heavy emissive so the near-black rock reads as a glowing mine.
    let floor = b.theme.floor.clone();
    let wall = b.theme.wall.clone();
    let ceil = b.theme.ceiling.clone();
    let trim = b.theme.trim.clone();
    // Cyan crystal: the dominant cool glow (clusters, spires, geodes).
    let cyan = b.mat(rgb(0.45, 0.95, 1.0), LinearRgba::rgb(0.4, 3.2, 4.4), 0.2, 0.1);
    // Magenta crystal: the rift's color, echoed in scattered clusters so the eye
    // ties the warm chasm to the cooler cavern.
    let magenta = b.mat(rgb(1.0, 0.35, 0.9), LinearRgba::rgb(3.8, 0.5, 3.4), 0.2, 0.1);
    // Teal crystal: a third accent so the geodes aren't two-tone.
    let teal = b.mat(rgb(0.35, 1.0, 0.85), LinearRgba::rgb(0.5, 3.4, 3.0), 0.2, 0.1);
    // The radiant rift surface itself: blinding magenta energy, far brighter than
    // any wall crystal so the chasm screams "lethal" from across the cavern.
    let rift = b.mat(rgb(1.0, 0.25, 0.85), LinearRgba::rgb(7.0, 0.9, 6.0), 0.3, 0.0);

    // ======================================================================
    // ENTRY GEODE-CAVERN  x[-14,14] z[6,24]  floor y=18  h=9  — on-foot warm-up.
    //   The cracked-open mouth of the vein. Clear the guards, grab the loadout,
    //   then drop north through the two crystal galleries to the cart.
    // ======================================================================
    b.room(-14.0, 14.0, 6.0, 24.0, 18.0, 9.0, &[Wall::N((-3.0, 3.0))]);
    b.slipgate(Vec3::new(-2.0, 18.1, 23.4), Vec3::new(2.0, 22.0, 23.8)); // arrival slip
    // Geode clusters bursting from the floor and walls — the cavern's character.
    // A glowing crystal gateway flanks the descent gap dead ahead so the very first
    // view reads as a luminous crystal mine, not a dark hall.
    crystal_spire(b, &cyan, -4.6, 7.6, 18.0, 6.0, 0.8); // left jamb of the descent
    crystal_spire(b, &magenta, 4.6, 7.6, 18.0, 6.0, 0.8); // right jamb
    crystal_down(b, &teal, 0.0, 8.5, 27.0, 6.0, 1.1); // glowing keystone overhead
    crystal_spire(b, &cyan, -11.0, 9.0, 18.0, 5.0, 0.8);
    crystal_spire(b, &teal, 11.5, 11.0, 18.0, 4.0, 0.7);
    crystal_spire(b, &magenta, -8.0, 20.0, 18.0, 3.0, 0.6);
    crystal_spire(b, &cyan, 9.0, 8.0, 18.0, 3.6, 0.6);
    vein(b, &cyan, Vec3::new(-14.0, 21.0, 10.0), Vec3::new(-13.7, 25.5, 14.0));
    vein(b, &magenta, Vec3::new(13.7, 19.0, 14.0), Vec3::new(14.0, 24.0, 18.0));
    b.light(Vec3::new(0.0, 21.0, 9.0), rgb(0.45, 0.95, 1.0), 900_000.0, 34.0); // the gateway glow
    b.light(Vec3::new(-10.0, 22.0, 11.0), rgb(0.3, 0.9, 1.0), 650_000.0, 30.0);
    b.light(Vec3::new(8.0, 21.0, 18.0), rgb(1.0, 0.35, 0.9), 500_000.0, 26.0);

    // Starter loadout — the player may arrive lightly stocked from level 8.
    b.item(ItemKind::WeaponLightning, Vec3::new(0.0, 18.6, 12.0));
    b.item(ItemKind::Cells(30), Vec3::new(2.5, 18.6, 12.0));
    b.item(ItemKind::Shells(20), Vec3::new(-4.0, 18.6, 16.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(4.0, 18.6, 16.0));
    b.item(ItemKind::Health(25), Vec3::new(-7.0, 18.6, 9.0));
    // Garrison: hitscan grunts + an energy enforcer, classic early-Quake fodder.
    b.monster(Grunt, Vec3::new(-7.0, 19.0, 11.0));
    b.monster(Grunt, Vec3::new(7.0, 19.0, 9.0));
    b.monster(Enforcer, Vec3::new(0.0, 19.0, 8.0));

    // ======================================================================
    // DESCENT STAIR 1  x[-3,3] z[-2,6]  — drops y18 → y14 into the first gallery.
    //   Steps of 0.5m (== STEP_HEIGHT) so a walking player descends cleanly.
    // ======================================================================
    b.floor(-3.0, 3.0, 2.0, 6.0, 18.0, floor.clone()); // top landing (entry level)
    b.stairs(-3.0, 3.0, -2.0, 18.0, 14.0, 8, Vec3::Z); // flight, high at the south
    b.wall_z(-2.0, 6.0, -3.0, 14.0, 27.0, wall.clone(), &[]);
    b.wall_z(-2.0, 6.0, 3.0, 14.0, 27.0, wall.clone(), &[]);
    b.ceiling(-3.0, 3.0, -2.0, 6.0, 23.0, ceil.clone());
    b.light(Vec3::new(0.0, 20.0, 8.0), rgb(0.4, 0.9, 1.0), 360_000.0, 22.0);

    // ======================================================================
    // CRYSTAL GALLERY 1  x[-13,13] z[-22,-2]  floor y=14  h=9.
    //   A worked-out drift studded with geodes. Open south (stair 1), north
    //   (stair 2) and a cracked WEST plug (the resonant Lightning secret).
    // ======================================================================
    b.room(-13.0, 13.0, -22.0, -2.0, 14.0, 9.0, &[Wall::S((-3.0, 3.0)), Wall::N((-3.0, 3.0)), Wall::W((-14.0, -10.0))]);
    crystal_spire(b, &cyan, -10.0, -6.0, 14.0, 5.0, 0.7);
    crystal_spire(b, &magenta, 10.0, -16.0, 14.0, 4.0, 0.6);
    crystal_spire(b, &teal, 6.0, -19.0, 14.0, 3.0, 0.5);
    crystal_spire(b, &cyan, -8.0, -18.0, 14.0, 3.6, 0.55);
    vein(b, &magenta, Vec3::new(12.7, 15.0, -8.0), Vec3::new(13.0, 20.0, -4.0));
    vein(b, &cyan, Vec3::new(-3.0, 22.5, -14.0), Vec3::new(3.0, 23.0, -10.0)); // ceiling seam
    b.light(Vec3::new(-9.0, 20.0, -8.0), rgb(0.3, 0.9, 1.0), 460_000.0, 28.0);
    b.light(Vec3::new(8.0, 18.0, -16.0), rgb(1.0, 0.35, 0.9), 360_000.0, 24.0);
    b.item(ItemKind::Nails(30), Vec3::new(-9.0, 14.6, -16.0));
    b.item(ItemKind::Health(25), Vec3::new(9.0, 14.6, -6.0));
    b.monster(Knight, Vec3::new(-6.0, 15.0, -10.0));
    b.monster(Enforcer, Vec3::new(7.0, 15.0, -18.0));
    b.monster(Ogre, Vec3::new(0.0, 15.0, -19.0));

    // --- Resonant crystal plug (west wall, z[-14,-10]) ---------------------
    // A cracked crystal plug filling the gallery's west gap. Sweep the Lightning
    // beam up to its shatter note and it detonates, opening a niche of scarce
    // Cells + armor. Reachable on foot before you ever reach the cart.
    b.floor(-17.0, -13.0, -14.0, -10.0, 14.0, floor.clone()); // niche floor behind the plug
    b.wall_z(-14.0, -10.0, -17.0, 14.0, 19.0, wall.clone(), &[]); // niche back wall
    b.wall_x(-17.0, -13.0, -14.0, 14.0, 19.0, wall.clone(), &[]); // niche sides
    b.wall_x(-17.0, -13.0, -10.0, 14.0, 19.0, wall.clone(), &[]);
    b.ceiling(-17.0, -13.0, -14.0, -10.0, 19.0, ceil.clone());
    let plug = b.resonant(Vec3::new(-13.3, 14.0, -14.0), Vec3::new(-12.7, 18.0, -10.0), trim.clone());
    // A glowing fracture telegraph pinned to the plug so it hides when it shatters.
    b.deco_child(
        plug,
        Vec3::new(-13.0, 16.0, -12.0),
        Vec3::new(-13.05, 14.6, -13.4),
        Vec3::new(-12.95, 17.4, -10.6),
        cyan.clone(),
    );
    b.item(ItemKind::Cells(30), Vec3::new(-15.0, 14.6, -12.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(-15.0, 14.6, -11.0));
    b.light(Vec3::new(-13.0, 16.0, -12.0), rgb(0.3, 0.9, 1.0), 220_000.0, 16.0);

    // ======================================================================
    // DESCENT STAIR 2  x[-3,3]  — drops y14 → y10 onto the staging ledge.
    // ======================================================================
    b.stairs(-3.0, 3.0, -26.0, 14.0, 10.0, 8, Vec3::Z); // high at the gallery (north z), low toward the ledge
    b.wall_z(-26.0, -22.0, -3.0, 10.0, 23.0, wall.clone(), &[]);
    b.wall_z(-26.0, -22.0, 3.0, 10.0, 23.0, wall.clone(), &[]);
    b.ceiling(-3.0, 3.0, -26.0, -22.0, 19.0, ceil.clone());
    b.light(Vec3::new(0.0, 14.0, -24.0), rgb(0.4, 0.9, 1.0), 320_000.0, 20.0);

    // ======================================================================
    // THE GREAT RIFT CAVERN  x[-26,26] z[-138,-26]  y[-9,26]  — the cart ride.
    //   One enormous cavern the rail spans end to end, split lengthwise by a
    //   radiant ENERGY RIFT. Only three surfaces have footing: the south STAGING
    //   LEDGE (y=10), the eastern CRYSTAL-NEST GALLERY (y=5, branch only) and the
    //   north VAULT FLOOR (y=0). Everything between is the glowing rift — a fall
    //   either way is fatal.
    // ======================================================================
    // Shell: tall flank walls, a high ceiling, a south wall with the stair gap, a
    // north wall with the locked-door gap.
    b.wall_z(-138.0, -26.0, 26.0, -9.0, 26.0, wall.clone(), &[]); // east flank
    b.wall_z(-138.0, -26.0, -26.0, -9.0, 26.0, wall.clone(), &[]); // west flank
    b.wall_x(-26.0, 26.0, -138.0, -9.0, 26.0, wall.clone(), &[(-4.0, 4.0)]); // north (locked door)
    b.wall_x(-26.0, 26.0, -26.0, -9.0, 26.0, wall.clone(), &[(-4.0, 4.0)]); // south (stair 2)
    b.ceiling(-26.0, 26.0, -138.0, -26.0, 26.0, ceil.clone());

    // --- The three footings -------------------------------------------------
    // STAGING LEDGE (y=10): the deck the cart is parked on. Its north edge (z=-46)
    // is a sheer cliff into the rift — you must ride to get down.
    b.floor(-16.0, 16.0, -46.0, -26.0, 10.0, floor.clone());
    // VAULT FLOOR (y=0): the key chamber the whole rail comes to rest on.
    b.floor(-16.0, 16.0, -138.0, -120.0, 0.0, floor.clone());
    // CRYSTAL-NEST GALLERY (y=5): the eastern branch shelf, a nest the high branch
    // line runs the deck straight through (the main line never touches it).
    b.floor(8.0, 24.0, -86.0, -62.0, 5.0, floor.clone());
    // A small western firing shelf (y=6) — an Enforcer perch that rakes the ride.
    b.floor(-26.0, -20.0, -84.0, -74.0, 6.0, floor.clone());

    // --- The radiant rift filling the open middle ---------------------------
    // RIFT, z[-120,-46]: a blinding magenta energy chasm. A glow surface deep down,
    // its own damage volume, and the kill plane below it — a fall in is fatal three
    // ways over. Hand-authored (no catch floor) so a faller drops clean to the kill
    // plane rather than being caught half-in.
    b.void_kill(VOID_KILL_Y);
    b.deco(Vec3::new(-26.0, -6.5, -120.0), Vec3::new(26.0, -6.0, -46.0), rift.clone());
    b.lava.volumes.push(Aabb::from_corners(
        Vec3::new(-26.0, -7.5, -120.0),
        Vec3::new(26.0, -4.0, -46.0),
    ));
    // Radiant light boiling up out of the rift (the cavern's main illumination).
    b.light(Vec3::new(0.0, -3.0, -60.0), rgb(1.0, 0.3, 0.85), 1_500_000.0, 60.0);
    b.light(Vec3::new(0.0, -3.0, -84.0), rgb(1.0, 0.3, 0.85), 1_500_000.0, 60.0);
    b.light(Vec3::new(0.0, -3.0, -106.0), rgb(1.0, 0.35, 0.9), 1_300_000.0, 56.0);

    // --- Crystal spires the rail arcs between, rising from the rift ---------
    // Deco only (no collider) so they never block the cart or a rider — they just
    // frame the ride as a glittering gauntlet between towers of glass.
    crystal_spire(b, &cyan, -12.0, -56.0, -6.0, 16.0, 1.4);
    crystal_spire(b, &magenta, 12.0, -68.0, -6.0, 20.0, 1.6);
    crystal_spire(b, &teal, -14.0, -82.0, -6.0, 18.0, 1.3);
    crystal_spire(b, &cyan, 13.0, -96.0, -6.0, 22.0, 1.7);
    crystal_spire(b, &magenta, -11.0, -108.0, -6.0, 15.0, 1.2);
    // Hanging crystal stalactites from the ceiling, growing down into the cavern.
    crystal_down(b, &cyan, -8.0, -64.0, 26.0, 7.0, 1.0);
    crystal_down(b, &magenta, 9.0, -90.0, 26.0, 8.0, 1.1);
    crystal_down(b, &teal, -6.0, -104.0, 26.0, 6.0, 0.9);
    // Veins raking the flank walls of the descent.
    vein(b, &cyan, Vec3::new(-26.0, 2.0, -58.0), Vec3::new(-25.7, 9.0, -64.0));
    vein(b, &magenta, Vec3::new(25.7, 0.0, -92.0), Vec3::new(26.0, 8.0, -98.0));
    vein(b, &cyan, Vec3::new(-26.0, -2.0, -104.0), Vec3::new(-25.7, 4.0, -110.0));

    // ----------------------------------------------------------------------
    // THE RAIL. Main-line cart ground-points descend the cavern from the staging
    // ledge, out over the radiant rift between the spires, to the vault floor.
    // Node 2 is the JUNCTION (switch); node 5 the weak JOINT (derail = into the
    // rift). The eastern branch forks at node 2, runs the crystal nest and rejoins
    // the main line on the vault floor.
    // ----------------------------------------------------------------------
    let main = [
        Vec3::new(0.0, 10.06, -40.0), // 0: parked on the staging ledge
        Vec3::new(0.0, 9.2, -47.0),   // 1: rolls off the cliff edge into the cavern
        Vec3::new(0.0, 7.4, -58.0),   // 2: JUNCTION node (switch sits beside it)
        Vec3::new(0.0, 5.6, -74.0),   // 3: arcing between spires over the rift
        Vec3::new(0.0, 4.0, -92.0),   // 4: deep out over the radiant rift
        Vec3::new(0.0, 2.6, -108.0),  // 5: weak JOINT node (derail = plunge into the rift)
        Vec3::new(0.0, 1.2, -120.0),  // 6: nearing the vault lip
        Vec3::new(0.0, 0.06, -132.0), // 7: run-out, rests on the vault floor by the door
    ];
    // Eastern branch tail (MUST start at the junction node): climbs onto the
    // crystal-nest gallery, runs its length past the Ogres, then drops the long
    // way back across the rift to rejoin the main line on the vault floor.
    let branch = [
        Vec3::new(0.0, 7.4, -58.0),   // == main[2]
        Vec3::new(10.0, 6.4, -64.0),  // swings east, descending toward the gallery
        Vec3::new(16.0, 5.06, -72.0), // onto the crystal-nest gallery deck (y=5)
        Vec3::new(16.0, 5.06, -82.0), // runs the gallery past the nest
        Vec3::new(10.0, 2.6, -108.0), // drops off the gallery's north lip, out over the rift
        Vec3::new(0.0, 0.06, -132.0), // rejoin == main[7]
    ];
    b.rail(&main, Some((2, &branch)), Some(5));

    // --- Things that fight you from the deck --------------------------------
    // Scrags haunt the open rift cavern so the ride is fought, not coasted.
    b.monster(Scrag, Vec3::new(8.0, 8.0, -54.0));
    b.monster(Scrag, Vec3::new(-9.0, 6.0, -72.0));
    b.monster(Scrag, Vec3::new(7.0, 5.0, -96.0));
    b.monster(Scrag, Vec3::new(-7.0, 4.0, -110.0));
    // Enforcers raking from the footings — the west firing shelf and the vault lip.
    b.monster(Enforcer, Vec3::new(-23.0, 7.0, -79.0));
    // (No stash on this shelf: it floats against the west wall, ringed by the lethal
    // rift with no rail or footing reaching it, so any reward here would be bait the
    // player has no normal path to collect — the perch earns its place as a threat.)
    // The crystal NEST on the eastern gallery (only the BRANCH line runs it).
    b.monster(Ogre, Vec3::new(15.0, 6.0, -70.0));
    b.monster(Ogre, Vec3::new(18.0, 6.0, -80.0));
    b.monster(Scrag, Vec3::new(12.0, 7.0, -76.0));
    b.monster(Enforcer, Vec3::new(20.0, 6.0, -74.0));
    crystal_spire(b, &magenta, 22.0, -66.0, 5.0, 5.0, 0.8);
    crystal_spire(b, &cyan, 11.0, -84.0, 5.0, 4.5, 0.7);
    vein(b, &cyan, Vec3::new(25.7, 6.0, -70.0), Vec3::new(26.0, 11.0, -76.0));
    b.light(Vec3::new(16.0, 9.0, -74.0), rgb(0.3, 0.9, 1.0), 600_000.0, 30.0);
    // The branch's reward stash, earned by throwing the switch and surviving the nest.
    b.item(ItemKind::ArmorYellow, Vec3::new(16.0, 5.6, -74.0));
    b.item(ItemKind::Rockets(10), Vec3::new(20.0, 5.6, -78.0));
    b.item(ItemKind::Health(25), Vec3::new(12.0, 5.6, -80.0));

    // ======================================================================
    // VAULT FLOOR (y=0, z[-138,-120]) — the key chamber the whole rail rests on.
    //   A Death Knight holds the Silver Key; a Knight pack camps the floor. The
    //   door in the north wall is locked until the key is grabbed.
    // ======================================================================
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 1.0, -134.0));
    b.item(ItemKind::Health(25), Vec3::new(-9.0, 0.6, -124.0));
    b.item(ItemKind::Nails(40), Vec3::new(9.0, 0.6, -124.0));
    crystal_spire(b, &cyan, -13.0, -132.0, 0.0, 6.0, 0.9);
    crystal_spire(b, &magenta, 13.0, -132.0, 0.0, 6.0, 0.9);
    crystal_spire(b, &teal, -10.0, -124.0, 0.0, 3.0, 0.5);
    vein(b, &cyan, Vec3::new(-16.0, 1.0, -130.0), Vec3::new(-15.7, 7.0, -135.0));
    vein(b, &magenta, Vec3::new(15.7, 1.0, -130.0), Vec3::new(16.0, 7.0, -135.0));
    b.light(Vec3::new(0.0, 6.0, -132.0), rgb(0.6, 1.0, 0.95), 800_000.0, 40.0);
    b.light(Vec3::new(0.0, 3.0, -124.0), rgb(0.3, 0.9, 1.0), 360_000.0, 24.0);
    // The Knight pack guarding the floor + the key's Death-Knight guardian.
    b.monster(Knight, Vec3::new(-5.0, 1.0, -126.0));
    b.monster(Knight, Vec3::new(5.0, 1.0, -126.0));
    b.monster(Grunt, Vec3::new(0.0, 1.0, -124.0));
    b.monster(DeathKnight, Vec3::new(0.0, 1.0, -136.0));
    // Key-grab ambush (classic Quake trap): an Ogre crashes in behind you.
    b.ambush(Ogre, Vec3::new(-9.0, 1.0, -134.0));

    // Locked crystal door filling the north-wall gap (slides up on key + near).
    // Placed LAST among the structural brushes so the cart's, switch's, joint's and
    // resonant plug's reserved collider slots all stay valid.
    b.door(Vec3::new(-4.0, 0.0, -138.3), Vec3::new(4.0, 6.5, -137.7), Vec3::new(0.0, 7.0, 0.0));

    // ======================================================================
    // EXIT CHAMBER  x[-12,12] z[-150,-138]  h=7  — beyond the door.
    // ======================================================================
    b.room(-12.0, 12.0, -150.0, -138.0, 0.0, 7.0, &[Wall::S((-3.0, 3.0))]);
    b.slipgate(Vec3::new(-3.0, 0.1, -149.6), Vec3::new(3.0, 5.0, -149.4));
    b.exit(Vec3::new(0.0, 1.5, -146.0));
    b.monster(Enforcer, Vec3::new(0.0, 1.0, -142.0));
    b.item(ItemKind::Health(25), Vec3::new(-7.0, 0.6, -140.0));
    b.item(ItemKind::Shells(20), Vec3::new(7.0, 0.6, -140.0));
    crystal_spire(b, &cyan, -10.0, -148.0, 0.0, 4.0, 0.6);
    crystal_spire(b, &magenta, 10.0, -148.0, 0.0, 4.0, 0.6);
    b.light(Vec3::new(0.0, 5.0, -144.0), rgb(0.4, 0.95, 1.0), 600_000.0, 34.0);

    // A dim, cool sun so the near-black rock keeps a faint silhouette top-to-bottom.
    b.sun(Vec3::new(14.0, 36.0, -10.0), Vec3::new(0.0, 0.0, -84.0), rgb(0.34, 0.40, 0.46), 700.0);
}

/// A faceted crystal spire (deco only): a tapering 3-tier column of the given
/// material rising from `(cx, cz, base_y)` to `base_y + height`, the upper tiers
/// stepping inward so it reads as a hand-cut crystal point, not a post. Visual-only,
/// so it never blocks the cart or a rider.
fn crystal_spire(b: &mut Build, mat: &Handle<StandardMaterial>, cx: f32, cz: f32, base_y: f32, height: f32, half: f32) {
    const TIERS: usize = 3;
    for i in 0..TIERS {
        let f0 = i as f32 / TIERS as f32;
        let f1 = (i as f32 + 1.0) / TIERS as f32;
        let hw = half * (1.0 - f0 * 0.6); // taper toward the tip
        let y0 = base_y + height * f0;
        let y1 = base_y + height * f1;
        b.deco(Vec3::new(cx - hw, y0, cz - hw), Vec3::new(cx + hw, y1, cz + hw), mat.clone());
    }
}

/// The same tapering crystal as [`crystal_spire`] but hung point-DOWN from a
/// ceiling at `top_y`, growing `height` down into the cavern (a glowing stalactite).
fn crystal_down(b: &mut Build, mat: &Handle<StandardMaterial>, cx: f32, cz: f32, top_y: f32, height: f32, half: f32) {
    const TIERS: usize = 3;
    for i in 0..TIERS {
        let f0 = i as f32 / TIERS as f32;
        let f1 = (i as f32 + 1.0) / TIERS as f32;
        let hw = half * (1.0 - f0 * 0.6); // taper toward the down-pointing tip
        let y1 = top_y - height * f0;
        let y0 = top_y - height * f1;
        b.deco(Vec3::new(cx - hw, y0, cz - hw), Vec3::new(cx + hw, y1, cz + hw), mat.clone());
    }
}

/// A glowing crystal vein streak set into the rock (emissive deco). Just the box
/// [min,max] in the wanted material — pulled out so the level reads as a list of
/// veins rather than a wall of `deco` calls.
fn vein(b: &mut Build, mat: &Handle<StandardMaterial>, min: Vec3, max: Vec3) {
    b.deco(min, max, mat.clone());
}
