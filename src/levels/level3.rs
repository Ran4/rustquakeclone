//! Level 3 — "The Brass Leviathan". A vertical steampunk clockwork foundry
//! inside a giant brass machine: riveted copper, gauges, gears and a molten
//! channel running the length of the great foundry hall.
//!
//! Layout (looking down, north = -Z is up the page):
//!
//!        ┌──────── EXIT FURNACE ────────┐   z=-70
//!        │  slipgate + exit, fire pits   │
//!        └────────[ BRASS BULKHEAD ]─────┘   z=-46  (locked door)
//!        ┌───────────────────────────────┐
//!        │   FOUNDRY HALL (tall, h=16)    │
//!        │  ░ high GEAR-PLATFORM (KEY) ░  │   z=-42  catwalks + Ogre
//!        │   catwalk ══╗  ┌MOLTEN┐  ╔══   │
//!        │   stairs    ║  │ ▓▓▓▓ │  ║     │
//!        │             ══╝  channel └══   │
//!        └────────────[ gap ]────────────┘   z=-14
//!                  ║ corridor C1 ║
//!        ┌────────────[ gap ]────────────┐
//!        │       BOILER ENTRY ROOM        │   z=0   spawn + slipgate
//!        └───────────────────────────────┘   z=10

use bevy::prelude::*;
use crate::common::rgb;
use crate::level::Build;
use crate::level::ItemKind;
use crate::level::MonsterKind::*;
use crate::level::Wall;

