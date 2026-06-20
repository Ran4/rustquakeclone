# QUAKECLONE — *Seven Dimensions*

A seven-level, Quake-flavoured first-person shooter written from scratch in **Rust + Bevy 0.19**.
WASD + mouse, Quake-style movement physics, seven weapons, six monster types, seven hand-built brush
maps each with its own theme, key-locked doors, environmental hazards, gibs, explosions and a
procedurally-synthesised sound set — it boots straight into the campaign and each level plays through
in a few minutes.

```
cargo run --release
```

(First build pulls and compiles Bevy, so it takes a while. `cargo run` works too but the dev profile
already optimises dependencies so the game is smooth.)

> Linux needs the usual Bevy deps (ALSA + a Vulkan/GL driver). The window starts immediately —
> **click once** to capture the mouse, then play. Press **Esc** to release the mouse.

## Configuration

The game starts **fullscreen** by default. Edit `config.ron` to change that — it's read at
startup (omit any field to keep its default):

```ron
(
    fullscreen: true,   // false → run in a window
    width: 1280,        // window size, used only when fullscreen is false
    height: 720,
    vsync: true,        // false → uncap the frame rate (watch the fps meter, top-left)
)
```

`config.ron` is looked for in the working directory, the crate root, then next to the executable;
if it's missing or malformed the defaults above apply.

## Controls

| Action            | Key |
|-------------------|-----|
| Move              | **W A S D** |
| Jump              | **Space** (hold to bunny-hop) |
| Look / aim        | **Mouse** |
| Fire              | **Left Mouse** (hold for automatic weapons) |
| Select weapon     | **1–7** or **mouse wheel** |
| Restart (on death / victory) | **R** |
| Release cursor    | **Esc** |

## The campaign

Seven self-contained dimensions, each a hand-built brush map with its own theme, textures, fog,
lighting and hazard. Every level is the same Quake loop — **grab the Silver Key → open the locked
door → reach the exit slipgate** — but the world around it changes completely:

1. **Dimension of the Doomed** — the original dark techbase-dungeon, lava and a Death Knight vault.
2. **Frostspire Keep** — a winter ice fortress; cross a cracked frozen lake on ice-block stepping
   stones, climb to the keep.
3. **The Brass Leviathan** — a vertical steampunk clockwork foundry; ascending catwalks over a
   molten-metal channel, giant gears, a brass bulkhead.
4. **Tomb of the Sunken King** — an Egyptian desert tomb descending underground; obelisks, gold
   sarcophagi, cursed quicksand pits, a pharaoh's vault.
5. **The Verdant Rot** — an alien bio-hive / toxic lab; organic tunnels, toxic-sludge canals,
   pulsating egg chambers, a Queen's nest.
6. **The Salt Wraith** — a sci-fi sky-pirate galleon among the clouds; open decks, plank bridges
   over a plasma-engine void, masts and energy sails, a captain's cabin vault.
7. **Sanctum of the Void** — the cosmic finale; floating obsidian islands and glowing crystal
   bridges over a lethal void rift, a Death Knight boss guarding the key.

- **A fresh run starts on a random level.** Finish a level and you carry your weapons, ammo, health
  and armor straight into the next one; finish the last and the campaign is won.
- A **"Level N: Name"** banner announces each level for a few seconds as it begins.
- **Objective HUD** tracks *Find the Silver Key → Reach the Exit* plus a live kill count.

## Weapons

1. **Shotgun** — hitscan, 6 pellets (starting weapon)
2. **Super Shotgun** — hitscan, 14-pellet double blast
3. **Nailgun** — rapid nail projectiles
4. **Grenade Launcher** — bouncing, timed grenades with splash
5. **Rocket Launcher** — direct + splash damage, *rocket-jump capable*
6. **Lightning Gun** — continuous hitscan beam
7. **Whip** — short-range melee, *uses no ammo*; flings whipped monsters back with heavy knockback

Ammo types: Shells, Nails, Rockets, Cells. The Whip needs none — it's the always-available
melee fallback (and a handy way to shove a charging Knight off you). Armor (green/yellow) absorbs a
fraction of damage.

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
hell-plate, etc.). The brush-built world is skinned the same way: the floors, walls, ceilings,
trim, metal, door and lava use seamless tiling textures in `assets/textures/world/`. Each brush is
emitted as its own mesh with **world-aligned UVs** (uniform texel density, so the tiling lines up
between adjacent brushes), and the textures load with a repeat sampler.

### Generating images

All textures are produced by one idempotent script — the single registry of every image the game
needs:

```
uv run scripts/generate_images.py          # render anything whose PNG is missing
uv run scripts/generate_images.py --list   # show what exists vs. is missing
uv run scripts/generate_images.py --force  # re-render everything
```

It scans the manifest, skips any image that already has a PNG on disk, and renders only the rest
with `gpt-image-2` (reading `OPENAI_API_KEY` from the env or `.env`). Add an entry to the manifest
to introduce a new texture; delete a PNG to regenerate it.

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
| `level.rs` | Themed material palettes, the `Build` level-authoring API, level registry + dispatch |
| `levels/` | One module per level (`level1`..`level7`) — each a `build(&mut Build)` map |
| `levelshot.rs` | `QC_LEVELSHOT=1` debug mode: build each level and screenshot its entry view |
| `gamestate.rs` | Objective, doors, hazards, exit→next-level / win, death/victory, restart |
| `hud.rs` | Crosshair, status, objective, flash, notifications, level banner |
| `effects.rs` | Particles, explosions, gibs, screen shake, view bob |
| `audio.rs` / `audio_gen.rs` | Playback + procedural WAV synthesis |

Each level is authored against the small `Build` API in `level.rs` (`room`, `corridor_z/x`,
`stairs`, `door`, `hazard`, `monster`, `item`, `light`, …) and registered in `levels/mod.rs`. A
level just lays brushes and fills the spawn plan; the active **theme** supplies its textures, fog,
ambient light and hazard styling.

Run `QC_AUTOTEST=1 cargo run` to launch a self-driving smoke test (the player walks forward and
fires while logging position/health) — handy for headless validation.

Run `QC_GALLERY=1 cargo run` to spawn each monster in turn, lit and centered, and save a close-up
`gallery_<n>_<kind>.png` of every model — handy for eyeballing the rigs and textures.

Run `QC_LEVELSHOT=1 cargo run` to build each level in turn and save `levelshot_<n>_<name>.png` from
the player's entry vantage — handy for eyeballing every level's geometry, textures and lighting.
`QC_LEVEL=<n> cargo run` forces the campaign to start on a specific level (0-indexed), and
`QC_EXIT_RUSH=1 cargo run` teleports the player onto each exit to fast-forward through the campaign
(validates level progression + weapon carry-over).
