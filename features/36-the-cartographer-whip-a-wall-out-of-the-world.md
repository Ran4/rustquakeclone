# 36. The Cartographer — Whip a Wall Out of the World

> Lash a wall, yank it free, and swing the chunk of level you just stole — a stepping stone, a ramp, or a wrecking ball on a leash.

**The idea** — A charged alt-fire on the Whip latches onto a flagged brush, tears it out of the static world, and turns it into a tethered *moving* collider that obeys the same physics as the truck. The freed chunk hangs on your whip-leash: reel it in, swing it in an arc, drop it as a platform, jam it into a doorway as a ramp, or let go at speed and fling it as a brick the size of a wall. One brush at a time, and only "loose" ones — the level is mostly load-bearing.

**Why it's fresh** — Destructible geometry is common; *portable, re-purposable* geometry is not. You don't break the wall, you **repossess** it. The level stops being scenery and becomes inventory. Combined with wall-jump and rocket-jump, the player gains a third verb for defeating a gap: *bring your own floor.*

**How it plays** — You hit a chasm with no jump line. Instead of dying, you whip a slab off the cliff face, swing it under your feet, and stand on your own loot mid-air. Mid-fight you tear a pillar loose and pendulum it through a pack of Knights — a momentum-scaled pulping just like a truck ram. The skill ceiling is the leash: a heavy brush swung wide will yank *you* off a ledge if you mistime the release. New decisions every room — climb it, bridge it, or throw it.

**How it fits QUAKECLONE** — It is the truck trick generalised. `vehicle.rs` already proves a moving `Aabb` rewritten each frame from a transform, carrying velocity + yaw_rate, bouncing off `physics.rs` swept-AABB normals and ramming monsters; a freed brush is that minus the wheels. `level.rs`'s `Build::solid` already pushes each brush as both a world-aligned-UV mesh *and* a `WorldColliders.solids` entry — so "free a brush" is detaching one slot (the door pattern, inverted) and re-driving it. The leash physics, knockback and `melee_strike` raycast live in `weapons.rs`; the clang/screech reuse `vehicle.rs` impact audio. `gamestate.rs` flags which brushes are loose per level.

**Build sketch** — Add a `loose: bool` to brushes in the `Build` API and pair each loose brush's mesh entity to its collider index. Alt-fire raycasts, claims that index, and promotes it to a vehicle-style mover with a spring tether to the muzzle. The hard part is the swept solver assuming colliders are static: a fast-swung brush must itself sweep against the player and monsters, not just be swept *into*. Cap it to one active brush and clamp size to keep the solver honest.

**Effort** — **L.** Main risk: mover-vs-mover collision (a swinging brush hitting the player who is also moving) stresses the swept-AABB solver's static-world assumption; needs careful ordering or it tunnels.
