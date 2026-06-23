# QUAKECLONE — *Eight Dimensions*

An eight-level, Quake-flavoured first-person shooter written from scratch in **Rust + Bevy 0.19**.
WASD + mouse, Quake-style movement physics, seven weapons, seven monster types, eight hand-built brush
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
    start_level: 1,         // 1–8 to always start on that level (default 1), "random" for a random one each run
)
```

`config.ron` is looked for in the working directory, the crate root, then next to the executable;
if it's missing or malformed the defaults above apply.

## Controls

| Action            | Key |
|-------------------|-----|
| Move              | **W A S D** |
| Jump              | **Space** (hold to bunny-hop; tap again in mid-air next to a wall to **wall-jump** off it) |
| Wall-run          | Carry speed into a wall and **hold a move key into/along it** — sprint flat across it for a short stamina window, then peel off keeping your momentum |
| Look / aim        | **Mouse** |
| Fire              | **Left Mouse** (hold for automatic weapons) |
| Select weapon     | **1–7** or **mouse wheel** |
| Enter / exit vehicle | **E** (while standing on the driver spot) |
| Restart (on death / victory) | **R** |
| Release cursor    | **Esc** |

## The campaign

Eight self-contained dimensions, each a hand-built brush map with its own theme, textures, fog,
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
7. **Sanctum of the Void** — the cosmic sanctum; floating obsidian islands and glowing crystal
   bridges over a lethal void rift, a Death Knight boss guarding the key.
8. **The Drowned Colossus** — the finale and a deliberate big-space stress test; you fight along the
   crest of a colossal concrete dam. The crest is a convex arch that bulges toward the gorge, so the
   span, the spillway gate-houses and the staggered control piers reveal the dam a reach at a time
   rather than all at once. The reservoir is held high on one side; the face drops ~70m into a
   turbine-discharge gorge on the other (a fatal fall either way). The Silver Key sits in a mid-span
   intake control house; a Death Knight holds the locked flood-gate before the exit. A **drivable
   flatbed truck** is parked on the entry terrace (see *Vehicles* below) — hop in and floor it.

- **A fresh run starts on level 1** (set `start_level` in `config.ron` to a fixed level or `"random"`).
  Finish a level and you carry your weapons, ammo, health and armor straight into the next one; finish
  the last and the campaign is won.
- A **"Level N: Name"** banner announces each level for a few seconds as it begins.
- **Objective HUD** tracks *Find the Silver Key → Reach the Exit* plus a live kill count.

## Weapons

1. **Shotgun** — hitscan, 6 pellets (starting weapon)
2. **Super Shotgun** — hitscan, 14-pellet double blast
3. **Nailgun** — rapid nail projectiles
4. **Grenade Launcher** — bouncing, timed grenades with splash; *right-click lobs a **Lodestone*** — a
   gravity-well grenade that **implodes**, dragging monsters (and loose projectiles) into a knot before it pops
5. **Rocket Launcher** — direct + splash damage, *rocket-jump capable*
6. **Lightning Gun** — continuous hitscan beam; *hold it on a **resonant** brush (cracked ice, a brass
   bulkhead…) and its hum sweeps up to the surface's shatter note — the wall detonates and opens a shortcut*
7. **Whip** — short-range melee, *uses no ammo*; flings whipped monsters back with heavy knockback. A
   well-timed swing also **parries** — bat an incoming Enforcer bolt, Scrag spit or Ogre grenade out of
   the air and back down its own throat (a perfect-window return screams back faster and homing, killing
   the shooter with their own ammo; a late swat just pops it harmlessly)

Ammo types: Shells, Nails, Rockets, Cells. The Whip needs none — it's the always-available
melee fallback (and a handy way to shove a charging Knight off you). Armor (green/yellow) absorbs a
fraction of damage.

## Vehicles

See `.claude/rules/vehicles.md`

## Monsters

Grunt (hitscan soldier) · Enforcer (energy bolts) · Knight (fast melee) · Scrag (flying spitter) ·
Ogre (grenade-lobbing bruiser + chainsaw) · Death Knight (mini-boss) · Weaver (venom-spitting spider
that anchors **walkable silk-strand bridges** — kill it and its bridge vanishes, dropping anything on
it). Each has line-of-sight perception and an Idle → Chase → Attack AI.

**Hijack the horde** — crack the **Whip** across an Ogre to stagger it, then press **E** within a
moment to vault onto its shoulders and *ride the monster itself* as a living battering ram: **W**
charges it forward, **A/D** steer its lumbering gait, and **left mouse** swings its own chainsaw arm
into whatever's ahead (your own guns are stowed while mounted). You ride on the Ogre's health — hits
aimed at you drain it instead — until it dies, at which point its death-topple bucks you off. Press
**E** to dismount early. It's the same live skeletal rig that was just fighting you, with no separate
vehicle model.

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

The weapons are skinned too: a small set of material albedos in `assets/textures/weapons/`
(gunmetal, brass, steel, wood and a tintable painted sheet) skins both the ground-pickup gun models
and the first-person view-models by material role — receivers/barrels/stocks pick the matching
metal or wood, while the green/red/blue gun bodies tint the neutral painted sheet. Ammo, health,
armor and the key stay flat-shaded so the glowing accents read at a glance.

### Generating images

See `.claude/rules/generating_images.md`

### Generating sounds

Most SFX are synthesised in `src/audio_gen.rs`, but the vehicle sounds want real recorded-sounding
audio, so they're rendered with ElevenLabs by one idempotent script — the registry of every such
sound:

```
uv run scripts/generate_sounds.py          # render anything whose WAV is missing
uv run scripts/generate_sounds.py --list   # show what exists vs. is missing
uv run scripts/generate_sounds.py --force  # re-render everything
```

It reads `ELEVENLABS_API_KEY` from the env or `.env`, transcodes the result to mono WAV with
`ffmpeg`, and writes to `assets/sounds/`. These WAVs are committed (unlike the procedural ones,
which are gitignored build artifacts) because they're not reproducible without the key.

## What makes it feel like Quake

- **Movement physics**: friction + ground/air acceleration with the classic air-strafe speed cap,
  gravity, jumping, wall-jumping (kick off a wall in mid-air for an upward boost + push away),
  **wall-running** (carry speed into a wall and hold the move key to sprint flat along it for a short
  stamina window, then peel off keeping every scrap of momentum), step-up over stairs/ledges — all on
  a custom swept-AABB collision solver.
- **Crunchy combat**: hitscan + projectiles, radius splash with falloff and knockback, blood, gibs,
  expanding fireballs, dynamic muzzle/explosion lights, screen shake, view bob, weapon kick, damage flash.
  A clean kill leaves a **ragdoll-brush corpse** — for a few seconds the body is a real solid you can
  shove with splash/Whip/a truck ram into a doorway, off a ledge, or into the lava.
- **Atmosphere**: dark brush-built level, distance fog, HDR + bloom on emissive lava/lights, point lights.
- **Procedural audio**: the core sound effects are synthesised at startup (no binary assets) into
  `assets/sounds/` and loaded by Bevy. The truck's engine, tyre screech and crash/ram hits are the
  one exception — recorded-sounding clips generated with ElevenLabs (see *Generating sounds*) and
  committed to the repo.

## Architecture (single crate, `src/`)

| Module | Responsibility |
|--------|----------------|
| `main.rs` | App, plugins, schedules, window, game-state wiring |
| `common.rs` | Shared components, resources, messages, tuning constants |
| `physics.rs` | Swept-AABB move-and-slide, step-up, ground probe, raycasts |
| `player.rs` | Spawn, mouse-look, Quake movement, cursor grab |
| `weapons.rs` | Inventory, firing (hitscan/projectile), switching, view-model |
| `vehicle.rs` | Drivable vehicles: moving-brush collider, driving physics, rider carry |
| `projectiles.rs` | Rockets, grenades, nails, enemy bolts |
| `combat.rs` | Damage, armor, explosions, death, gibs |
| `enemies.rs` | Monster spawning + AI state machine + attacks |
| `monster_model.rs` | Procedural skeletal monster rigs, textures, bone animation + death topple |
| `gallery.rs` | `QC_GALLERY=1` debug mode: render each monster solo and screenshot it |
| `itemshot.rs` | `QC_ITEMSHOT=1` debug mode: render each pickup solo and screenshot it |
| `pickups.rs` | Health, armor, ammo, weapons, key (every pickup is a little hand-built low-poly model) |
| `level.rs` | Themed material palettes, the `Build` level-authoring API, level registry + dispatch |
| `levels/` | One module per level (`level1`..`level8`) — each a `build(&mut Build)` map |
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

Run `QC_FPSLOG=1 cargo run` to log frame-time/fps to the console once a second (Bevy's frame
diagnostics) — handy for the big level-8 dam, the deliberate large-space stress test. Set
`vsync: false` in `config.ron` first to uncap the frame rate.

Run `QC_GALLERY=1 cargo run` to spawn each monster in turn, lit and centered, and save a close-up
`gallery_<n>_<kind>.png` of every model — handy for eyeballing the rigs and textures.

Run `QC_ITEMSHOT=1 cargo run` to render each pickup in turn, lit and centered, and save a close-up
`itemshot_<n>_<name>.png` of every model — handy for eyeballing the pickup props.

Run `QC_LEVELSHOT=1 cargo run` to build each level in turn and save `levelshot_<n>_<name>.png` from
the player's entry vantage — handy for eyeballing every level's geometry, textures and lighting.
`QC_LEVEL=<n> cargo run` forces the campaign to start on a specific level (0-indexed), and
`QC_EXIT_RUSH=1 cargo run` teleports the player onto each exit to fast-forward through the campaign
(validates level progression + weapon carry-over).
