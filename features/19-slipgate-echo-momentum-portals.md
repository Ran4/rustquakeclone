# 19. Slipgate Echo — Momentum-Preserving Portals

> Two slipgate mouths stitched into one space: dive in carrying speed, fall out the far side carrying it rotated into the new frame — and your shots and your enemies' sightlines travel through with you.

**The idea** — A new `Build` primitive, `portal(a, b)`, drops a pair of glowing slipgate frames into a level. Touch one mouth and you are teleported to its twin, your position re-expressed in the exit frame and — the crux — your *velocity vector rotated by the same delta-yaw*. Walk in slow, walk out slow; bunny-hop in at full air-strafe cap, blast out the other mouth at full speed pointed wherever the exit faces. Hitscan rays and enemy line-of-sight that strike a mouth are re-cast from the twin, so you can shotgun a Grunt that's nowhere near you, and a Scrag can see (and spit at) you through the link.

**Why it's fresh** — Quake's slipgates only ever loaded the next map. Plenty of shooters teleport you; almost none preserve and *re-orient* momentum the way a real portal must, and fewer still forward weapons fire and AI perception through the seam. The result is genuinely non-Euclidean level space built from the same brushes you already have.

**How it plays** — Portals become movement tools, not set dressing. You learn to feed a hop into a mouth aimed to spit you across a lava gap, to rocket-jump into one and exit launched sideways, to peek-shoot a vault through a portal you can't walk to yet. Monsters wander through them; a rammed Knight can be punched into one and reappear behind you.

**How it fits QUAKECLONE** — It leans on `physics.rs`: the `move_and_slide` swept result detects the mouth-crossing, and `raycast_world`/`ray_aabb` are re-invoked from the twin for hitscan and LoS. The mouths reserve fixed slots in `WorldColliders.solids` like `vehicle.rs` doors do. `level.rs` gains the `portal` Build verb; `audio_gen.rs` synthesises a whoosh; `effects.rs` paints the swirl and exit muzzle-flash; `gamestate.rs` can gate a portal behind the Silver Key.

**Build sketch** — A trigger AABB per mouth, a stored frame transform per pair, and a delta-transform that maps point and velocity from one frame to the other. The hard part is recursion and double-teleport: clamp ray-forwarding to one bounce, debounce a body that straddles a mouth so it doesn't ping-pong, and make sure the swept solver doesn't re-collide with the mouth it just exited.

**Effort** — M. Main risk: the velocity/orientation re-frame interacting badly with `move_and_slide` step-up and the `vehicle.rs` rider-carry, plus jitter when a body hugs a seam.
