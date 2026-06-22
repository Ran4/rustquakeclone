# 16. Spreading Fire — Lava That Learns to Climb

> Today's lava is a static box that sits and hurts; this turns it into a living thing that catches, climbs, and chases you across the level.

**The idea** — Replace the inert `LavaVolumes` list with a coarse grid of hazard *cells* per level, each tagged flammable (wood catwalks, sludge skins, egg-sacs) or inert (stone, metal). Today fire only exists where a level author painted it. Here, any ignition source — a grenade splash, a rocket's fireball, a burning gib skidding to rest, a tongue of base lava — seeds a fire that *propagates*: each tick, lit cells roll a chance to ignite flammable neighbours, including the cell directly above, so flame creeps up a wooden ramp and races along a sludge canal. Cells burn down over time and gutter out, leaving scorch.

**Why it's fresh** — FPS fire is almost always decorative or a fixed scripted trigger. A real spreading cellular automaton that *the player starts and steers* — and that monsters and the truck can start too — is a systemic hazard you reason about, not a cutscene. Fire that climbs vertically through QUAKECLONE's catwalk-and-shaft geometry is the twist: the danger comes from below and rises to meet you.

**How it plays** — A grenade into a wooden vault no longer just dings a Knight; it lights the floor, and you watch the line of flame decide who it reaches first. You can burn a path clear, herd Ogres into a wall of fire, or panic when your own rocket-jump ignites the catwalk you need to cross. New skills: reading spread direction, timing a sprint through a closing gap, choosing *not* to shoot flammable cover.

**How it fits QUAKECLONE** — Extends `LavaVolumes`/`lava_damage` in `gamestate.rs` (overlap-test the player AABB against *lit* cells instead of static boxes), `animate_lava`'s emissive pulse per cell, the `hazard` and `brush` authoring in `level.rs`'s Build API (a `flammable` flag), `effects.rs` fireballs/gibs as ignition seeds, `combat.rs` splash as a source, and `audio_gen.rs` for a crackle loop.

**Build sketch** — Voxelise hazard regions into a fixed grid at level build; store cell state (inert/flammable/lit/spent + fuel) in a resource. A throttled system steps the automaton a few times a second, igniting neighbours by probability with an upward bias. Bridge it to existing damage and particle spawns. The hard part is performance and feel: keep the grid coarse, cap active cells, and tune spread rate so it threatens without becoming an unstoppable screen-filler — plus syncing dozens of emissive cell materials cheaply.

**Effort** — **M**. Main risk: a runaway grid that either fizzles instantly or torches the whole map, so spread-rate and fuel tuning (and a hard active-cell cap) carry the design.
