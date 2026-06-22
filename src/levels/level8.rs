use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

// ============================================================================
// THE DROWNED COLOSSUS  —  level 8 (a deliberate stress-test of LARGE space)
//
// Half-Life's dam: you are pinned to the downstream FACE of a colossal concrete
// dam, edging along a maintenance gallery that clings to the wall. The wall is a
// convex cliff of concrete — it rises ~120m overhead and plunges ~200m below, and
// it BOWS OUT toward you (convex), so the gallery curves and you never see more
// than one reach of the dam at a time: the bulge, the spillway gate-houses and
// the cascading outfalls each hide what lies beyond. To your LEFT the wall towers
// up; to your RIGHT it drops sheer into a turbine gorge (clear the rail and you
// die). The whole traverse is ~5.9km long — roughly twenty times the other maps,
// built to see how far the renderer and the collision solver stretch.
//
// Travel runs SOUTH (-Z) from the north entry tunnel to the south powerhouse.
//
//   LEFT  (-X): the dam face, towering up to the crest & spillway gates
//   gallery     a thin catwalk hugging the convex face (you walk here)
//   RIGHT (+X): open air over the spillway gorge, ~200m down (fall = die)
// ============================================================================

// --- the dam's dimensions (one place to scale the whole thing) --------------
const Z0: f32 = 40.0; //   north end of the gallery
const LEN: f32 = 5880.0; // ~20x the length of the other maps
const Z1: f32 = Z0 - LEN; // south end
const ZMID: f32 = (Z0 + Z1) * 0.5;
const HALF: f32 = (Z0 - Z1) * 0.5;
const BULGE: f32 = 160.0; // how far the convex face bows out toward the gorge at mid-span
const WALL_TOP: f32 = 120.0;
const WALL_BASE: f32 = -200.0;
const DECK_W: f32 = 14.0; // gallery width out from the wall face
const KEY_Z: f32 = -2900.0; // the intake-control hall (mid-span, the convex apex)
const GATE_Z: f32 = -4400.0; // the locked sluice gate

/// The convex face: the wall's +X surface as a function of Z. It bows out to
/// +BULGE at mid-span and returns to ~0 at either abutment — the curve the
/// player reads as "the dam bends away" instead of a flat endless wall.
fn face_x(z: f32) -> f32 {
    let t = (z - ZMID) / HALF;
    (BULGE * (1.0 - t * t)).max(0.0)
}

