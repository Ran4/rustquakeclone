# 26. Overclock Gauntlet — The Truck Is the Run Clock

> The flatbed follows you through every dimension on a draining overclock core, and its engine-rev pitch is your run clock — bank charge at each exit while mounted to keep the run alive.

**The idea** — The level-8 truck is promoted to a campaign-long companion. It spawns on level 1 and persists, dimension to dimension, carrying a single resource: an **overclock core** that drains in real time the whole run. The core is the clock. When it hits zero the run ends. The only way to refill it is to reach the exit slipgate **while mounted in the truck** and bank whatever charge you have left — each exit crossed-on-wheels tops the core back up, plus a bonus scaled to how much you arrived with. Cross on foot and you bank nothing; the clock keeps draining into the next dimension.

**Why it's fresh** — Speedrun timers are usually invisible UI. Here the timer is a physical, drivable, rammable object you have to babysit across the whole campaign — a roguelike "fuel run" fused with a movement shooter. The genius twist: the truck's existing **adaptive engine-rev audio** *is* the meter. Low core = a sick, dragging idle; healthy core = a bright redline vroom. You hear your run dying before you read it.

**How it plays** — Every dimension becomes a route-planning problem: ditch the truck to squeeze through a tight key vault on foot, then sprint back before the core bleeds out — or muscle the flatbed through the whole level and ram a path clear. Driving conserves nothing by itself; only *banking at the exit, mounted* refills you. So you're constantly weighing a clean fast on-wheels exit against detouring for the Silver Key. Ramming monsters and rocket-jumping the truck up ledges become time-saving plays, not stunts.

**How it fits QUAKECLONE** — Leans hard on `vehicle.rs` (the moving-collider truck, rider carry, rev/tyre audio scrub) and its `audio_gen.rs`/ElevenLabs loops, now pitch-driven by core charge. `gamestate.rs` owns the core resource, the drain, and the exit-bank check folded into the existing exit→next-level handoff (alongside weapon/ammo carry-over). `level.rs` Build API tags a truck spawn pad per level; `hud.rs` shows the core bar. `physics.rs` swept-AABB already lets the truck thread or wreck through any brush map unchanged.

**Build sketch** — The honest hard part isn't the timer; it's making the truck *survivable to relocate* across eight maps authored for it. Each level needs a guaranteed truck-wide path to its exit (or a sanctioned on-foot fallback that still banks). Spawn the persistent truck once, carry its state through level transitions like inventory, re-register its collider slot on each load, and route the rev pitch off the core. Tuning the drain rate so failure feels fair, not arbitrary, is the real labor.

**Effort** — **L.** Main risk: retrofitting all eight hand-built brush maps with truck-passable exit routes without gutting their pacing.
