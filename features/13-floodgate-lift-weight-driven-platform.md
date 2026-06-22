# 13. Floodgate Lift — A Weight-Driven Platform You Fight On

> A counterweighted freight lift whose height is set by physical weight — shoot out the holding pin to plummet, or stack gibbed corpses on the pan to ride yourself and a clinging Knight up the dam shaft.

**The idea** — A heavy steel deck hangs in a vertical dam shaft on a cable, balanced against an iron counterweight. It is not a triggered elevator: its height is driven by *weight on the deck versus the counterweight*. You weigh something. Each monster aboard weighs something. Every gibbed corpse you pile on the pan weighs something. Heavier than the counterweight, the deck sinks; lighter, it rises; balanced, it hangs. A holding pin can lock it — shoot the pin out and physics takes over instantly.

**Why it's fresh** — Quake elevators are scripted lifts that ignore what stands on them. This one's vertical position is the *running sum of the masses riding it*, computed every frame. Corpses become fuel; the death you cause is the elevator you ride. Combat and traversal stop being separate verbs.

**How it plays** — You need to reach a high intake gallery. The deck won't lift you alone. So you bait Knights onto the pan, gib them, and let the carcasses tip the balance upward — or shoot the pin and ride a controlled plummet past a hazard, braking by dumping weight (kick a corpse off the edge) at the right instant. A live Knight riding up with you is both ballast and a knife fight in a moving box with no rails.

**How it fits QUAKECLONE** — It is a `vehicle.rs`-style moving collider: it reserves one slot in `WorldColliders` and rewrites it each frame (the door/truck moving-brush trick), so `physics.rs` swept-AABB, monster pathing, hitscan and LoS treat it as solid for free. The per-frame rider "carry" step already exists. Mass comes from `monster_model.rs` rigs and `combat.rs` gibs; the pin is a hitscan target in `weapons.rs`; authored via the `level.rs` Build API into the level-8 dam shaft; clanks/cable groans via `audio_gen.rs`.

**Build sketch** — Extend the vehicle body with a 1-D vertical solver: sum the masses of entities resting on the deck plus loose gib bodies, compare to the counterweight, integrate velocity along the shaft, clamp at travel stops. Reuse carry for riders; gibs need a lightweight "settled on deck" mass tag. The hard part is the corpse/gib bookkeeping: counting only bodies actually *on the pan* (not grazing the edge), and keeping the balance stable instead of oscillating — a little damping and a snap-to-rest band.

**Effort** — **M**. Main risk: the weight loop feeling fiddly or jittery rather than tactile; mitigate with strong audio/visual feedback (cable strain, counterweight motion) and generous balance hysteresis.