pub fn build(b: &mut Build) {
    // Emerge from the north tunnel onto the face, looking south down the gallery.
    b.start.pos = Vec3::new(7.0, 1.0, 54.0);
    b.start.yaw = 0.0;

    // Theme handles.
    let floor = b.theme.floor.clone();
    let wall = b.theme.wall.clone();
    let trim = b.theme.trim.clone();
    let metal = b.theme.metal.clone();

    // Custom materials: glowing spillway water, hazard-stripe caution paint,
    // turbine-discharge river, dull distant ridges for haze depth.
    let caution = b.mat(rgb(0.85, 0.72, 0.12), LinearRgba::rgb(0.35, 0.28, 0.04), 0.5, 0.35);
    let spill = b.mat(rgb(0.70, 0.86, 0.98), LinearRgba::rgb(0.6, 1.2, 1.7), 0.18, 0.0);
    let ridge = b.mat(rgb(0.30, 0.34, 0.40), LinearRgba::BLACK, 1.0, 0.0);

    // Clear the gorge-side rail and you plunge below this plane and die.
    b.void_kill(-25.0);

    // Overcast daylight raking down the face.
    b.sun(Vec3::new(120.0, 160.0, 40.0), Vec3::new(40.0, -40.0, -200.0), rgb(0.95, 0.97, 1.0), 2600.0);

    // ---- the gorge backdrop: river floor far below + the far canyon wall ----
    b.hazard(60.0, 250.0, Z1 - 40.0, Z0 + 40.0, -198.0);
    b.solid(Vec3::new(250.0, WALL_BASE, Z1 - 40.0), Vec3::new(258.0, 70.0, Z0 + 40.0), wall.clone());
    b.deco(Vec3::new(258.0, -30.0, Z0 - 400.0), Vec3::new(360.0, 90.0, Z0 + 60.0), ridge.clone());
    b.deco(Vec3::new(258.0, -30.0, Z1 - 60.0), Vec3::new(360.0, 110.0, Z1 + 800.0), ridge.clone());

    // ====================================================================
    // THE DAM FACE — coarse, towering wall chunks following the convex curve.
    // ~320m tall and ~40m thick; the +X surface is the cliff beside the gallery.
    // ====================================================================
    let wseg = 80.0;
    let wn = (LEN / wseg).ceil() as i32;
    for i in 0..wn {
        let za = Z0 - wseg * i as f32;
        let zb = (za - wseg).max(Z1);
        let fx = face_x((za + zb) * 0.5);
        b.solid(Vec3::new(fx - 40.0, WALL_BASE, zb - 2.0), Vec3::new(fx + 0.5, WALL_TOP, za + 2.0), wall.clone());
    }
    // Back the end terraces (just beyond the gallery loop's z-range).
    b.solid(Vec3::new(-40.0, WALL_BASE, 40.0), Vec3::new(0.5, WALL_TOP, 80.0), wall.clone());
    b.solid(Vec3::new(-40.0, WALL_BASE, Z1 - 40.0), Vec3::new(0.5, WALL_TOP, -5800.0), wall.clone());

    // ====================================================================
    // THE GALLERY — fine catwalk segments hugging the face, with a gorge-side
    // rail. Each segment's small step to its neighbour is sealed by a connector.
    // ====================================================================
    let seg = 40.0;
    let n = (LEN / seg).ceil() as i32;
    let mut prev: Option<(f32, f32)> = None; // (boundary z, face x) of the last segment
    for i in 0..n {
        let za = Z0 - seg * i as f32;
        let zb = (za - seg).max(Z1);
        let fx = face_x((za + zb) * 0.5);
        // Catwalk (its inner edge tucks under the wall so it never gaps off it).
        b.floor(fx - 4.0, fx + DECK_W, zb - 1.0, za + 1.0, 0.0, floor.clone());
        // Gorge-side rail.
        b.wall_z(zb, za, fx + DECK_W, 0.0, 1.2, trim.clone(), &[]);
        if let Some((zbnd, pfx)) = prev {
            let (a, c) = (fx + DECK_W, pfx + DECK_W);
            b.wall_x(a.min(c) - 0.3, a.max(c) + 0.3, zbnd, 0.0, 1.2, trim.clone(), &[]);
        }
        prev = Some((zb, fx));
    }

    // ====================================================================
    // SPILLWAY OUTFALLS — sheets of glowing water cascading down the face.
    // ====================================================================
    let sp = 13;
    for k in 1..sp {
        let z = Z0 - LEN * (k as f32 / sp as f32);
        let fx = face_x(z);
        b.deco(Vec3::new(fx + 0.4, WALL_BASE, z - 10.0), Vec3::new(fx + 2.2, WALL_TOP, z + 10.0), spill.clone());
        // a gate housing at the crest the water pours from
        b.solid(Vec3::new(fx - 2.0, WALL_TOP - 14.0, z - 11.0), Vec3::new(fx + 6.0, WALL_TOP + 6.0, z + 11.0), wall.clone());
        b.light(Vec3::new(fx + 6.0, 10.0, z), rgb(0.55, 0.85, 1.0), 800_000.0, 46.0);
    }

    // ====================================================================
    // GATE-HOUSES — concrete blocks straddling the gallery with a low passage
    // through them. They wall off the reach beyond, so the dam reveals itself a
    // section at a time (along with the bulge of the convex face).
    // ====================================================================
    let gh = 20;
    for k in 1..gh {
        let z = Z0 - LEN * (k as f32 / gh as f32);
        if (z - KEY_Z).abs() < 80.0 || (z - GATE_Z).abs() < 70.0 {
            continue; // leave room for the setpieces
        }
        gate_house(b, z, &wall, &caution, &spill);
    }

    // ====================================================================
    // INTAKE-CONTROL HALL (mid-span, the convex apex) — the Silver Key vault.
    // The gallery widens into a terrace; an enclosed hall holds the key.
    // ====================================================================
    let kfx = face_x(KEY_Z);
    // wide terrace cantilevered off the face
    b.floor(kfx - 2.0, kfx + 26.0, KEY_Z - 18.0, KEY_Z + 18.0, 0.0, floor.clone());
    b.wall_z(KEY_Z - 18.0, KEY_Z + 18.0, kfx + 26.0, 0.0, 1.2, trim.clone(), &[]);
    // the control hall, set against the wall, passed through along the gallery
    b.room(kfx + 0.5, kfx + 16.0, KEY_Z - 12.0, KEY_Z + 12.0, 0.0, 7.0,
        &[Wall::S((kfx + 5.0, kfx + 11.0)), Wall::N((kfx + 5.0, kfx + 11.0))]);
    // key console + strong-weapon cache
    b.solid(Vec3::new(kfx + 6.0, 0.0, KEY_Z - 2.0), Vec3::new(kfx + 10.0, 1.0, KEY_Z + 2.0), metal.clone());
    b.item(ItemKind::SilverKey, Vec3::new(kfx + 8.0, 1.6, KEY_Z));
    b.item(ItemKind::WeaponRocket, Vec3::new(kfx + 3.0, 1.6, KEY_Z));
    b.item(ItemKind::Rockets(25), Vec3::new(kfx + 13.0, 1.6, KEY_Z));
    b.item(ItemKind::MegaHealth, Vec3::new(kfx + 3.0, 0.8, KEY_Z - 9.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(kfx + 13.0, 0.8, KEY_Z - 9.0));
    b.item(ItemKind::Cells(40), Vec3::new(kfx + 20.0, 0.8, KEY_Z + 8.0));
    // guards + classic key-grab ambush
    b.monster(Ogre, Vec3::new(kfx + 8.0, 1.0, KEY_Z + 6.0));
    b.monster(Knight, Vec3::new(kfx + 4.0, 1.0, KEY_Z + 9.0));
    b.monster(Knight, Vec3::new(kfx + 12.0, 1.0, KEY_Z + 9.0));
    b.monster(Enforcer, Vec3::new(kfx + 22.0, 1.0, KEY_Z + 12.0));
    b.ambush(Knight, Vec3::new(kfx + 5.0, 1.0, KEY_Z - 10.0));
    b.ambush(Knight, Vec3::new(kfx + 11.0, 1.0, KEY_Z - 10.0));
    b.ambush(Ogre, Vec3::new(kfx + 20.0, 1.0, KEY_Z - 6.0));
    b.ambush(Scrag, Vec3::new(kfx + 14.0, 5.0, KEY_Z));
    b.light(Vec3::new(kfx + 8.0, 6.0, KEY_Z), rgb(1.0, 0.88, 0.65), 1_000_000.0, 34.0);
    b.light(Vec3::new(kfx + 18.0, 6.0, KEY_Z), rgb(0.6, 0.9, 1.0), 600_000.0, 30.0);

    // ====================================================================
    // LOCKED SLUICE GATE (~3/4 of the way) — a Death Knight holds the approach.
    // A bulkhead across the gallery; the steel gate slides up once you hold the key.
    // ====================================================================
    let gfx = face_x(GATE_Z);
    b.wall_x(gfx + 0.5, gfx + DECK_W + 0.5, GATE_Z, 0.0, 6.0, wall.clone(), &[(gfx + 5.0, gfx + 11.0)]);
    b.door(
        Vec3::new(gfx + 5.0, 0.0, GATE_Z - 0.3),
        Vec3::new(gfx + 11.0, 5.0, GATE_Z + 0.3),
        Vec3::new(0.0, 5.4, 0.0),
    );
    b.monster(DeathKnight, Vec3::new(gfx + 8.0, 2.0, GATE_Z + 16.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(gfx + 4.0, 0.8, GATE_Z + 20.0));
    b.item(ItemKind::Cells(40), Vec3::new(gfx + 12.0, 0.8, GATE_Z + 20.0));
    b.light(Vec3::new(gfx + 8.0, 5.0, GATE_Z + 12.0), rgb(0.6, 0.9, 1.0), 700_000.0, 36.0);

    // ====================================================================
    // ROLLING COMBAT + SUPPLIES along the whole traverse.
    // ====================================================================
    let stops = 26;
    for k in 1..stops {
        let z = Z0 - LEN * (k as f32 / stops as f32);
        if (z - KEY_Z).abs() < 70.0 || (z - GATE_Z).abs() < 50.0 {
            continue;
        }
        let fx = face_x(z);
        let x = fx + 7.0;
        match k % 6 {
            0 => {
                b.monster(Grunt, Vec3::new(x - 3.0, 1.0, z));
                b.monster(Grunt, Vec3::new(x + 3.0, 1.0, z));
            }
            1 => {
                b.monster(Enforcer, Vec3::new(x, 1.0, z));
                b.monster(Scrag, Vec3::new(x + 3.0, 5.0, z - 5.0));
            }
            2 => {
                b.monster(Knight, Vec3::new(x - 2.0, 1.0, z));
                b.monster(Knight, Vec3::new(x + 3.0, 1.0, z));
            }
            3 => {
                b.monster(Ogre, Vec3::new(x, 1.0, z));
                b.monster(Grunt, Vec3::new(x + 4.0, 1.0, z + 4.0));
            }
            4 => {
                b.monster(Enforcer, Vec3::new(x - 3.0, 1.0, z));
                b.monster(Enforcer, Vec3::new(x + 3.0, 1.0, z));
            }
            _ => {
                b.monster(Scrag, Vec3::new(x, 6.0, z));
                b.monster(Knight, Vec3::new(x + 2.0, 1.0, z + 3.0));
            }
        }
        match k % 4 {
            0 => b.item(ItemKind::Health(25), Vec3::new(x, 0.7, z + 8.0)),
            1 => b.item(ItemKind::Shells(20), Vec3::new(x, 0.7, z + 8.0)),
            2 => b.item(ItemKind::Nails(40), Vec3::new(x, 0.7, z + 8.0)),
            _ => b.item(ItemKind::Cells(25), Vec3::new(x, 0.7, z + 8.0)),
        }
    }

    // ---- weapon pickups staged across the run ----
    weapon(b, Z0 - 140.0, ItemKind::WeaponSuperShotgun, ItemKind::Shells(20));
    weapon(b, Z0 - 1000.0, ItemKind::WeaponNailgun, ItemKind::Nails(50));
    weapon(b, Z0 - 2000.0, ItemKind::WeaponGrenade, ItemKind::Rockets(10));
    weapon(b, Z0 - 3800.0, ItemKind::WeaponLightning, ItemKind::Cells(50));

    // ---- gallery ambience: warm worklights every few hundred metres ----
    let gl = 12;
    for k in 1..gl {
        let z = Z0 - LEN * (k as f32 / gl as f32);
        b.light(Vec3::new(face_x(z) + 8.0, 6.0, z), rgb(1.0, 0.9, 0.7), 500_000.0, 42.0);
    }

    // ====================================================================
    // NORTH ENTRY TERRACE (spawn) — you step out of a tunnel onto the face.
    // ====================================================================
    b.floor(-2.0, 16.0, 38.0, 66.0, 0.0, floor.clone());
    b.wall_z(38.0, 66.0, 16.0, 0.0, 1.2, trim.clone(), &[]);
    b.wall_x(-2.0, 16.0, 66.0, 0.0, 4.0, wall.clone(), &[]);
    b.slipgate(Vec3::new(2.0, 0.1, 64.6), Vec3::new(10.0, 4.0, 65.0));
    b.item(ItemKind::ArmorGreen, Vec3::new(12.0, 0.7, 58.0));
    b.item(ItemKind::Shells(20), Vec3::new(4.0, 0.7, 58.0));
    b.light(Vec3::new(7.0, 5.0, 60.0), rgb(1.0, 0.9, 0.7), 600_000.0, 30.0);

    // A drivable flatbed truck parked on the terrace, facing south down the dam.
    // Jump onto the bed, walk to the open cab and press E to take the wheel.
    b.vehicle(Vec3::new(7.0, 0.0, 47.0), 0.0);

    // ====================================================================
    // SOUTH POWERHOUSE TERRACE (exit) — reaching the slipgate wins the campaign.
    // ====================================================================
    let efx = face_x(Z1 + 24.0);
    b.floor(efx - 2.0, efx + 18.0, Z1 - 24.0, Z1 + 24.0, 0.0, floor.clone());
    b.wall_z(Z1 - 24.0, Z1 + 24.0, efx + 18.0, 0.0, 1.2, trim.clone(), &[]);
    b.wall_x(efx - 2.0, efx + 18.0, Z1 - 24.0, 0.0, 4.0, wall.clone(), &[]);
    b.slipgate(Vec3::new(efx + 4.0, 0.1, Z1 - 23.0), Vec3::new(efx + 12.0, 4.0, Z1 - 22.6));
    b.exit(Vec3::new(efx + 8.0, 1.5, Z1 - 18.0));
    b.monster(Enforcer, Vec3::new(efx + 12.0, 1.0, Z1 + 6.0));
    b.monster(Scrag, Vec3::new(efx + 6.0, 5.0, Z1 + 8.0));
    b.item(ItemKind::Health(25), Vec3::new(efx + 4.0, 0.7, Z1 - 10.0));
    b.item(ItemKind::Cells(40), Vec3::new(efx + 14.0, 0.7, Z1 - 10.0));
    b.light(Vec3::new(efx + 8.0, 5.0, Z1 - 12.0), rgb(0.7, 0.9, 1.0), 700_000.0, 34.0);
}

/// A gate-house straddling the gallery: piers on either side of a low central
/// passage, a lintel and a hazard-stripe crown, with an outfall down the face.
/// You walk through the passage; everything beyond is hidden until you do.
fn gate_house(
    b: &mut Build,
    z: f32,
    wall: &Handle<StandardMaterial>,
    caution: &Handle<StandardMaterial>,
    spill: &Handle<StandardMaterial>,
) {
    let fx = face_x(z);
    b.solid(Vec3::new(fx + 0.5, 0.0, z - 5.0), Vec3::new(fx + 5.0, 22.0, z + 5.0), wall.clone());
    b.solid(Vec3::new(fx + 10.0, 0.0, z - 5.0), Vec3::new(fx + DECK_W + 0.5, 22.0, z + 5.0), wall.clone());
    b.solid(Vec3::new(fx + 0.5, 14.0, z - 5.0), Vec3::new(fx + DECK_W + 0.5, 22.0, z + 5.0), wall.clone());
    b.solid(Vec3::new(fx + 0.3, 20.0, z - 5.3), Vec3::new(fx + DECK_W + 0.8, 22.2, z + 5.3), caution.clone());
    b.deco(Vec3::new(fx + 11.5, -40.0, z - 2.5), Vec3::new(fx + DECK_W, 1.0, z + 2.5), spill.clone());
    b.light(Vec3::new(fx + 7.0, 7.0, z), rgb(1.0, 0.86, 0.6), 450_000.0, 26.0);
}

/// Drop a weapon pickup plus its ammo on the gallery at depth `z`.
fn weapon(b: &mut Build, z: f32, gun: ItemKind, ammo: ItemKind) {
    let x = face_x(z) + 7.0;
    b.item(gun, Vec3::new(x, 0.7, z));
    b.item(ammo, Vec3::new(x + 3.0, 0.7, z));
}
