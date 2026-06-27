//! Campaign level 3 — **Blackvein Deep** (`ThemeId::Mine`).
//!
//! A long, pitch-dark abandoned ore mine that descends in three stages: an entry
//! cavern you clear on foot, a stepped descent into the workings, and then the
//! headline — the **mine-cart line** (feature 45), a single rail spine that drops
//! the length of a vast black shaft cavern, **over a bottomless mineshaft** and
//! **across a flooded sump**, all the way to the vault floor. You ride the cart
//! down fighting from the deck and *shoot the world to steer*:
//!   - a **junction switch** part-way down throws the cart onto a high eastern
//!     branch that runs the deck straight through an **Ogre ore-nest** (with the
//!     branch's reward stash) instead of the low main line, and
//!   - a **weak rail joint** above the vault can be shot to **derail** on purpose,
//!     tumbling the cart (rider aboard) as a wrecking ball into the monster pack
//!     guarding the floor.
//!
//! The Silver Key sits on the vault floor where every line comes to rest; a
//! Death Knight holds it, a locked flood-gate gates the exit slipgate beyond. The
//! whole shaft is lit only by ore-vein glints, hung lanterns and a dim raking sun
//! — black-with-glints, not flat black. Falling off the cart kills either way:
//! the dry shaft drops you past the kill plane, the sump drowns you in black water.

use crate::level::{Build, ItemKind, MonsterKind::*, Wall};
use crate::common::rgb;
use bevy::prelude::*;

/// The "fell out of the world" kill plane: anything whose center drops below this
/// dies instantly. Set well below the vault floor (y=0) and the sump surface so a
/// fall off the cart — down the dry shaft OR through the black sump — is fatal.
const VOID_KILL_Y: f32 = -6.0;

