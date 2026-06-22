# 22. The Tide Engine — A Rising Lethal Sea

> The level's lava sea is no longer a dead floor stain — it breathes, rising and falling each frame so the same trench is a shortcut at low tide and a death pool at high.

**The idea** — One hazard volume per level becomes a *tide*: a horizontal slab whose top face is driven by a sine (plus configurable swell/period/phase) every frame. Below its surface the slab is a solid, rideable floor; the lethal DoT only bites the player or a monster whose feet are below the live surface line. So the "sea" is simultaneously a moving elevator, a timed gate, and a burning hazard — depending entirely on where the surface is *this* instant relative to *your* feet.

**Why it's fresh** — Quake lava is a static plane you either avoid or die in. Here the lethal line moves, and crucially the surface is *solid up to that line*, so the sea physically lifts you. Rising lava that carries you upward — and that you can out-climb, out-time, or surf — isn't a thing the genre does. It turns a flat hazard into a vertical traversal puzzle with a heartbeat.

**How it plays** — You watch the swell. A low tide opens a trench crossing or exposes a ledge; mistime it and the surface climbs past your shins and starts ticking. Ride a rising slab up a shaft you couldn't otherwise reach (the tide *is* the elevator). Bunny-hop the crests, wall-jump off a pillar as the pool swells under you, or rocket-jump to buy height when the tide is winning. Monsters dropped into a trench cook on the same schedule you do.

**How it fits QUAKECLONE** — It fuses two existing systems. The hazard is already an `Aabb` volume overlap-tested against the player box in `gamestate.rs` (DoT every 0.3s, `style.hazard_dot`, `hazard_flash`); the moving-solid + rider lift is exactly `vehicle.rs`'s trick — reserve one slot in `WorldColliders`, rewrite its top each frame, and carry riders via a `ride()`-style delta (reused for player *and* monsters). It leans on `physics.rs` swept-AABB step-up, the `level.rs` Build API (a `b.tide(...)` authoring call), and `audio_gen.rs` for a synthesised swell/lap loop scrubbed by tide height.

**Build sketch** — Add a `Tide` component owning a base Y, amplitude, period; per frame compute the surface, rewrite the reserved collider's `max.y`, and feed a `last_delta` so the carry step lifts riders. Split the hazard test: solid *below* surface, DoT *above feet*. The hard part is the seam — a rider resting exactly on a rising surface can freeze the swept solver (a known grazing failure), so the surface must lead the collider slightly and the carry must nudge riders, not snap them.

**Effort** — **M.** Main risk: the rest-on-rising-surface grazing freeze and DoT/solid feeling fair at the moving threshold.
