# QUAKECLONE — design

A single-mission, instantly-playable FPS in Rust + Bevy 0.19 that feels like Quake (1996).
WASD + mouse, Quake-style movement physics, multiple weapons, multiple enemies, one map
that takes a few minutes to play through.

## Pillars
- **Movement is the soul.** Quake-accurate accelerate/friction/air-accelerate physics so it
  feels fast, slippery, and skill-rewarding (strafe-jump / rocket-jump capable).
- **Crunchy combat.** Hitscan + projectiles, splash damage + knockback, blood, gibs, juice.
- **Atmosphere.** Dark techbase/dungeon, fog, dynamic lights, emissive lava, bloom.
- **End-to-end.** Title → mission → win/lose → restart. HUD, sound, objective, the works.

## Mission ("Dimension of the Doomed")
Linear-ish map, key-locked door, exit slipgate.
1. Slipgate start room → corridor.
2. Hall of the Grunts: 2–3 hitscan soldiers, shotgun shells + health.
3. Lava channel (jump across) → Ogre ledges + Nailgun.
4. Atrium: mixed Knights (melee) + Scrags (flyers), Rocket Launcher + armor.
5. Side vault: mini-boss (Death Knight) guards the **Silver Key**.
6. Locked door → final chamber: enemy cluster → **Exit slipgate = victory**.
Objectives on HUD: "Find the Silver Key" → "Reach the Exit". Live kill count.

## Movement (FixedUpdate, ~60 Hz)
Quake PM model in meters: friction, ground accelerate (wishdir/wishspeed dot clamp),
air accelerate (small cap → bunny hop / air strafe), gravity, jump.
Player = AABB vs axis-aligned brushes. Move-and-slide with plane clipping, step-up (~0.5 m)
for stairs, downward ground probe. Tuned: maxspeed ~9 m/s, accel ~100, friction ~6,
gravity ~25, jump ~8.5.

## Weapons (1–6 + wheel)
1 Shotgun (hitscan, 6 pellets) · 2 Super Shotgun (14 pellets) · 3 Nailgun (fast nail projectiles)
· 4 Grenade Launcher (bouncing, timed splash) · 5 Rocket Launcher (impact splash + rocketjump)
· 6 Lightning Gun (continuous hitscan beam, stretch).
Ammo: Shells, Nails, Rockets, Cells. Per-weapon cooldown, cost, damage, spread.
First-person view model that bobs + kicks; muzzle flash.

## Combat
Hitscan ray vs enemy AABBs + world. Projectiles: velocity + lifetime, explode → radius
splash w/ falloff + knockback (player self-knockback = rocket jump). Health/armor (armor
absorbs %). Death → gibs + corpse. Damage events.

## Enemies (AI state machine: Idle→Chase→Attack→Pain→Death, LOS raycast)
Grunt (hitscan), Enforcer (energy bolts), Ogre (grenades + melee, tanky), Knight (fast melee),
Scrag (flyer, spits), Death Knight (mini-boss). Direct chase + wall-slide (shared collision),
flyers ignore gravity.

## Effects / juice
Muzzle flash, view bob, weapon kick, screen shake, damage red flash, blood, gibs, explosions
(emissive sphere + light flash + smoke + sparks), bullet puffs, projectile trails, pickup glow/bob.
Simple particle system (vel + gravity + lifetime + fade).

## HUD
Crosshair, health / armor / ammo / weapon, objective, kill count, damage flash, pickup flash,
death + victory overlays (R restart). Quake-style bottom status bar.

## Audio (procedural WAV synthesis, pure std → assets/sounds/*.wav, loaded by AssetServer)
shotgun, ssg, nailgun, rocket fire, explosion, grenade bounce, pickups, jump/land, pains,
enemy sight/pain/death per type, door, key, victory, low ambient drone.

## Modules
main · gamestate · audio_gen (pure std) · texture_gen · level · physics (collision/raycast)
· player · camera_look · weapons · projectiles · combat · enemies · pickups · effects · hud · audio
Shared component/resource/event types live in a central `types`/`prelude` module (one contract).

## States
Title (brief) → Playing → Dead → Victory. Instant start into the mission. Esc releases cursor.
