//! Campaign level 6 — **The Brine Gallery** (`ThemeId::Brine`).
//!
//! A flooded, half-drowned salt mine that opens onto a vast sea-cavern. It runs
//! the same three-stage shape as Blackvein Deep (the mine-cart exemplar): an entry
//! gallery you clear on foot, a stepped descent into the flooded workings, and then
//! the headline — the **mine-cart line** (feature 45), a single rail spine that
//! skims the length of a huge black-water cavern, **a low, tense pass just over the
//! drowning pool**, all the way to the pump-house vault. You ride the cart fighting
//! from the deck and *shoot the world to steer*:
//!   - a **junction switch** part-way out throws the cart onto an eastern **salt
//!     side-gallery** that runs the deck straight through an Ogre nest (with the
//!     branch's reward stash) instead of the low main skim, and
//!   - a **weak rail joint** above the vault approach can be shot to **derail** on
//!     purpose — a gamble: tumble forward as a wrecking ball into the vault pack on
//!     solid ground, OR mistime it and plunge off the trestle into the black water.
//!
//! Everything between the few stone footings is **deep dark seawater**: stand or
//! fall in it and you drown (a damage volume at the surface, no catch floor, and a
//! kill plane in the deep — a faller sinks clean through and dies). The cart line
//! skims ~1m over that waterline the whole crossing, so a derail or a clipped corner
//! drops you straight in.
//!
//! The Silver Key sits in the flooded pump-house the rail comes to rest on; a Death
//! Knight holds it, a locked **sluice-gate** gates the exit slipgate beyond. The
//! cavern is lit only by pale hung lanterns and the cold glow of salt crystals — dim,
//! damp and grey-green, never flat black.

use crate::level::{Build, ItemKind, MonsterKind::*, Wall};
use crate::common::rgb;
use bevy::prelude::*;

/// The "drowned" kill plane: any player whose center sinks below this dies
/// instantly. Set in the deep, below the seawater surface (y=1.0) and every stone
/// footing, so a fall off the cart — or a derail over the pool — sinks clean
/// through the black water and drowns rather than catching half-submerged.
const VOID_KILL_Y: f32 = -5.0;

/// Seawater surface height across the open middle of the flooded cavern.
const WATER_Y: f32 = 1.0;

