# 32. Volumetric Slabs — Fog You Can Shoot a Hole In

> A wall of drifting murk hides whatever's behind it — until your rocket blows a glowing porthole clean through and the Knight inside lights up for half a second.

**The idea** — A new `Build` primitive: a *fog slab*, a stack of large soft-edged alpha quads that fill a doorway, a canal, or the void over a dam spillway with drifting volumetric haze. Monsters can stand inside it, fully occluded. When a rocket detonates against (or inside) a slab, or the lightning beam crosses it, it punches a **temporary glowing clear-disc** — a bright-rimmed hole that fades the fog to nothing in a growing circle, then heals shut over a second or two. For that window you see straight through, and anything lurking in there is suddenly, vividly lit.

**Why it's fresh** — Quake fog is decoration you walk through. Here it's *interactive cover* you can carve. The clear-disc isn't a particle gimmick; it's a hole in the volume that both you and the enemy's line-of-sight react to. Destructible atmosphere as a tactic is rare in any shooter.

**How it plays** — Murk becomes a thing you *clear*. Hear an Ogre grunt in the soup, rocket the haze, and for a beat you've got a lit target. Lightning sweeps a tracking slit you can rake across a slab. The disc heals, so you fight the fog continuously, spending splash to keep a sightline open — and monsters use the uncleared murk to close on you unseen.

**How it fits QUAKECLONE** — A `fog_slab(...)` call joins the `level.rs` Build API beside `hazard`/`light`, themed per level (the dam gorge, the Verdant Rot canals). The hole is driven by `projectiles.rs` rocket detonations and the `weapons.rs` lightning hitscan, with `physics.rs` raycasts placing the disc on the slab plane. Crucially, fog density gates **LoS in `enemies.rs`**: a monster only perceives you across cleared fog, so the murk genuinely hides both ways. Rendering leans on the existing HDR + bloom path so the disc rim glows.

**Build sketch** — Slab quads with a scrolling noise alpha; per-slab a small ring buffer of active discs (center, radius, age) fed to the fog shader as uniforms, each carving and fading. The honest hard part is the **two-way LoS coupling** — sampling fog density along the perception raycast cheaply enough that it stays a gameplay rule, not just a visual.

**Effort** — **M**. Main risk: the custom fog material and disc-carve shader fighting Bevy 0.19's transparency sort and bloom.