pub fn build(b: &mut Build) {
    // Player spawns in the boiler room facing north (-Z), into the machine.
    b.start.pos = Vec3::new(0.0, 1.0, 7.0);
    b.start.yaw = 0.0;

    // Themed handles + a few custom steampunk materials.
    let metal = b.theme.metal.clone();
    let brass = b.tex("textures/world/metal.png", 0.35, 0.85);
    // Glowing orange valves / sparks / gear cores.
    let valve = b.mat(rgb(1.0, 0.55, 0.2), LinearRgba::rgb(4.0, 1.4, 0.2), 0.4, 0.6);
    let gearcore = b.mat(rgb(0.9, 0.6, 0.25), LinearRgba::rgb(2.0, 1.0, 0.25), 0.5, 0.8);

    // ============================================================
    // BOILER ENTRY ROOM  x[-7,7] z[-2,10]  h=6
    // ============================================================
    b.room(-7.0, 7.0, -2.0, 10.0, 0.0, 6.0, &[Wall::N((-2.0, 2.0))]);
    b.slipgate(Vec3::new(-2.0, 0.1, 9.2), Vec3::new(2.0, 4.0, 9.6));

    // Boiler tanks (vertical brass cylinders faked as tall boxes) + pipes.
    b.solid(Vec3::new(-6.5, 0.0, 6.0), Vec3::new(-4.5, 4.5, 8.0), brass.clone());
    b.solid(Vec3::new(4.5, 0.0, 6.0), Vec3::new(6.5, 4.5, 8.0), brass.clone());
    b.deco(Vec3::new(-6.0, 1.5, 5.85), Vec3::new(-5.0, 2.3, 5.9), valve.clone());
    b.deco(Vec3::new(5.0, 1.5, 5.85), Vec3::new(6.0, 2.3, 5.9), valve.clone());
    // Pipe runs along the side walls.
    b.solid(Vec3::new(-6.7, 3.5, -1.5), Vec3::new(-6.2, 4.0, 9.0), brass.clone());
    b.solid(Vec3::new(6.2, 3.5, -1.5), Vec3::new(6.7, 4.0, 9.0), brass.clone());

    // Starter loadout — the player may arrive with only a shotgun.
    b.item(ItemKind::ArmorGreen, Vec3::new(-5.0, 0.6, 2.0));
    b.item(ItemKind::Shells(20), Vec3::new(5.0, 0.6, 2.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 0.6, 0.5));

    // ============================================================
    // CORRIDOR C1  x[-2,2] z[-14,-2]
    // ============================================================
    b.corridor_z(-2.0, 2.0, -14.0, -2.0, 0.0, 5.0);
    // A glowing pressure-gauge gear above the corridor mouth.
    b.deco(Vec3::new(-1.4, 3.2, -13.9), Vec3::new(1.4, 4.6, -13.85), gearcore.clone());
    b.item(ItemKind::Nails(25), Vec3::new(0.0, 0.6, -8.0));

    // ============================================================
    // FOUNDRY HALL  x[-14,14] z[-46,-14]  floor y=0  h=16 (tall + vertical)
    //   - molten channel runs down the middle (x[-3,3])
    //   - side floors at y=0 on west (x[-14,-3]) and east (x[3,14])
    //   - catwalks at different heights span across the channel
    // ============================================================
    b.room(
        -14.0, 14.0, -46.0, -14.0, 0.0, 16.0,
        &[Wall::S((-2.0, 2.0)), Wall::N((-3.0, 3.0))],
    );

    // Molten-metal channel down the centre.
    b.hazard(-3.0, 3.0, -45.0, -15.0, 0.0);
    // Channel retaining walls (brass kerb) so the lava reads as a sunken trough.
    b.solid(Vec3::new(-3.4, 0.0, -45.0), Vec3::new(-3.0, 0.6, -15.0), brass.clone());
    b.solid(Vec3::new(3.0, 0.0, -45.0), Vec3::new(3.4, 0.6, -15.0), brass.clone());

    // --- Giant decorative GEARS on the side walls (stacked thin discs) ---
    gear(b, &gearcore, &brass, Vec3::new(-13.6, 5.0, -22.0), 3.0);
    gear(b, &gearcore, &brass, Vec3::new(-13.6, 11.0, -34.0), 4.0);
    gear(b, &gearcore, &brass, Vec3::new(13.6, 6.0, -28.0), 3.5);
    gear(b, &gearcore, &brass, Vec3::new(13.6, 12.0, -40.0), 3.0);

    // --- Long pipes spanning the hall near the ceiling ---
    b.solid(Vec3::new(-13.5, 13.5, -45.0), Vec3::new(-12.8, 14.1, -15.0), brass.clone());
    b.solid(Vec3::new(12.8, 13.5, -45.0), Vec3::new(13.5, 14.1, -15.0), brass.clone());
    b.solid(Vec3::new(-13.0, 10.0, -45.0), Vec3::new(-12.4, 10.5, -15.0), brass.clone());

    // --- Glowing valves dotted along the walls ---
    b.deco(Vec3::new(-13.85, 7.5, -19.0), Vec3::new(-13.8, 8.5, -17.0), valve.clone());
    b.deco(Vec3::new(13.8, 9.0, -25.0), Vec3::new(13.85, 10.0, -23.0), valve.clone());
    b.deco(Vec3::new(-13.85, 3.0, -37.0), Vec3::new(-13.8, 4.0, -35.0), valve.clone());

    // ---- Vertical catwalk progression over the molten channel ----
    // Stairs up the WEST side floor from y=0 to a low landing at y=3.
    b.stairs(-13.0, -9.0, -16.0, 3.0, 0.0, 6, Vec3::Z * -1.0);
    // West landing (low catwalk anchor) at y=3.
    b.solid(Vec3::new(-13.5, 2.5, -22.0), Vec3::new(-7.0, 3.0, -19.0), brass.clone());

    // LOW CATWALK: a narrow bridge across the channel at y=3, z≈-20.
    b.solid(Vec3::new(-7.0, 2.8, -21.0), Vec3::new(7.0, 3.0, -19.0), metal.clone());
    // East landing of the low catwalk.
    b.solid(Vec3::new(7.0, 2.5, -22.0), Vec3::new(13.5, 3.0, -19.0), brass.clone());

    // Stairs up the EAST landing from y=3 to a mid landing at y=7.
    b.stairs(7.5, 12.5, -25.0, 7.0, 3.0, 7, Vec3::Z * -1.0);
    // East mid landing at y=7.
    b.solid(Vec3::new(7.0, 6.5, -33.0), Vec3::new(13.5, 7.0, -29.0), brass.clone());

    // MID CATWALK: bridge across the channel at y=7, z≈-31.
    b.solid(Vec3::new(-7.0, 6.8, -32.0), Vec3::new(7.0, 7.0, -30.0), metal.clone());
    // West mid landing.
    b.solid(Vec3::new(-13.5, 6.5, -33.0), Vec3::new(-7.0, 7.0, -29.0), brass.clone());

    // Stairs up the WEST mid landing from y=7 to the high gear-platform at y=11.
    b.stairs(-12.5, -7.5, -35.0, 11.0, 7.0, 7, Vec3::Z * -1.0);
    // West high landing feeding the gear-platform.
    b.solid(Vec3::new(-13.5, 10.5, -42.0), Vec3::new(-7.0, 11.0, -38.0), brass.clone());

    // ---- HIGH GEAR-PLATFORM (the KEY perch), y=11, north end over the lava ----
    gear_platform(b, &gearcore, &brass, &metal, Vec3::new(0.0, 11.0, -42.0), 6.5);
    // The Silver Key sits on the very centre of the gear, guarded.
    b.item(ItemKind::SilverKey, Vec3::new(0.0, 11.6, -42.0));

    // --- Weapons & supplies spread along the climb ---
    // Low catwalk: Grenade Launcher + grenades.
    b.item(ItemKind::WeaponGrenade, Vec3::new(0.0, 3.6, -20.0));
    b.item(ItemKind::Rockets(10), Vec3::new(-2.0, 3.6, -20.0));
    b.item(ItemKind::Health(25), Vec3::new(11.0, 3.6, -20.5));
    // Mid catwalk: Nailgun + nails + yellow armor.
    b.item(ItemKind::WeaponNailgun, Vec3::new(0.0, 7.6, -31.0));
    b.item(ItemKind::Nails(60), Vec3::new(2.0, 7.6, -31.0));
    b.item(ItemKind::ArmorYellow, Vec3::new(-11.0, 7.6, -31.0));
    // Just before the high platform: Rocket Launcher + rockets.
    b.item(ItemKind::WeaponRocket, Vec3::new(-10.0, 11.6, -40.0));
    b.item(ItemKind::Rockets(15), Vec3::new(0.0, 11.6, -39.0));
    b.item(ItemKind::Health(25), Vec3::new(3.0, 11.6, -39.0));

    // --- Resident monsters of the foundry hall ---
    b.monster(Enforcer, Vec3::new(-10.0, 1.0, -18.0));
    b.monster(Enforcer, Vec3::new(10.0, 1.0, -42.0));
    b.monster(Knight, Vec3::new(-10.0, 1.0, -40.0));
    b.monster(Knight, Vec3::new(10.0, 7.6, -31.0));
    b.monster(Ogre, Vec3::new(-11.0, 7.6, -30.0));
    // The KEY's guardian Ogre on the high gear-platform.
    b.monster(Ogre, Vec3::new(4.0, 11.6, -42.0));

    // --- Key-grab ambush (classic Quake trap): brass hatches snap open ---
    b.ambush(Knight, Vec3::new(-4.0, 11.6, -42.0));
    b.ambush(Enforcer, Vec3::new(-12.0, 7.6, -31.0));

    // ============================================================
    // BRASS BULKHEAD (locked door) in the foundry's NORTH wall, gap x[-3,3].
    // Room height 16 >> door height 6, so it slides up out of the way.
    // ============================================================
    b.door(
        Vec3::new(-3.0, 0.0, -46.3),
        Vec3::new(3.0, 6.0, -45.7),
        Vec3::new(0.0, 6.5, 0.0),
    );

    // ============================================================
    // CORRIDOR C2  x[-3,3] z[-54,-46] — into the furnace.
    // ============================================================
    b.corridor_z(-3.0, 3.0, -54.0, -46.0, 0.0, 6.0);
    b.deco(Vec3::new(-2.4, 4.2, -53.9), Vec3::new(2.4, 5.6, -53.85), gearcore.clone());

    // ============================================================
    // EXIT FURNACE  x[-12,12] z[-70,-54]  h=10
    // ============================================================
    b.room(-12.0, 12.0, -70.0, -54.0, 0.0, 10.0, &[Wall::S((-3.0, 3.0))]);
    // Furnace fire pits flanking the exit (hazard) + a safe central walkway.
    b.hazard(-12.0, -5.0, -68.0, -56.0, 0.0);
    b.hazard(5.0, 12.0, -68.0, -56.0, 0.0);
    // Central raised brass walkway to the exit slipgate (safe over the fire).
    b.solid(Vec3::new(-4.0, 0.0, -69.0), Vec3::new(4.0, 0.5, -54.0), brass.clone());
    // Furnace gear + glowing core behind the exit.
    gear(b, &gearcore, &brass, Vec3::new(0.0, 6.0, -69.6), 4.5);
    b.deco(Vec3::new(-3.0, 1.0, -69.4), Vec3::new(3.0, 4.0, -69.35), valve.clone());

    // Exit slipgate + exit trigger.
    b.slipgate(Vec3::new(-3.0, 0.6, -69.2), Vec3::new(3.0, 5.0, -68.8));
    b.exit(Vec3::new(0.0, 1.5, -67.0));

    // A last fight guarding the furnace.
    b.monster(Ogre, Vec3::new(0.0, 1.0, -62.0));
    b.monster(Enforcer, Vec3::new(-3.0, 0.7, -58.0));
    b.item(ItemKind::Health(25), Vec3::new(0.0, 1.1, -56.0));
    b.item(ItemKind::Rockets(10), Vec3::new(-2.0, 1.1, -57.0));

    // ============================================================
    // LIGHTS — warm amber foundry glow; every area lit.
    // ============================================================
    b.sun(Vec3::new(8.0, 30.0, 10.0), Vec3::new(0.0, 0.0, -30.0), rgb(0.7, 0.55, 0.4), 1800.0);
    // Boiler room.
    b.light(Vec3::new(0.0, 4.5, 4.0), rgb(1.0, 0.7, 0.4), 600_000.0, 36.0);
    // Corridor C1.
    b.light(Vec3::new(0.0, 4.0, -8.0), rgb(1.0, 0.6, 0.3), 350_000.0, 28.0);
    // Foundry hall — molten glow from below + amber lamps at each catwalk level.
    b.light(Vec3::new(0.0, 2.0, -20.0), rgb(1.0, 0.45, 0.12), 700_000.0, 40.0);
    b.light(Vec3::new(0.0, 2.0, -38.0), rgb(1.0, 0.45, 0.12), 700_000.0, 40.0);
    b.light(Vec3::new(-9.0, 6.0, -22.0), rgb(1.0, 0.7, 0.4), 600_000.0, 34.0);
    b.light(Vec3::new(9.0, 9.0, -31.0), rgb(1.0, 0.7, 0.4), 700_000.0, 36.0);
    b.light(Vec3::new(0.0, 13.5, -40.0), rgb(1.0, 0.75, 0.45), 900_000.0, 40.0);
    b.light(Vec3::new(0.0, 13.0, -22.0), rgb(0.9, 0.7, 0.5), 600_000.0, 38.0);
    // Furnace corridor + exit room.
    b.light(Vec3::new(0.0, 4.0, -50.0), rgb(1.0, 0.6, 0.3), 350_000.0, 26.0);
    b.light(Vec3::new(0.0, 5.0, -60.0), rgb(1.0, 0.55, 0.25), 800_000.0, 38.0);
    b.light(Vec3::new(0.0, 3.0, -67.0), rgb(1.0, 0.4, 0.1), 700_000.0, 34.0);
}