pub fn build(b: &mut Build) {
    // Player spawns on the high entry-cavern floor, facing north (-Z) into the mine.
    b.start.pos = Vec3::new(0.0, 13.0, 12.0);
    b.start.yaw = 0.0;

    // Theme handles + a few custom mine surfaces.
    let floor = b.theme.floor.clone();
    let wall = b.theme.wall.clone();
    let ceil = b.theme.ceiling.clone();
    let trim = b.theme.trim.clone();
    // Rough-hewn timber pit-props (deco only — they never block the cart or you).
    let timber = b.mat(rgb(0.16, 0.11, 0.06), LinearRgba::BLACK, 0.95, 0.0);
    // Amber ore-vein glow set into the black rock (emissive accent deco).
    let ore = b.mat(rgb(1.0, 0.6, 0.22), LinearRgba::rgb(3.0, 1.2, 0.25), 0.5, 0.2);
    // The faint hot glow far down the bottomless shaft (seen, never reached alive).
    let deep = b.mat(rgb(0.5, 0.18, 0.05), LinearRgba::rgb(1.1, 0.32, 0.06), 0.85, 0.0);
    // Black sump water: near-black with a cold sheen so the flooded channel reads.
    let water = b.mat(rgb(0.015, 0.025, 0.04), LinearRgba::rgb(0.0, 0.02, 0.05), 0.1, 0.0);

    // ======================================================================
    // ENTRY CAVERN  x[-12,12] z[2,18]  floor y=12  h=8  — on-foot warm-up.
    //   The mouth of the mine. Clear the guards here, grab the Nailgun + ammo,
    //   then drop north through the stepped descent to the cart.
    // ======================================================================
    b.room(-12.0, 12.0, 2.0, 18.0, 12.0, 8.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 12.1, 17.2), Vec3::new(2.0, 16.0, 17.6)); // arrival slip
    // Pit-props framing the cavern + a couple of glinting ore veins in the rock.
    timber_frame(b, &timber, -9.0, 6.0, 12.0, 19.0, 0.4);
    timber_frame(b, &timber, 9.0, 14.0, 12.0, 19.0, 0.4);
    ore_seam(b, &ore, Vec3::new(-12.0, 15.0, 8.0), Vec3::new(-11.7, 17.5, 11.0));
    ore_seam(b, &ore, Vec3::new(11.7, 14.0, 12.0), Vec3::new(12.0, 16.0, 15.0));

    // Starter loadout — the player may arrive with little more than a shotgun.
    b.item(ItemKind::WeaponNailgun, Vec3::new(0.0, 12.6, 6.0));
    b.item(ItemKind::Nails(30), Vec3::new(2.0, 12.6, 6.0));
    b.item(ItemKind::Shells(20), Vec3::new(-4.0, 12.6, 14.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(4.0, 12.6, 14.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 12.6, 5.0));
    // Garrison: hitscan grunts + an energy enforcer, classic early Quake fodder.
    b.monster(Grunt, Vec3::new(-6.0, 13.0, 8.0));
    b.monster(Grunt, Vec3::new(6.0, 13.0, 6.0));
    b.monster(Enforcer, Vec3::new(0.0, 13.0, 3.0));

    // ======================================================================
    // DESCENT STAIRWAY  x[-3,3] z[-6,2]  — drops y12 → y8 into the workings.
    //   Steps of 0.5m (== STEP_HEIGHT) so a walking player descends cleanly.
    // ======================================================================
    b.floor(-3.0, 3.0, -2.0, 2.0, 12.0, floor.clone()); // top landing (entry level)
    b.stairs(-3.0, 3.0, -6.0, 12.0, 8.0, 8, Vec3::Z * 1.0); // flight, high at the south
    b.wall_z(-6.0, 2.0, -3.0, 8.0, 16.0, wall.clone(), &[]);
    b.wall_z(-6.0, 2.0, 3.0, 8.0, 16.0, wall.clone(), &[]);
    b.ceiling(-3.0, 3.0, -6.0, 2.0, 16.0, ceil.clone());
    timber_frame(b, &timber, 0.0, -4.0, 8.0, 15.5, 1.4);
    b.light(Vec3::new(0.0, 14.0, 8.0), rgb(1.0, 0.62, 0.28), 500_000.0, 30.0); // entry lantern
    b.light(Vec3::new(0.0, 13.0, -2.0), rgb(1.0, 0.55, 0.22), 320_000.0, 22.0); // stair lantern
    b.monster(Grunt, Vec3::new(0.0, 9.0, -4.0)); // a guard posted on the stair

    // ======================================================================
    // THE GREAT SHAFT CAVERN  x[-18,18] z[-86,-6]  y[-4,20]  — the cart ride.
    //   One enormous black void the rail spans end to end. Only three surfaces
    //   have footing: the south STAGING LEDGE (y=8), the eastern ORE-NEST GALLERY
    //   (y=4, branch only) and the north VAULT FLOOR (y=0). Everything between is
    //   the drop: a dry bottomless shaft to the south, a flooded sump to the north.
    // ======================================================================
    // Shell: tall side + end walls and a high ceiling. The west wall carries a gap
    // for the resonant ore-seam secret; the north wall a gap for the flood-gate;
    // the south wall a gap where the descent stair feeds onto the staging ledge.
    b.wall_z(-86.0, -6.0, 18.0, -4.0, 20.0, wall.clone(), &[]); // east flank
    b.wall_z(-86.0, -6.0, -18.0, -4.0, 20.0, wall.clone(), &[(-14.0, -10.0)]); // west (secret gap)
    b.wall_x(-18.0, 18.0, -86.0, -4.0, 20.0, wall.clone(), &[(-3.0, 3.0)]); // north (flood-gate)
    b.wall_x(-18.0, 18.0, -6.0, -4.0, 20.0, wall.clone(), &[(-3.0, 3.0)]); // south (descent)
    b.ceiling(-18.0, 18.0, -86.0, -6.0, 20.0, ceil.clone());

    // --- The three footings -------------------------------------------------
    // STAGING LEDGE (y=8): full-width deck the cart is parked on. Its north edge
    // (z=-22) is a sheer cliff into the shaft — you must ride to get down.
    b.floor(-18.0, 18.0, -22.0, -6.0, 8.0, floor.clone());
    // VAULT FLOOR (y=0): full-width key chamber the whole rail comes to rest on.
    b.floor(-18.0, 18.0, -86.0, -70.0, 0.0, floor.clone());
    // ORE-NEST GALLERY (y=4): the eastern branch ledge, an Ogre nest the high
    // branch line runs the deck straight through (the main line never touches it).
    b.floor(4.0, 18.0, -62.0, -48.0, 4.0, floor.clone());

    // --- The two hazards filling the open middle ----------------------------
    // BOTTOMLESS SHAFT (dry), z[-56,-22]: nothing but a faint hot glow far below.
    b.void_kill(VOID_KILL_Y);
    b.deco(Vec3::new(-18.0, -29.0, -56.0), Vec3::new(18.0, -28.0, -22.0), deep.clone());
    b.light(Vec3::new(0.0, -24.0, -40.0), rgb(1.0, 0.4, 0.12), 1_400_000.0, 70.0);
    // FLOODED SUMP (black water), z[-70,-56]: a dark surface ~4m under the rail
    // with its own damage volume; sink past it and the kill plane drowns you.
    // Authored by hand (no catch floor) so a faller drops clean through to the
    // kill plane rather than being caught half-submerged.
    b.deco(Vec3::new(-18.0, -1.7, -70.0), Vec3::new(18.0, -1.5, -56.0), water.clone());
    b.lava.volumes.push(crate::physics::Aabb::from_corners(
        Vec3::new(-18.0, -3.0, -70.0),
        Vec3::new(18.0, -1.0, -56.0),
    ));

    // ----------------------------------------------------------------------
    // THE RAIL. Main-line cart ground-points descend the shaft from the staging
    // ledge to the vault floor. Node 2 is the JUNCTION (switch), node 5 the weak
    // JOINT (derail). The eastern branch forks at node 2, runs the ore-nest
    // gallery and rejoins the main line on the vault floor.
    // ----------------------------------------------------------------------
    let main = [
        Vec3::new(0.0, 8.08, -20.0), // 0: parked on the staging ledge
        Vec3::new(0.0, 7.0, -26.0),  // 1: rolls off the cliff edge
        Vec3::new(0.0, 5.6, -36.0),  // 2: JUNCTION node (switch sits beside it)
        Vec3::new(0.0, 4.0, -48.0),  // 3: out over the bottomless shaft
        Vec3::new(0.0, 2.4, -60.0),  // 4: out over the flooded sump
        Vec3::new(0.0, 1.0, -70.0),  // 5: weak JOINT node (over the vault approach)
        Vec3::new(0.0, 0.08, -80.0), // 6: rests on the vault floor by the door
    ];
    // Eastern branch tail (MUST start at the junction node); climbs onto the
    // ore-nest gallery, runs its length, then drops back to rejoin at the end.
    let branch = [
        Vec3::new(0.0, 5.6, -36.0),  // == main[2]
        Vec3::new(10.0, 4.6, -44.0), // swings east, climbing toward the gallery
        Vec3::new(12.0, 4.2, -50.0), // onto the ore-nest gallery deck (y=4)
        Vec3::new(12.0, 4.2, -60.0), // runs the gallery past the Ogres
        Vec3::new(6.0, 1.6, -70.0),  // drops off the gallery's north lip
        Vec3::new(0.0, 0.08, -80.0), // rejoin == main[6]
    ];
    b.rail(&main, Some((2, &branch)), Some(5));

    // --- Pooled lanterns + ore veins raking the descent (read the markers) ---
    b.light(Vec3::new(-3.0, 7.0, -24.0), rgb(1.0, 0.6, 0.25), 420_000.0, 26.0); // off the ledge
    b.light(Vec3::new(2.0, 5.5, -38.0), rgb(1.0, 0.7, 0.3), 380_000.0, 24.0); // by the junction
    b.light(Vec3::new(-2.0, 3.0, -52.0), rgb(1.0, 0.5, 0.2), 380_000.0, 26.0); // over the shaft
    b.light(Vec3::new(2.0, 2.0, -64.0), rgb(0.9, 0.55, 0.45), 360_000.0, 26.0); // over the sump
    ore_seam(b, &ore, Vec3::new(-18.0, 9.0, -30.0), Vec3::new(-17.7, 13.0, -34.0));
    ore_seam(b, &ore, Vec3::new(17.7, 6.0, -42.0), Vec3::new(18.0, 11.0, -46.0));
    ore_seam(b, &ore, Vec3::new(-18.0, 2.0, -58.0), Vec3::new(-17.7, 7.0, -62.0));
    // Pit-props bracing the shaft walls (deco).
    timber_frame(b, &timber, 0.0, -30.0, 8.0, 19.5, 17.5);
    timber_frame(b, &timber, 0.0, -64.0, 0.0, 19.5, 17.5);

    // --- Things that fight you from the deck --------------------------------
    // Scrags haunt the open shaft so the ride is fought, not coasted.
    b.monster(Scrag, Vec3::new(8.0, 9.0, -34.0));
    b.monster(Scrag, Vec3::new(-8.0, 6.0, -50.0));
    b.monster(Scrag, Vec3::new(7.0, 4.0, -64.0));
    // The Ogre ore-nest on the eastern gallery (only the BRANCH line runs it).
    b.monster(Ogre, Vec3::new(12.0, 5.0, -52.0));
    b.monster(Ogre, Vec3::new(15.0, 5.0, -58.0));
    b.monster(Enforcer, Vec3::new(8.0, 5.0, -50.0));
    // The branch's reward stash, earned by throwing the switch and surviving it.
    b.item(ItemKind::ArmorYellow, Vec3::new(13.0, 4.6, -55.0));
    b.item(ItemKind::Rockets(10), Vec3::new(15.0, 4.6, -52.0));
    b.item(ItemKind::Health(25), Vec3::new(10.0, 4.6, -58.0));
    ore_seam(b, &ore, Vec3::new(17.7, 5.0, -54.0), Vec3::new(18.0, 9.0, -58.0));
    b.light(Vec3::new(13.0, 7.0, -55.0), rgb(1.0, 0.55, 0.2), 450_000.0, 26.0);

    // ----------------------------------------------------------------------
    // RESONANT ORE-SEAM SECRET (west wall, z[-14,-10]). A cracked rock plug in
    // the staging-ledge wall; sweep the Lightning beam up to its shatter note and
    // it detonates, opening a niche of scarce Cells. Reachable on foot before you
    // ever board the cart.
    // ----------------------------------------------------------------------
    b.floor(-21.0, -18.0, -14.0, -10.0, 8.0, floor.clone()); // niche floor behind the plug
    b.wall_z(-14.0, -10.0, -21.0, 8.0, 12.0, wall.clone(), &[]); // niche back wall
    b.wall_x(-21.0, -18.0, -14.0, 8.0, 12.0, wall.clone(), &[]); // niche sides
    b.wall_x(-21.0, -18.0, -10.0, 8.0, 12.0, wall.clone(), &[]);
    let seam = b.resonant(Vec3::new(-18.3, 8.0, -14.0), Vec3::new(-17.7, 11.5, -10.0), trim.clone());
    // A glowing fracture telegraph pinned to the plug so it hides when it shatters.
    b.deco_child(
        seam,
        Vec3::new(-18.0, 9.75, -12.0),
        Vec3::new(-17.65, 8.6, -13.4),
        Vec3::new(-17.6, 10.9, -10.6),
        ore.clone(),
    );
    b.item(ItemKind::Cells(30), Vec3::new(-19.5, 8.6, -12.0));
    b.item(ItemKind::Health(25), Vec3::new(-19.5, 8.6, -11.0));
    b.light(Vec3::new(-16.0, 10.0, -12.0), rgb(1.0, 0.55, 0.2), 200_000.0, 16.0);

    // ======================================================================
    // VAULT FLOOR (y=0, z[-86,-70]) — the key chamber the whole rail rests on.
    //   A Death Knight holds the Silver Key; a monster pack camps the floor (the
    //   derail's wrecking-ball target). The flood-gate in the north wall is locked.
    // ======================================================================
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 1.0, -82.0));
    b.item(ItemKind::Health(25), Vec3::new(-8.0, 0.6, -74.0));
    b.item(ItemKind::Nails(40), Vec3::new(8.0, 0.6, -74.0));
    b.light(Vec3::new(0.0, 6.0, -80.0), rgb(1.0, 0.78, 0.55), 800_000.0, 40.0);
    b.light(Vec3::new(0.0, 3.0, -72.0), rgb(1.0, 0.5, 0.2), 360_000.0, 24.0);
    ore_seam(b, &ore, Vec3::new(-18.0, 3.0, -78.0), Vec3::new(-17.7, 8.0, -82.0));
    ore_seam(b, &ore, Vec3::new(17.7, 3.0, -78.0), Vec3::new(18.0, 8.0, -82.0));
    timber_frame(b, &timber, 0.0, -84.0, 0.0, 19.5, 17.5);
    // The monster pack guarding the floor — derail the cart into it as a battering
    // ram, or fight it the hard way after you step off.
    b.monster(Knight, Vec3::new(-5.0, 1.0, -76.0));
    b.monster(Knight, Vec3::new(5.0, 1.0, -76.0));
    b.monster(Grunt, Vec3::new(0.0, 1.0, -74.0));
    // The key's guardian, dead north by the gate.
    b.monster(DeathKnight, Vec3::new(0.0, 1.0, -84.0));
    // Key-grab ambush (classic Quake trap): an Ogre crashes in behind you.
    b.ambush(Ogre, Vec3::new(-9.0, 1.0, -82.0));

    // Locked flood-gate door filling the north-wall gap (slides up on key + near).
    // Placed LAST among the structural brushes so the cart's and markers' reserved
    // collider slots stay valid.
    b.door(Vec3::new(-3.0, 0.0, -86.3), Vec3::new(3.0, 6.0, -85.7), Vec3::new(0.0, 6.5, 0.0));

    // ======================================================================
    // EXIT CHAMBER  x[-10,10] z[-98,-86]  h=7  — beyond the flood-gate.
    // ======================================================================
    b.room(-10.0, 10.0, -98.0, -86.0, 0.0, 7.0, &[Wall::S((-3.0, 3.0))]);
    b.slipgate(Vec3::new(-3.0, 0.1, -97.6), Vec3::new(3.0, 5.0, -97.4));
    b.exit(Vec3::new(0.0, 1.5, -94.0));
    b.monster(Enforcer, Vec3::new(0.0, 1.0, -90.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 0.6, -88.0));
    b.item(ItemKind::Shells(20), Vec3::new(6.0, 0.6, -88.0));
    ore_seam(b, &ore, Vec3::new(-10.0, 4.0, -96.0), Vec3::new(-9.7, 6.5, -93.0));
    b.light(Vec3::new(0.0, 5.0, -92.0), rgb(1.0, 0.6, 0.3), 600_000.0, 34.0);

    // A dim, raking sun so the black rock keeps a faint silhouette top-to-bottom.
    b.sun(Vec3::new(12.0, 30.0, -10.0), Vec3::new(0.0, 2.0, -50.0), rgb(0.42, 0.38, 0.42), 900.0);
}

