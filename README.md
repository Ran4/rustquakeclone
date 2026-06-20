# QUAKECLONE — *Dimension of the Doomed*

A single-mission, Quake-flavoured first-person shooter written from scratch in **Rust + Bevy 0.19**.
WASD + mouse, Quake-style movement physics, six weapons, six monster types, hand-built brush map,
a key-locked door, lava, gibs, explosions and a procedurally-synthesised sound set — it boots
straight into the mission and plays through in a few minutes.

```
cargo run --release
```

(First build pulls and compiles Bevy, so it takes a while. `cargo run` works too but the dev profile
already optimises dependencies so the game is smooth.)

> Linux needs the usual Bevy deps (ALSA + a Vulkan/GL driver). The window starts immediately —
> **click once** to capture the mouse, then play. Press **Esc** to release the mouse.

## Controls

| Action            | Key |
|-------------------|-----|
| Move              | **W A S D** |
| Jump              | **Space** (hold to bunny-hop) |
| Look / aim        | **Mouse** |
| Fire              | **Left Mouse** (hold for automatic weapons) |
| Select weapon     | **1–6** or **mouse wheel** |
| Restart (on death / victory) | **R** |
| Release cursor    | **Esc** |

## The mission

Punch out of the slipgate and fight through the techbase-dungeon: the Hall of the Grunts, a
lava-split corridor, the Ogre ledges, the great Atrium and the Death Knight's vault. Grab the
**Silver Key**, open the locked door, reach the **exit slipgate** — and try to survive.

- **Objective HUD** tracks *Find the Silver Key → Reach the Exit* plus a live kill count.

## Weapons

1. **Shotgun** — hitscan, 6 pellets (starting weapon)
2. **Super Shotgun** — hitscan, 14-pellet double blast
3. **Nailgun** — rapid nail projectiles
4. **Grenade Launcher** — bouncing, timed grenades with splash
5. **Rocket Launcher** — direct + splash damage, *rocket-jump capable*
6. **Lightning Gun** — continuous hitscan beam

Ammo types: Shells, Nails, Rockets, Cells. Armor (green/yellow) absorbs a fraction of damage.

## Monsters

Grunt (hitscan soldier) · Enforcer (energy bolts) · Knight (fast melee) · Scrag (flying spitter) ·
Ogre (grenade-lobbing bruiser + chainsaw) · Death Knight (mini-boss). Each has line-of-sight
perception and an Idle → Chase → Attack AI.

Every monster is a **procedurally-assembled skeletal model** in the angular, low-poly Quake style:
a hierarchy of parented bone entities (pelvis → torso → head/jaw, articulated arms and legs, plus
per-kind extras — the Ogre's chainsaw, the Scrag's wings and tail, the Knight's and Death Knight's
swords). Body parts are **faceted low-poly meshes** — tapered muscular limbs, splayed claws and
overlapping muscle-mass blobs — that are flat-shaded and vertex-jittered (so they read as hard,
hand-modeled facets, not smooth balloons) and skinned with a texture. A procedural animator drives
the bones every frame — a speed-scaled walk cycle, idle breathing, attack swings, a pain flinch and
a death topple. No binary mesh assets: the skeleton *is* the entity hierarchy.

The skin textures live in `assets/textures/monsters/` and were generated with OpenAI's
`gpt-image-2` (seamless dark-fantasy albedo maps — rotting flesh, flak armor, demon hide, obsidian
hell-plate, etc.).

## What makes it feel like Quake

- **Movement physics**: friction + ground/air acceleration with the classic air-strafe speed cap,
  gravity, jumping, step-up over stairs/ledges — all on a custom swept-AABB collision solver.
- **Crunchy combat**: hitscan + projectiles, radius splash with falloff and knockback, blood, gibs,
  expanding fireballs, dynamic muzzle/explosion lights, screen shake, view bob, weapon kick, damage flash.
- **Atmosphere**: dark brush-built level, distance fog, HDR + bloom on emissive lava/lights, point lights.
- **Procedural audio**: all 23 sound effects are synthesised at startup (no binary assets) into
  `assets/sounds/` and loaded by Bevy.

## Architecture (single crate, `src/`)

| Module | Responsibility |
|--------|----------------|
| `main.rs` | App, plugins, schedules, window, game-state wiring |
| `common.rs` | Shared components, resources, messages, tuning constants |
| `physics.rs` | Swept-AABB move-and-slide, step-up, ground probe, raycasts |
| `player.rs` | Spawn, mouse-look, Quake movement, cursor grab |
| `weapons.rs` | Inventory, firing (hitscan/projectile), switching, view-model |
| `projectiles.rs` | Rockets, grenades, nails, enemy bolts |
| `combat.rs` | Damage, armor, explosions, death, gibs |
| `enemies.rs` | Monster spawning + AI state machine + attacks |
| `monster_model.rs` | Procedural skeletal monster rigs, textures, bone animation + death topple |
| `gallery.rs` | `QC_GALLERY=1` debug mode: render each monster solo and screenshot it |
| `pickups.rs` | Health, armor, ammo, weapons, key |
| `level.rs` | The brush-built map + spawn plan |
| `gamestate.rs` | Objective, door, lava, exit, death/victory, restart |
| `hud.rs` | Crosshair, status, objective, flash, notifications |
| `effects.rs` | Particles, explosions, gibs, screen shake, view bob |
| `audio.rs` / `audio_gen.rs` | Playback + procedural WAV synthesis |

Run `QC_AUTOTEST=1 cargo run` to launch a self-driving smoke test (the player walks forward and
fires while logging position/health) — handy for headless validation.

Run `QC_GALLERY=1 cargo run` to spawn each monster in turn, lit and centered, and save a close-up
`gallery_<n>_<kind>.png` of every model — handy for eyeballing the rigs and textures.