/// A flat decorative GEAR stuck on a wall: stacked thin emissive brass discs
/// faked from boxes, with a glowing hub. `r` is the radius; the gear lies flat
/// against an X/Y wall (thin along Z).
fn gear(b: &mut Build, core: &Handle<StandardMaterial>, rim: &Handle<StandardMaterial>, c: Vec3, r: f32) {
    // Outer rim disc (thin along Z).
    b.deco(Vec3::new(c.x - r, c.y - r, c.z - 0.05), Vec3::new(c.x + r, c.y + r, c.z + 0.05), rim.clone());
    // Spoke cross + glowing hub.
    let h = r * 0.55;
    b.deco(Vec3::new(c.x - h, c.y - r * 1.15, c.z - 0.1), Vec3::new(c.x + h, c.y + r * 1.15, c.z + 0.1), core.clone());
    b.deco(Vec3::new(c.x - r * 1.15, c.y - h, c.z - 0.1), Vec3::new(c.x + r * 1.15, c.y + h, c.z + 0.1), core.clone());
    b.deco(Vec3::new(c.x - r * 0.35, c.y - r * 0.35, c.z - 0.15), Vec3::new(c.x + r * 0.35, c.y + r * 0.35, c.z + 0.15), core.clone());
}

/// The high circular gear-PLATFORM (walkable). A solid brass disc-ish slab with
/// emissive gear teeth and a glowing hub, centred at `c`, radius `r`.
fn gear_platform(
    b: &mut Build,
    core: &Handle<StandardMaterial>,
    rim: &Handle<StandardMaterial>,
    deck: &Handle<StandardMaterial>,
    c: Vec3,
    r: f32,
) {
    // Walkable deck (solid).
    b.solid(Vec3::new(c.x - r, c.y - 0.4, c.z - r), Vec3::new(c.x + r, c.y, c.z + r), deck.clone());
    // Gear teeth (emissive nubs) around the rim, top surface.
    let t = r + 0.5;
    b.deco(Vec3::new(c.x - 0.8, c.y, c.z - t), Vec3::new(c.x + 0.8, c.y + 0.3, c.z - t + 0.6), core.clone());
    b.deco(Vec3::new(c.x - 0.8, c.y, c.z + t - 0.6), Vec3::new(c.x + 0.8, c.y + 0.3, c.z + t), core.clone());
    b.deco(Vec3::new(c.x - t, c.y, c.z - 0.8), Vec3::new(c.x - t + 0.6, c.y + 0.3, c.z + 0.8), core.clone());
    b.deco(Vec3::new(c.x + t - 0.6, c.y, c.z - 0.8), Vec3::new(c.x + t, c.y + 0.3, c.z + 0.8), core.clone());
    // Glowing central hub.
    b.deco(Vec3::new(c.x - 1.2, c.y, c.z - 1.2), Vec3::new(c.x + 1.2, c.y + 0.25, c.z + 1.2), rim.clone());
}