/// A rough timber pit-prop frame (deco only): two posts of `half`-square section
/// straddling `cx`, rising from `floor_y` to `top_y`, capped by a cross-beam — the
/// classic mine roof support. Visual-only, so it never blocks the cart or a rider.
fn timber_frame(b: &mut Build, mat: &Handle<StandardMaterial>, cx: f32, z: f32, floor_y: f32, top_y: f32, span: f32) {
    let (zl, zh) = (z - 0.18, z + 0.18);
    // Left + right posts.
    b.deco(Vec3::new(cx - span - 0.18, floor_y, zl), Vec3::new(cx - span + 0.18, top_y, zh), mat.clone());
    b.deco(Vec3::new(cx + span - 0.18, floor_y, zl), Vec3::new(cx + span + 0.18, top_y, zh), mat.clone());
    // Cross-beam over the top.
    b.deco(Vec3::new(cx - span - 0.18, top_y - 0.36, zl), Vec3::new(cx + span + 0.18, top_y, zh), mat.clone());
}

/// A glinting amber ore-vein streak set into the rock (emissive deco). Just the box
/// [min,max] in the wanted material — pulled out so the level reads as a list of
/// veins rather than a wall of `deco` calls.
fn ore_seam(b: &mut Build, mat: &Handle<StandardMaterial>, min: Vec3, max: Vec3) {
    b.deco(min, max, mat.clone());
}
