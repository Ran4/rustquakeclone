# 08. Conductive Lightning — Arcs That Find Their Path

> Stop spraying the beam across a mob — hit one wet, packed, or armored monster and watch the bolt *find the rest itself.*

**The idea** — The Lightning Gun's continuous hitscan stays a single beam to your first target, but on impact it spawns a **chain-conduction pass**: a breadth-first search hops the charge from victim to nearby victim, branching outward through the cluster. Each hop costs distance-falloff damage, so the group jolt fans out from a hot first strike to weaker tips. Enemies don't conduct equally — proximity, **metal armor** (Grunt/Enforcer flak, the Knights' plate and swords), and **standing in water or toxic sludge** all make a monster a better relay, widening the arc tree. A puddle full of Scrags or a sludge canal of crowding Knights becomes one connected circuit.

**Why it's fresh** — Chain lightning exists in RPGs, but here it's grounded in the *brush world's own materials and the swept collider's spatial truth*, not a scripted spell. Conductivity is read off the level geometry the player already fights on — the toxic-sludge canals of the Verdant Rot, the frozen lake of Frostspire, a flooded dam gallery — so the same hazard that hurts you becomes a weapon multiplier.

**How it plays** — You stop hosing and start *positioning*. Bunch enemies, herd them into a sludge pool or onto wet metal, then tap the beam at the nearest one and let conduction do the sweep. New skills emerge: shooting the *wet* target over the dry one, baiting a Knight pack onto conductive footing, kiting along a canal edge. The beam still rewards aim; the chain rewards reading the room.

**How it fits QUAKECLONE** — Leans on `weapons.rs` lightning hitscan, `physics.rs` raycasts + swept-AABB for hop line-of-sight/range, `enemies.rs` for the live monster set and a per-kind conductivity tag, `combat.rs` for falloff damage/knockback, `level.rs` hazard styling to mark water/sludge volumes, `effects.rs` + dynamic lights for branching arc visuals, and `audio_gen.rs` for a crackling multi-hop zap.

**Build sketch** — Tag each monster kind with a conductivity weight; mark hazard brushes as conductive in the `Build` API. On beam impact, run a bounded BFS over nearby enemies, each edge validated by a `physics.rs` raycast (no arcing through walls), accumulating falloff damage per depth and boosting edge weight for armored/standing-in-fluid relays. Honest hard part: tuning the branch budget and falloff so it's a satisfying group nuke without trivializing dense rooms — and drawing readable arc geometry between bones.

**Effort** — **M.** Main risk is balance: conduction can turn the Lightning Gun into a crowd-clear win button, so the falloff curve, hop cap, and cell cost need careful tuning against the existing levels.
