# 31. Damage Decay — Wounds Carved Into the Rig's Facets

> Every hit caves a dent into the exact bone you struck, so a monster wears its wounds in its own facets and gibs when those facets finally invert.

**The idea** — Monsters take *visible* damage. Each hit doesn't just dock a health number and flash red — it deforms the struck bone's mesh: facets around the impact cave inward and the per-vertex jitter cranks up locally, so a clean rig ruins into dented armor, crumpled plate and mangled limbs as you chew it down. Pour fire into one shoulder and that shoulder caves first. When a bone's facets are pushed in far enough to fully invert, that limb gibs off and the monster ragdolls the rest of the way.

**Why it's fresh** — Most games swap to a pre-authored "damaged" mesh or bolt on decals. Here there *are* no mesh assets — the rig is procedural, so the wounds are too: real geometry, carved live, unique to where you actually hit. No two corpses dent the same.

**How it plays** — You can read an enemy's remaining health off its silhouette instead of a bar. A half-gibbed Ogre with one arm stove in tells you to finish it or save the shell. It rewards aimed fire — concentrate a nailgun stream on one limb to lop it, or spread damage for a slower, uglier kill — and makes the Whip's knockback land visibly: shove a caved-in Knight and watch the dents wobble.

**How it fits QUAKECLONE** — It extends `monster_model.rs` directly: the same `make_mesh`/`jitter` path (deterministic `vhash` keyed on vertex bits) gains a damage term, and the per-frame procedural animator already touching every bone is the natural place to re-deform. It hangs off the existing `DamageEvent` flow in `combat.rs`, reuses the per-enemy `flash`/`pain` fields, and routes limb-gibbing through the same `spawn_gibs` + overkill path in `check_deaths`.

**Build sketch** — Hitscan/projectile hits already know an impact point; map it to the nearest bone entity and accumulate a per-bone "wound" (impulse direction + magnitude). The animator rewrites that bone's mesh, displacing vertices within a falloff radius of the wound inward along the surface, scaling jitter by accumulated damage. The honest hard part: re-deforming meshes every frame for many bones is allocation- and upload-heavy, so wounds must be quantised, dirtied-on-change, and shared per damage-bucket so most bones reuse a cached handle.

**Effort** — M. Main risk: per-frame mesh re-upload cost and keeping coincident-vertex welds watertight as facets invert (else holes/flicker before the gib).
