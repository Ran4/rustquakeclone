# 33. Slipgate Bleed — The Next Dimension Leaks Backward

> The exit slipgate sweats the *next* level's wrong-colored fog a few meters into the current room, so a transition becomes a gradient you walk through instead of a hard cut.

**The idea** — Every level resolves a theme palette — a fog color, near/far falloff, ambient and clear color (`level.rs`, `theme_spec`). Today level N's fog is uniform and level N+1's is unseen until you load it. Slipgate Bleed peeks ahead: when a map is built, it also resolves `LEVEL_META[idx+1]`'s theme and stashes its fog tint. Near the exit slipgate, the *next* dimension's fog physically intrudes — Frostspire's pale blue mist curling into the Doomed dungeon's violet murk, the Hive's sick green seeping toward the Salt Wraith's sky-blue. The wrongness is the point: a color that doesn't belong to this room, pooling around the gate, thickest at the threshold and thinning to nothing a few meters back.

**Why it's fresh** — Level transitions are almost universally a fade-to-black or a load screen. Here the seam is *diegetic and spatial*: the boundary between two worlds is a place in the room you can stand near, lean into, or back away from. The campaign stops being eight sealed boxes and becomes a continuum where each dimension faintly bleeds into the last.

**How it plays** — Long before you find the exit you start noticing the air is wrong in one corner — an alien tint that tells you the slipgate is near and hints at the *flavor* of what's coming (cold, toxic, void-black). It becomes a soft wayfinding cue and a tease. Standing in the bleed zone, your view-model and the world tint toward the next palette; step back and your home dimension reclaims you. Walking the gradient *is* the transition.

**How it fits QUAKECLONE** — It extends `level.rs`'s `LevelStyle`/`apply_fog` and `theme_spec`, reuses the exit point already stored in `plan.exit` and ranged by `gamestate.rs`'s `exit_system`, and rides the player camera's existing `DistanceFog` + HDR/bloom stack from `player.rs`. `LEVEL_META` lookahead is free.

**Build sketch** — Resolve next-theme fog at build time; each frame, blend the camera's fog color/falloff by the player's distance to `plan.exit`. The honest hard part: Bevy `DistanceFog` is one global per-camera value, so a *localized* pool isn't native — you fake it by driving the global blend from proximity, accepting that the whole view tints (not just the corner) unless you add a small additive emissive fog volume around the gate.

**Effort** — S–M. Main risk: the global-fog fake reads as the whole room recoloring rather than a contained leak; tuning the proximity curve so it feels like a seam, not a strobe.
