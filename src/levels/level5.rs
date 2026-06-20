use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

// =============================================================================
// LEVEL 5 — "The Verdant Rot"  (Hive theme: alien bio-hive / toxic lab)
//
//  Layout (north = -Z, looking down from above; player starts at bottom):
//
//                          QUEEN'S NEST (key + big ambush)
//                        +-----------------------+   z -78..-62
//                        |   ###  SilverKey  ###  |
//                        |   ###   on dais   ###  |
//                        +----------[door]--------+
//                                    | C5 (locked corridor) z -62..-54
//          EGG CHAMBER               |
//        +----------------+   SLUDGE CANAL HALL (hazard + catwalks)
//        | egg  egg   egg |  +---------------------------+  z -54..-34
//        |  (Scrag swarm) |==| ~~~ TOXIC SLUDGE ~~~      |
//        +-------[gap]----+  | catwalks over the acid    |
//                |          +-------------[gap]----------+
//          C2b (z)          |
//                |        C3 (x)  to egg chamber side
//        TWISTING TUNNEL (narrow, organic)   z -34..-12
//        +------+  C2 corridor jogs +------+
//               |
//          AIRLOCK ENTRY (start)              z -12..+8
//        +------------------+
//        |   o slipgate o   |
//        +------------------+
//
//  Path: airlock -> tunnel -> sludge hall -> (egg chamber branch) ->
//        locked door -> queen's nest (key). Key opens door behind you to exit.
// =============================================================================

