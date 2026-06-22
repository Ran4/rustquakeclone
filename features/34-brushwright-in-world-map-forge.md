# 34. Brushwright — In-World Map Forge

> Freeze the level, raise a glowing grid cursor, and stamp real brushes into the world with your own hands — the same brushes the campaign is carved from.

**The idea** — A first-person, pause-time editor that lives *inside* the running game. Tap the forge key and the simulation halts; a translucent snap-to-grid box hovers where you aim. Drag two corners, pick a theme material from a radial, and commit — a real `Build::solid` brush drops in: a world-aligned-UV textured mesh *and* an AABB collider, indistinguishable from the hand-authored map. Delete by aiming at a brush and yanking it back out. Unfreeze and your new wall is solid, fogged and lit like everything else.

**Why it's fresh** — Most in-game editors are a separate mode bolted onto a level format. Brushwright has no format: it calls the exact authoring path `levels/` modules use, so there's nothing to serialize-then-reload. The map you're standing in *is* the editor's document, edited live. You sculpt cover mid-firefight-prep, not in a detached tool.

**How it plays** — It turns the player into a level designer at combat tempo. Pinned down? Freeze, stamp a chest-high brush for cover, step-up onto it, unfreeze and peek. Need a rocket-jump perch over a lava channel? Build the ledge. Want to dam the truck's path on level 8 and watch it bounce off your wall? Stamp it and floor the throttle. New skills emerge: reading sightlines, judging step-up heights, grid economy.

**How it fits QUAKECLONE** — It leans on `level.rs`'s `Build` API (`solid`, `deco`, `slab`, themed `tex`/`mat`) and the active `Theme` palette so every stamp inherits world-aligned UVs and fog. The cursor's placement uses `physics.rs` raycasts against `WorldColliders.solids`; commits mutate that same live list, so swept-AABB collision, monster pathing, hitscan and LoS pick it up instantly — the moving-brush trick `vehicle.rs` already proves works. `gamestate.rs` owns the freeze/unfreeze toggle; `audio_gen.rs` synthesises a stamp *chunk* and a delete *whoosh*.

**Build sketch** — A `Forge` resource holds cursor transform, grid step and pending corner. A pause schedule runs only cursor/preview systems. The honest hard part: live collider mutation. Today brushes spawn once at `setup_level` into `colliders.solids.clear()`-then-fill; Brushwright must append, track which `Aabb` belongs to which entity for deletes, and keep the index stable so the vehicle's reserved slot doesn't drift. A `ForgeBrush` component mapping entity↔collider-index, with deletes swapping in a degenerate AABB rather than reindexing, keeps it safe.

**Effort** — **M.** Main risk: collider-list bookkeeping colliding with the vehicle's reserved-slot invariant — get index stability wrong and the truck eats a phantom wall.
