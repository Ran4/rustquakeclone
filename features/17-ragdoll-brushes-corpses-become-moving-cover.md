# 17. Ragdoll Brushes — Corpses Become Moving Cover

> Every clean kill leaves a body that's actually *there* — a shovable, shootable brick of meat you can push into a doorway, dam a sludge channel with, or boot into the lava.

**The idea** — When a heavy monster dies cleanly (the topple path in `combat.rs`, not the overkill-gib path), its corpse is promoted from a decorative prop into a live moving-collider slot. For the few seconds it lingers, the body is *solid*: it blocks hitscan and projectiles, occludes line-of-sight, plugs a doorway, and slides when something pushes it. A rocket splash, a truck ram, or a Whip fling gives it a velocity and it skids, tumbles down stairs, drops off a ledge, or sinks into lava and is gone.

**Why it's fresh** — FPS corpses are almost always pure decoration that you walk straight through. Here the body is the same class of object as the truck and the sliding doors — a first-class brush in the world. Dead enemies become terrain you author *during* the firefight: cover that didn't exist a second ago, a barricade, a stepping-stone, a thing to ram.

**How it plays** — Kills stop being only subtractive. A downed Ogre in a chokepoint is a sandbag you crouch-peek over; rocket it again and you've cleared your own lane. On level 8 you can ram a Knight off the dam crest, or pile two bodies to dam the toxic canal on level 5 and walk the gap dry. New skills: corpse-shoving with splash and the Whip, ledge-positioning your kills, and reading which doorway a body just sealed shut.

**How it fits QUAKECLONE** — It reuses the truck's whole trick from `vehicle.rs`: a reserved slot in `WorldColliders.solids` rewritten every frame from a transform, swept through `move_and_slide` in `physics.rs` so it bounces, rests, and pushes for free. It hooks the `Dying` corpse path in `combat.rs`/`monster_model.rs`, takes velocity from `combat.rs` explosion knockback, and despawns through the existing topple-sink timer. Hazards in `gamestate.rs` dissolve corpses dropped in lava; a faint settle/scrape cue comes from `audio_gen.rs`.

**Build sketch** — On clean death, grab a free collider slot, size an AABB to the rig's footprint, and integrate it under gravity/restitution each frame, feeding knockback impulses in. The honest hard part: slots are currently reserved at *build* time for stable door/truck indices, so this needs a small runtime free-list (reuse the disabled-door degenerate-AABB sentinel) and a cap so a massacre can't spawn dozens of swept boxes and tank the frame.

**Effort** — **M.** Main risk: corpse colliders pushing the *player* into geometry or wedging doors; needs a tight body count cap and careful rest-state handling (the truck's "don't sit exactly on the floor" gotcha applies).