pub fn build(b: &mut Build) {
    // Player starts in the corroded airlock, facing north (-Z) into the hive.
    b.start.pos = Vec3::new(0.0, 1.0, 4.0);
    b.start.yaw = 0.0;

    // Quick custom emissive materials for the gross bio-glow props.
    let vein = b.mat(rgb(0.25, 0.9, 0.35), LinearRgba::rgb(0.4, 3.0, 0.6), 0.5, 0.1);
    let egg = b.mat(rgb(0.35, 1.0, 0.4), LinearRgba::rgb(0.5, 4.0, 0.8), 0.4, 0.0);
    let flesh = b.theme.wall.clone();
    let metal = b.theme.metal.clone();
    let trim = b.theme.trim.clone();

    // ========================================================================
    // AIRLOCK ENTRY  (start room)  x[-7,7] z[-12,8]  open north at x[-2,2]
    // ========================================================================
    b.room(-7.0, 7.0, -12.0, 8.0, 0.0, 5.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 7.2), Vec3::new(2.0, 4.0, 7.6));
    // corroded airlock framing + glowing veins climbing the walls
    b.solid(Vec3::new(-3.0, 0.0, -12.0), Vec3::new(-2.5, 5.0, -11.5), metal.clone());
    b.solid(Vec3::new(2.5, 0.0, -12.0), Vec3::new(3.0, 5.0, -11.5), metal.clone());
    b.deco(Vec3::new(-6.8, 0.0, -2.0), Vec3::new(-6.4, 4.5, -1.6), vein.clone());
    b.deco(Vec3::new(6.4, 0.0, 3.0), Vec3::new(6.8, 4.5, 3.4), vein.clone());
    b.deco(Vec3::new(-1.5, 4.5, 0.0), Vec3::new(1.5, 4.9, 6.0), vein.clone());
    // starting kit
    b.item(ItemKind::ArmorGreen, Vec3::new(-5.0, 0.6, 4.0));
    b.item(ItemKind::Shells(20), Vec3::new(5.0, 0.6, 4.0));
    b.item(ItemKind::Health(25), Vec3::new(-5.0, 0.6, -8.0));
    b.monster(Grunt, Vec3::new(4.0, 1.0, -9.0));

    // ========================================================================
    // C1 — short organic neck from airlock north into the tunnels
    // ========================================================================
    b.corridor_z(-2.0, 2.0, -20.0, -12.0, 0.0, 4.0);
    b.deco(Vec3::new(-1.9, 0.0, -16.0), Vec3::new(-1.6, 3.8, -15.6), vein.clone());
    b.deco(Vec3::new(1.6, 0.0, -17.5), Vec3::new(1.9, 3.8, -17.1), vein.clone());

    // ========================================================================
    // TWISTING TUNNEL — narrow claustrophobic jog (C2 x-run, C2b z-run)
    //   from x[-2,2]@z-20  ->  west to x-12  ->  north to the sludge hall
    // ========================================================================
    // small junction box so the corner is sealed
    b.room(-12.0, 2.0, -26.0, -20.0, 0.0, 4.0, &[Wall::S((-2.0, 2.0)), Wall::W((-25.0, -21.0))]);
    b.monster(Knight, Vec3::new(-6.0, 1.0, -23.0));
    b.item(ItemKind::Nails(40), Vec3::new(-10.0, 0.6, -22.0));
    b.deco(Vec3::new(-11.8, 0.0, -23.5), Vec3::new(-11.4, 3.5, -23.1), vein.clone());
    // C2: narrow tunnel running west (along X) at z[-25,-21]
    b.corridor_x(-26.0, -12.0, -25.0, -21.0, 0.0, 4.0);
    b.monster(Enforcer, Vec3::new(-20.0, 1.0, -23.0));
    b.item(ItemKind::Health(15), Vec3::new(-24.0, 0.6, -23.0));

    // corner junction at far west, turning north
    b.room(-30.0, -26.0, -34.0, -20.0, 0.0, 4.0, &[Wall::E((-25.0, -21.0)), Wall::N((-30.0, -26.0))]);
    b.deco(Vec3::new(-29.8, 0.0, -27.0), Vec3::new(-29.4, 3.6, -26.6), vein.clone());
    b.item(ItemKind::WeaponNailgun, Vec3::new(-28.0, 0.6, -30.0));

    // C2b: tunnel running north (along Z) at x[-30,-26]
    b.corridor_z(-30.0, -26.0, -42.0, -34.0, 0.0, 4.0);

    // ========================================================================
    // SLUDGE CANAL HALL  x[-30,6] z[-54,-42]  — toxic sludge crossed by catwalks
    //   open south at x[-30,-26] (from C2b), open north at x[-2,2] (locked door)
    // ========================================================================
    b.room(-30.0, 6.0, -54.0, -42.0, 0.0, 7.0,
        &[Wall::S((-30.0, -26.0)), Wall::N((-2.0, 2.0)), Wall::E((-50.0, -46.0))]);
    // recess the floor as a sludge canal: dig a pit by raising side decks instead.
    // bubbling toxic sludge pool across the middle of the hall
    b.hazard(-28.0, 4.0, -52.0, -44.0, 0.2);
    // fleshy catwalks (solid) crossing the acid — must be walked to reach the door
    b.solid(Vec3::new(-30.0, 0.0, -49.5), Vec3::new(-18.0, 0.5, -46.5), flesh.clone());
    b.solid(Vec3::new(-18.0, 0.0, -49.5), Vec3::new(-6.0, 0.5, -46.5), trim.clone());
    b.solid(Vec3::new(-6.0, 0.0, -49.0), Vec3::new(6.0, 0.5, -45.5), flesh.clone());
    // a stepping platform bridging to the north door lane
    b.solid(Vec3::new(-3.0, 0.0, -49.0), Vec3::new(3.0, 0.5, -42.0), trim.clone());
    // glowing egg-sac clusters bulging from the hall walls
    egg_sac(b, egg.clone(), Vec3::new(-26.0, 1.0, -53.0));
    egg_sac(b, egg.clone(), Vec3::new(2.0, 1.2, -53.0));
    // Scrag swarm over the acid + an Enforcer on the far deck
    b.monster(Scrag, Vec3::new(-20.0, 4.0, -48.0));
    b.monster(Scrag, Vec3::new(-8.0, 5.0, -49.0));
    b.monster(Scrag, Vec3::new(4.0, 4.5, -48.0));
    b.monster(Enforcer, Vec3::new(-12.0, 0.7, -47.5));
    b.item(ItemKind::WeaponRocket, Vec3::new(-12.0, 0.9, -48.0));
    b.item(ItemKind::Rockets(15), Vec3::new(-9.0, 0.9, -48.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(-24.0, 0.9, -48.0));

    // ========================================================================
    // EGG CHAMBER (branch east, off the sludge hall)  x[6,26] z[-58,-40]
    //   open west at z[-50,-46] — pulsating egg-sacs + a Scrag/Ogre nest
    // ========================================================================
    b.room(6.0, 26.0, -58.0, -40.0, 0.0, 6.0, &[Wall::W((-50.0, -46.0))]);
    // a bridge of floor over the wall gap so the player can cross from the hall
    b.solid(Vec3::new(4.0, 0.0, -49.5), Vec3::new(8.0, 0.5, -46.5), trim.clone());
    // clusters of pulsating egg-sacs (stacked emissive boxes ~ spheres)
    egg_sac(b, egg.clone(), Vec3::new(10.0, 1.0, -55.0));
    egg_sac(b, egg.clone(), Vec3::new(15.0, 1.0, -44.0));
    egg_sac(b, egg.clone(), Vec3::new(21.0, 1.0, -53.0));
    egg_sac(b, egg.clone(), Vec3::new(22.0, 1.0, -45.0));
    egg_sac(b, egg.clone(), Vec3::new(13.0, 1.0, -50.0));
    // the egg-chamber guardian + flyers
    b.monster(Ogre, Vec3::new(20.0, 1.0, -50.0));
    b.monster(Scrag, Vec3::new(14.0, 5.0, -48.0));
    b.monster(Scrag, Vec3::new(18.0, 5.5, -53.0));
    b.item(ItemKind::WeaponLightning, Vec3::new(24.0, 0.8, -50.0));
    b.item(ItemKind::Cells(60), Vec3::new(24.0, 0.8, -47.0));
    b.item(ItemKind::MegaHealth, Vec3::new(8.0, 0.8, -56.0));
    b.deco(Vec3::new(25.6, 0.0, -50.0), Vec3::new(26.0, 5.5, -49.6), vein.clone());

    // ========================================================================
    // C5 — open neck up into the Queen's nest  x[-2,2] z[-62,-54]
    //   (the Silver Key lives in the nest, so this approach is NOT locked —
    //    the locked door instead gates the EXIT beyond the nest.)
    // ========================================================================
    b.corridor_z(-2.0, 2.0, -62.0, -54.0, 0.0, 5.0);
    b.deco(Vec3::new(-1.9, 0.0, -58.0), Vec3::new(-1.6, 4.6, -57.6), vein.clone());
    b.deco(Vec3::new(1.6, 0.0, -59.0), Vec3::new(1.9, 4.6, -58.6), vein.clone());

    // ========================================================================
    // QUEEN'S NEST  x[-16,16] z[-80,-62]  — the Silver Key on a fleshy dais
    //   open south at x[-2,2] (from C5)
    // ========================================================================
    b.room(-16.0, 16.0, -80.0, -62.0, 0.0, 9.0, &[Wall::S((-2.0, 2.0)), Wall::N((-2.0, 2.0))]);
    // raised central dais of overlapping flesh-mass
    b.solid(Vec3::new(-4.0, 0.0, -76.0), Vec3::new(4.0, 1.2, -68.0), trim.clone());
    b.solid(Vec3::new(-2.5, 1.2, -74.5), Vec3::new(2.5, 1.8, -69.5), flesh.clone());
    // glowing egg-sacs ringing the nest
    egg_sac(b, egg.clone(), Vec3::new(-13.0, 1.0, -78.0));
    egg_sac(b, egg.clone(), Vec3::new(13.0, 1.0, -78.0));
    egg_sac(b, egg.clone(), Vec3::new(-13.0, 1.0, -64.0));
    egg_sac(b, egg.clone(), Vec3::new(13.0, 1.0, -64.0));
    egg_sac(b, egg.clone(), Vec3::new(0.0, 1.0, -78.5));
    // dripping vein columns
    b.deco(Vec3::new(-9.0, 0.0, -77.0), Vec3::new(-8.4, 8.5, -76.4), vein.clone());
    b.deco(Vec3::new(8.4, 0.0, -77.0), Vec3::new(9.0, 8.5, -76.4), vein.clone());
    // THE KEY — on the dais, guarded by the nest's resident Ogre
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 2.0, -72.0));
    b.monster(Ogre, Vec3::new(-11.0, 1.0, -70.0));
    b.monster(Enforcer, Vec3::new(11.0, 1.0, -70.0));
    b.monster(Scrag, Vec3::new(0.0, 6.0, -78.0));
    // support supplies so the grab is survivable
    b.item(ItemKind::Cells(40), Vec3::new(-14.0, 0.9, -66.0));
    b.item(ItemKind::Health(25), Vec3::new(14.0, 0.9, -66.0));
    b.item(ItemKind::Rockets(15), Vec3::new(0.0, 0.9, -64.0));

    // ---- BIG KEY AMBUSH: the nest erupts when the key is taken ----
    b.ambush(Knight, Vec3::new(-13.0, 1.0, -76.0));
    b.ambush(Knight, Vec3::new(13.0, 1.0, -76.0));
    b.ambush(Scrag, Vec3::new(-6.0, 5.0, -77.0));
    b.ambush(Scrag, Vec3::new(6.0, 5.0, -77.0));
    b.ambush(DeathKnight, Vec3::new(0.0, 1.0, -78.0));
    // and reinforcements flooding the corridor behind you
    b.ambush(Ogre, Vec3::new(0.0, 1.0, -58.0));

    // ========================================================================
    // LOCKED DOOR — north wall of the Queen's nest (gap x[-2,2] @ z-80). Gates
    //   the exit; opens once the Silver Key (held from the dais) is carried near.
    //   Nest height 9 >= door height 6, so it slides up.
    // ========================================================================
    b.door(Vec3::new(-2.0, 0.0, -80.3), Vec3::new(2.0, 6.0, -79.7), Vec3::new(0.0, 6.2, 0.0));

    // ========================================================================
    // EXIT VENT  x[-8,8] z[-92,-80]  — the slipgate out, beyond the locked door
    // ========================================================================
    b.room(-8.0, 8.0, -92.0, -80.0, 0.0, 6.0, &[Wall::S((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-3.0, 0.1, -91.6), Vec3::new(3.0, 4.5, -91.2));
    b.exit(Vec3::new(0.0, 1.2, -90.0));
    egg_sac(b, egg.clone(), Vec3::new(-6.0, 0.6, -90.0));
    egg_sac(b, egg.clone(), Vec3::new(6.0, 0.6, -90.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 0.6, -83.0));
    b.light(Vec3::new(0.0, 4.0, -86.0), rgb(0.45, 1.1, 0.55), 600_000.0, 32.0);
    let _ = metal;

    // ========================================================================
    // LIGHTS — sickly green glow in every area (pitch black otherwise) + one sun
    // ========================================================================
    b.sun(Vec3::new(-10.0, 30.0, 10.0), Vec3::new(0.0, 0.0, -40.0), rgb(0.45, 0.6, 0.45), 1800.0);
    // airlock
    b.light(Vec3::new(0.0, 4.0, -2.0), rgb(0.4, 1.0, 0.5), 600_000.0, 36.0);
    b.light(Vec3::new(0.0, 4.0, 5.0), rgb(0.5, 1.0, 0.6), 400_000.0, 30.0);
    // tunnels
    b.light(Vec3::new(-6.0, 3.2, -23.0), rgb(0.4, 1.0, 0.5), 450_000.0, 30.0);
    b.light(Vec3::new(-20.0, 3.2, -23.0), rgb(0.45, 1.0, 0.5), 450_000.0, 30.0);
    b.light(Vec3::new(-28.0, 3.2, -30.0), rgb(0.4, 1.0, 0.5), 450_000.0, 30.0);
    b.light(Vec3::new(-28.0, 3.2, -38.0), rgb(0.4, 1.0, 0.5), 450_000.0, 30.0);
    // sludge hall — bright sickly acid glow
    b.light(Vec3::new(-20.0, 5.0, -48.0), rgb(0.4, 1.1, 0.4), 900_000.0, 45.0);
    b.light(Vec3::new(-2.0, 5.0, -48.0), rgb(0.45, 1.1, 0.45), 900_000.0, 45.0);
    b.light(Vec3::new(-14.0, 1.5, -48.0), rgb(0.3, 1.3, 0.3), 500_000.0, 28.0);
    // egg chamber
    b.light(Vec3::new(16.0, 4.5, -50.0), rgb(0.4, 1.2, 0.5), 800_000.0, 42.0);
    b.light(Vec3::new(10.0, 2.5, -52.0), rgb(0.5, 1.3, 0.55), 500_000.0, 30.0);
    // locked corridor
    b.light(Vec3::new(0.0, 4.0, -58.0), rgb(0.45, 1.0, 0.55), 450_000.0, 26.0);
    // queen's nest — big central glow + edge lights
    b.light(Vec3::new(0.0, 6.5, -71.0), rgb(0.4, 1.2, 0.5), 1_300_000.0, 50.0);
    b.light(Vec3::new(-12.0, 4.0, -76.0), rgb(0.45, 1.1, 0.45), 700_000.0, 38.0);
    b.light(Vec3::new(12.0, 4.0, -76.0), rgb(0.45, 1.1, 0.45), 700_000.0, 38.0);
    b.light(Vec3::new(0.0, 4.0, -79.0), rgb(0.5, 1.2, 0.6), 700_000.0, 36.0);
}

/// A pulsating egg-sac: a cluster of emissive boxes stacked to read as a
/// bulging organic sphere. Visual-only (deco), placed with its base at `base`.
fn egg_sac(b: &mut Build, mat: Handle<StandardMaterial>, base: Vec3) {
    let c = base;
    // wide lower bulge
    b.deco(c + Vec3::new(-0.9, 0.0, -0.9), c + Vec3::new(0.9, 0.7, 0.9), mat.clone());
    // mid body
    b.deco(c + Vec3::new(-1.1, 0.4, -1.1), c + Vec3::new(1.1, 1.3, 1.1), mat.clone());
    // upper taper
    b.deco(c + Vec3::new(-0.7, 1.1, -0.7), c + Vec3::new(0.7, 1.9, 0.7), mat.clone());
    // tip
    b.deco(c + Vec3::new(-0.35, 1.7, -0.35), c + Vec3::new(0.35, 2.3, 0.35), mat);
}