pub fn build(b: &mut Build) {
    // Player spawns on the high entry-gallery floor, facing north (-Z) into the mine.
    b.start.pos = Vec3::new(0.0, 11.0, 20.0);
    b.start.yaw = 0.0;

    // Theme handles + a few custom brine surfaces.
    let floor = b.theme.floor.clone();
    let wall = b.theme.wall.clone();
    let ceil = b.theme.ceiling.clone();
    let trim = b.theme.trim.clone();
    // Pale white-grey rock salt (deco pillars, vault dressing).
    let salt = b.mat(rgb(0.86, 0.89, 0.92), LinearRgba::rgb(0.04, 0.06, 0.07), 0.5, 0.0);
    // Cold luminous brine crystal — the faint blue-green glow that picks out the salt.
    let brine = b.mat(rgb(0.55, 0.85, 0.86), LinearRgba::rgb(0.3, 1.1, 1.1), 0.3, 0.0);
    // Black seawater: near-black with a cold sheen so the flooded pool reads deadly.
    let water = b.mat(rgb(0.012, 0.022, 0.03), LinearRgba::rgb(0.0, 0.015, 0.03), 0.1, 0.0);
    // The murk under the surface — almost lightless, just a hint of depth.
    let deep = b.mat(rgb(0.008, 0.015, 0.022), LinearRgba::BLACK, 0.2, 0.0);

    // ======================================================================
    // ENTRY GALLERY  x[-12,12] z[6,24]  floor y=10  h=8  — on-foot warm-up.
    //   The dry mouth of the workings. Clear the guards, grab the ammo, then
    //   drop north down the stepped descent to the cart.
    // ======================================================================
    b.room(-12.0, 12.0, 6.0, 24.0, 10.0, 8.0, &[Wall::N((-4.0, 4.0))]);
    b.slipgate(Vec3::new(-2.0, 10.1, 23.2), Vec3::new(2.0, 14.0, 23.6)); // arrival slip
    // Salt pillars framing the gallery + a couple of glowing brine seams.
    salt_pillar(b, &salt, &brine, -8.0, 14.0, 10.0, 18.0);
    salt_pillar(b, &salt, &brine, 8.0, 16.0, 10.0, 18.0);
    salt_seam(b, &brine, Vec3::new(-12.0, 13.0, 8.0), Vec3::new(-11.7, 16.0, 11.0));
    salt_seam(b, &brine, Vec3::new(11.7, 12.0, 14.0), Vec3::new(12.0, 15.0, 17.0));
    b.light(Vec3::new(0.0, 16.0, 18.0), rgb(0.85, 0.9, 1.0), 700_000.0, 38.0); // gallery lantern
    b.light(Vec3::new(0.0, 14.0, 9.0), rgb(0.8, 0.88, 1.0), 420_000.0, 26.0); // by the descent

    // Resupply — this is level 6, so the player likely arrives armed; feed ammo.
    b.item(ItemKind::Shells(20), Vec3::new(-4.0, 10.6, 12.0));
    b.item(ItemKind::Nails(30), Vec3::new(4.0, 10.6, 12.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(0.0, 10.6, 10.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 10.6, 20.0));
    // Garrison: hitscan grunts + an energy enforcer, classic early-room fodder.
    b.monster(Grunt, Vec3::new(-6.0, 11.0, 12.0));
    b.monster(Grunt, Vec3::new(6.0, 11.0, 10.0));
    b.monster(Enforcer, Vec3::new(0.0, 11.0, 8.0));

    // ======================================================================
    // DESCENT SHAFT  x[-4,4] z[-6,6]  — drops y10 → y3 into the flooded workings.
    //   A top landing (entry level) then a flight of 0.5m steps down to the
    //   cavern's staging ledge.
    // ======================================================================
    b.floor(-4.0, 4.0, 2.0, 6.0, 10.0, floor.clone()); // top landing (entry level)
    b.stairs(-4.0, 4.0, -6.0, 10.0, 3.0, 14, Vec3::Z * 1.0); // flight, high at the south
    b.wall_z(-6.0, 6.0, -4.0, 2.5, 14.0, wall.clone(), &[]);
    b.wall_z(-6.0, 6.0, 4.0, 2.5, 14.0, wall.clone(), &[]);
    b.ceiling(-4.0, 4.0, -6.0, 6.0, 14.0, ceil.clone());
    b.light(Vec3::new(0.0, 12.0, 5.0), rgb(0.85, 0.9, 1.0), 460_000.0, 28.0); // landing lantern
    b.light(Vec3::new(0.0, 7.0, -3.0), rgb(0.7, 0.85, 1.0), 360_000.0, 24.0); // stair lantern
    b.monster(Knight, Vec3::new(0.0, 4.0, -4.0)); // a guard posted on the descent

    // ======================================================================
    // THE FLOODED CAVERN  x[-20,20] z[-82,-6]  y[-4,22]  — the cart ride.
    //   One enormous sea-cavern the rail skims end to end. Only four surfaces have
    //   footing: the south STAGING LEDGE (y=3), the eastern SALT GALLERY (y=1.8,
    //   branch only), a small SALT ISLET (y=2, the Weaver bridge's reward) and the
    //   north PUMP-HOUSE VAULT (y=1.6). Everything between is black drowning water.
    // ======================================================================
    // Shell: tall side + end walls (base sunk below the waterline) and a high
    // ceiling. The west wall carries a gap for the resonant sluice-plate secret;
    // the north wall a gap for the sluice-gate; the south wall a gap where the
    // descent feeds onto the staging ledge.
    b.wall_z(-82.0, -6.0, 20.0, -4.0, 22.0, wall.clone(), &[]); // east flank
    b.wall_z(-82.0, -6.0, -20.0, -4.0, 22.0, wall.clone(), &[(-78.0, -74.0)]); // west (secret gap)
    b.wall_x(-20.0, 20.0, -82.0, -4.0, 22.0, wall.clone(), &[(-3.0, 3.0)]); // north (sluice-gate)
    b.wall_x(-20.0, 20.0, -6.0, -4.0, 22.0, wall.clone(), &[(-4.0, 4.0)]); // south (descent)
    b.ceiling(-20.0, 20.0, -82.0, -6.0, 22.0, ceil.clone());

    // --- The four footings -------------------------------------------------
    // STAGING LEDGE (y=3): full-width deck the cart is parked on. Its north edge
    // (z=-22) is a sheer drop into the water — you must ride to cross.
    b.floor(-20.0, 20.0, -22.0, -6.0, 3.0, floor.clone());
    // PUMP-HOUSE VAULT FLOOR (y=1.6): the key chamber the whole rail comes to rest
    // on, a stone platform raised just clear of the seawater.
    b.floor(-20.0, 20.0, -82.0, -66.0, 1.6, floor.clone());
    // SALT SIDE-GALLERY (y=1.8): the eastern branch ledge, an Ogre nest the high
    // branch line runs the deck straight through (the main skim never touches it).
    b.floor(6.0, 20.0, -58.0, -42.0, 1.8, floor.clone());
    // SALT ISLET (y=2): a lone salt outcrop mid-pool, reachable only by the Weaver's
    // silk bridge — its reward is the bet for crossing a bridge that drops on a kill.
    b.floor(-18.0, -10.0, -40.0, -32.0, 2.0, floor.clone());

    // --- The drowning pool filling the open middle (z[-66,-22]) -------------
    // Black surface sheen, an unlit murk below it, a thin damage volume at the
    // waterline and the kill plane in the deep. Authored by hand (NO catch floor)
    // so a faller drops clean through the surface to the kill plane and drowns,
    // rather than being caught half-submerged on a hazard slab.
    b.void_kill(VOID_KILL_Y);
    b.deco(Vec3::new(-20.0, WATER_Y - 0.2, -66.0), Vec3::new(20.0, WATER_Y, -22.0), water.clone());
    b.deco(Vec3::new(-20.0, -5.0, -66.0), Vec3::new(20.0, WATER_Y - 0.25, -22.0), deep.clone());
    b.lava.volumes.push(crate::physics::Aabb::from_corners(
        Vec3::new(-20.0, WATER_Y - 1.2, -66.0),
        Vec3::new(20.0, WATER_Y + 0.2, -22.0),
    ));

    // Salt pillars rising out of the pool to the cavern roof (deco — they never
    // block the cart or a rider), dressing the void the rail crosses.
    salt_pillar(b, &salt, &brine, -14.0, -30.0, 0.0, 22.0);
    salt_pillar(b, &salt, &brine, 16.0, -38.0, 0.0, 22.0);
    salt_pillar(b, &salt, &brine, -16.0, -56.0, 0.0, 22.0);
    salt_pillar(b, &salt, &brine, 14.0, -62.0, 0.0, 22.0);

    // ----------------------------------------------------------------------
    // THE RAIL. Main-line cart ground-points skim the pool from the staging ledge
    // to the pump-house floor, riding ~1m over the waterline the whole crossing.
    // Node 2 is the JUNCTION (switch), node 5 the weak JOINT (derail). The eastern
    // branch forks at node 2, runs the salt-gallery nest and rejoins on the vault.
    // ----------------------------------------------------------------------
    let main = [
        Vec3::new(0.0, 3.08, -20.0), // 0: parked on the staging ledge
        Vec3::new(0.0, 2.7, -26.0),  // 1: rolls off the ledge lip onto the trestle
        Vec3::new(0.0, 2.4, -34.0),  // 2: JUNCTION node (switch sits beside it)
        Vec3::new(0.0, 2.1, -44.0),  // 3: low over the black water
        Vec3::new(0.0, 1.9, -54.0),  // 4: the lowest skim over the deep
        Vec3::new(0.0, 1.75, -63.0), // 5: weak JOINT node (over the vault approach)
        Vec3::new(0.0, 1.68, -70.0), // 6: runs onto the pump-house floor
        Vec3::new(0.0, 1.68, -77.0), // 7: flat run-out, settles by the gate
    ];
    // Eastern branch tail (MUST start at the junction node); descends onto the salt
    // gallery, runs its length past the nest, then drops back to rejoin at the end.
    let branch = [
        Vec3::new(0.0, 2.4, -34.0),   // == main[2]
        Vec3::new(9.0, 2.1, -40.0),   // swings east, descending toward the gallery
        Vec3::new(13.0, 1.88, -46.0), // onto the salt-gallery deck (y=1.8)
        Vec3::new(13.0, 1.88, -56.0), // runs the gallery past the Ogre nest
        Vec3::new(7.0, 1.75, -66.0),  // drops off the gallery's north lip
        Vec3::new(0.0, 1.68, -77.0),  // rejoin == main[7]
    ];
    b.rail(&main, Some((2, &branch)), Some(5));

    // --- Pooled lanterns raking the crossing (read the cart's markers) -----
    b.light(Vec3::new(-3.0, 4.5, -24.0), rgb(0.8, 0.88, 1.0), 420_000.0, 26.0); // off the ledge
    b.light(Vec3::new(3.0, 3.5, -36.0), rgb(0.85, 0.92, 1.0), 380_000.0, 24.0); // by the junction
    b.light(Vec3::new(-2.0, 3.5, -52.0), rgb(0.7, 0.85, 1.0), 380_000.0, 26.0); // over the deep
    b.light(Vec3::new(2.0, 3.5, -64.0), rgb(0.8, 0.9, 1.0), 360_000.0, 26.0); // over the approach

    // --- Things that fight you from the deck -------------------------------
    // Scrags haunt the open cavern so the skim is fought, not coasted.
    b.monster(Scrag, Vec3::new(8.0, 5.0, -32.0));
    b.monster(Scrag, Vec3::new(-8.0, 4.5, -48.0));
    b.monster(Scrag, Vec3::new(7.0, 4.0, -60.0));

    // ----------------------------------------------------------------------
    // SALT SIDE-GALLERY NEST (y=1.8, x[6,20] z[-58,-42]) — the branch line's reach.
    //   Throwing the junction runs the deck straight through an Ogre nest; the
    //   reward stash is the bet for surviving it (the main skim never comes here).
    // ----------------------------------------------------------------------
    b.monster(Ogre, Vec3::new(13.0, 2.8, -50.0));
    b.monster(Enforcer, Vec3::new(9.0, 2.8, -45.0));
    b.monster(Scrag, Vec3::new(16.0, 4.0, -54.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(17.0, 2.4, -52.0));
    b.item(ItemKind::Rockets(10), Vec3::new(13.0, 2.4, -45.0));
    b.item(ItemKind::Health(25), Vec3::new(10.0, 2.4, -55.0));
    salt_pillar(b, &salt, &brine, 19.0, -50.0, 1.8, 22.0);
    salt_seam(b, &brine, Vec3::new(19.7, 4.0, -54.0), Vec3::new(20.0, 8.0, -48.0));
    b.light(Vec3::new(14.0, 5.0, -50.0), rgb(0.7, 0.9, 1.0), 460_000.0, 28.0);

    // ----------------------------------------------------------------------
    // SALT ISLET + WEAVER BRIDGE (y=2, x[-18,-10] z[-40,-32]). A lone outcrop in
    // the pool, reachable on foot only across the Weaver's walkable silk strand
    // slung from the staging ledge's west edge. Its reward (scarce Cells) is the
    // bet: the spider anchors the bridge, so killing it mid-crossing drops the
    // strand and drowns you. Cross, grab, get back — then kill it from safety.
    // ----------------------------------------------------------------------
    b.weaver(
        Vec3::new(-14.0, 2.1, -34.0),  // Weaver perches on the islet (adopts the strand)
        Vec3::new(-14.0, 3.0, -21.0),  // anchor A: the staging ledge's west lip
        Vec3::new(-14.0, 2.1, -33.0),  // anchor B: the islet edge
    );
    b.item(ItemKind::Cells(30), Vec3::new(-14.0, 2.6, -36.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(-16.0, 2.6, -35.0));
    salt_pillar(b, &salt, &brine, -16.0, -38.0, 2.0, 10.0);
    b.light(Vec3::new(-14.0, 5.0, -36.0), rgb(0.6, 0.85, 1.0), 380_000.0, 24.0);

    // ----------------------------------------------------------------------
    // RESONANT SLUICE-PLATE SECRET (west wall, z[-78,-74]). A cracked salt-crusted
    // plate plugging a gap in the vault's west wall; sweep the Lightning beam up to
    // its shatter note (the brine theme's wet-stone voice) and it detonates, opening
    // a niche of scarce Cells. Reachable on foot once the cart reaches the vault.
    // ----------------------------------------------------------------------
    b.floor(-23.0, -20.0, -78.0, -74.0, 1.6, floor.clone()); // niche floor behind the plate
    b.wall_z(-78.0, -74.0, -23.0, 1.6, 6.0, wall.clone(), &[]); // niche back wall
    b.wall_x(-23.0, -20.0, -78.0, 1.6, 6.0, wall.clone(), &[]); // niche sides
    b.wall_x(-23.0, -20.0, -74.0, 1.6, 6.0, wall.clone(), &[]);
    let plate = b.resonant(Vec3::new(-20.3, 1.6, -78.0), Vec3::new(-19.7, 5.0, -74.0), trim.clone());
    // A glowing fracture telegraph pinned to the plate so it hides when it shatters.
    b.deco_child(
        plate,
        Vec3::new(-20.0, 3.3, -76.0),
        Vec3::new(-19.65, 2.0, -77.4),
        Vec3::new(-19.6, 4.6, -74.6),
        brine.clone(),
    );
    b.item(ItemKind::Cells(30), Vec3::new(-21.5, 2.2, -76.0));
    b.item(ItemKind::Health(25), Vec3::new(-21.5, 2.2, -75.0));
    b.light(Vec3::new(-18.0, 3.5, -76.0), rgb(0.6, 0.9, 1.0), 200_000.0, 16.0);

    // ======================================================================
    // PUMP-HOUSE VAULT (y=1.6, z[-82,-66]) — the key chamber the rail rests on.
    //   A Death Knight holds the Silver Key; a monster pack camps the floor (the
    //   derail's wrecking-ball target). The sluice-gate in the north wall is locked.
    // ======================================================================
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 2.1, -78.0));
    b.item(ItemKind::Health(25), Vec3::new(-9.0, 2.2, -70.0));
    b.item(ItemKind::Nails(40), Vec3::new(9.0, 2.2, -70.0));
    b.light(Vec3::new(0.0, 7.0, -76.0), rgb(0.85, 0.92, 1.0), 800_000.0, 40.0);
    b.light(Vec3::new(0.0, 4.0, -68.0), rgb(0.7, 0.85, 1.0), 360_000.0, 24.0);
    salt_pillar(b, &salt, &brine, -17.0, -76.0, 1.6, 22.0);
    salt_pillar(b, &salt, &brine, 17.0, -76.0, 1.6, 22.0);
    // The monster pack guarding the floor — derail the cart into it as a battering
    // ram, or fight it the hard way after you step off.
    b.monster(Knight, Vec3::new(-5.0, 2.0, -72.0));
    b.monster(Knight, Vec3::new(5.0, 2.0, -72.0));
    b.monster(Grunt, Vec3::new(0.0, 2.0, -70.0));
    // The key's guardian, dead north by the gate.
    b.monster(DeathKnight, Vec3::new(0.0, 2.0, -80.0));
    // Key-grab ambush (classic Quake trap): an Ogre crashes in behind you.
    b.ambush(Ogre, Vec3::new(-9.0, 2.0, -78.0));

    // Locked sluice-gate filling the north-wall gap (slides up on key + near).
    // Placed LAST among the structural brushes so the cart's, the strand's, the
    // switch/joint's and the resonant plate's reserved collider slots stay valid.
    b.door(Vec3::new(-3.0, 1.5, -82.3), Vec3::new(3.0, 7.5, -81.7), Vec3::new(0.0, 6.2, 0.0));

    // ======================================================================
    // EXIT CHAMBER  x[-10,10] z[-94,-82]  floor y=1.6  h=7  — beyond the sluice-gate.
    // ======================================================================
    b.room(-10.0, 10.0, -94.0, -82.0, 1.6, 7.0, &[Wall::S((-3.0, 3.0))]);
    b.slipgate(Vec3::new(-3.0, 1.7, -93.6), Vec3::new(3.0, 6.5, -93.4));
    b.exit(Vec3::new(0.0, 3.0, -90.0));
    b.monster(Enforcer, Vec3::new(0.0, 2.6, -86.0));
    b.item(ItemKind::Health(25), Vec3::new(-6.0, 2.2, -84.0));
    b.item(ItemKind::Shells(20), Vec3::new(6.0, 2.2, -84.0));
    salt_seam(b, &brine, Vec3::new(-10.0, 5.0, -92.0), Vec3::new(-9.7, 7.5, -89.0));
    b.light(Vec3::new(0.0, 6.0, -88.0), rgb(0.8, 0.9, 1.0), 600_000.0, 34.0);

    // A dim, cold raking sun so the grey rock keeps a faint silhouette through the
    // sea-mist haze, top to bottom of the cavern.
    b.sun(Vec3::new(-14.0, 32.0, 14.0), Vec3::new(0.0, 1.0, -50.0), rgb(0.40, 0.46, 0.55), 800.0);
}

/// A pale salt pillar (deco only): a square-section column (0.7m half-width) at
/// `(cx,cz)` rising from `base_y` to `top_y`, banded near its top by a glowing
/// brine-crystal node. Visual-only, so it never blocks the cart or a rider.
fn salt_pillar(
    b: &mut Build,
    salt: &Handle<StandardMaterial>,
    glow: &Handle<StandardMaterial>,
    cx: f32,
    cz: f32,
    base_y: f32,
    top_y: f32,
) {
    const R: f32 = 0.7;
    let (lo, hi) = (base_y.min(top_y), base_y.max(top_y));
    b.deco(Vec3::new(cx - R, lo, cz - R), Vec3::new(cx + R, hi, cz + R), salt.clone());
    // A luminous crystal node banded near the top so the pillar reads in low light.
    b.deco(
        Vec3::new(cx - R - 0.12, hi - 2.2, cz - R - 0.12),
        Vec3::new(cx + R + 0.12, hi - 1.4, cz + R + 0.12),
        glow.clone(),
    );
}

/// A glowing brine-crystal seam streaked into the rock (emissive deco). Just the
/// box [min,max] in the wanted material — pulled out so the level reads as a list
/// of seams rather than a wall of `deco` calls.
fn salt_seam(b: &mut Build, mat: &Handle<StandardMaterial>, min: Vec3, max: Vec3) {
    b.deco(min, max, mat.clone());
}
