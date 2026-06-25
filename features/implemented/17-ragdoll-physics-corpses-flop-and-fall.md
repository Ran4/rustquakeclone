# 17. Ragdoll Physics — Corpses Flop and Fall

> Every clean kill drops a body that *falls over for real* — its skeleton goes
> limp and topples under gravity, drapes over whatever it lands on, and is still
> solid enough to bump, stand on, and shove.

**The idea** — When a heavy monster dies cleanly (the topple path in `combat.rs`,
not the overkill-gib path), its body stops being a canned 90°-rotate-and-sink
animation. Instead the *same skeletal rig that was just fighting you* is handed to a
small Verlet/PBD ragdoll: one particle per bone joint, the bone lengths as distance
constraints, light bend-stiffness so the trunk stays semi-rigid while the limbs flop,
and per-particle world collision so the body genuinely drapes over floors, ledges and
lava lips instead of clipping through them. To the player and the monsters it's also
*solid* — a single body-sized box tracks the flopped pose so you can bump it, stand on
the pile, and it blocks shots and line-of-sight.

**Why it's fresh** — FPS corpses are almost always either a one-shot canned topple or
pure decoration you walk through. Here the death is a physics event: the body falls
the way it was killed (a side rocket tips it sideways, a face-on blow drops it
forward), arms and legs trail and fold, and it settles into a shape you couldn't have
predicted. Knock it afterwards — splash, a Whip fling, a truck ram — and the whole
ragdoll lurches and tumbles, then drapes again.

**How it plays** — Kills read as physical, not bookkeeping. A body folds over a
railing, slumps down a stairwell, or piles onto another corpse. Because it stays solid
for a few seconds you can still use it — crouch-peek a downed Ogre in a chokepoint,
boot a body off a ledge or into the lava (where it dissolves) — but the point is the
*fall*, not a sliding brick of cover.

**How it fits QUAKECLONE** — The skeleton is the existing parented bone hierarchy
(`monster_model.rs`); the ragdoll keeps that hierarchy and each frame overwrites every
bone's local `Transform` from the solved pose, so the rigged, textured meshes follow
for free (`corpse.rs::reconstruct`). World collision reuses `physics::depenetrate`
(point-vs-solids push-out) per particle. Going *solid to others* reuses the truck's
moving-brush trick — a slot in `WorldColliders.solids` rewritten every frame from the
body box — handed out from a small build-time pool (`CORPSE_CAP`) via a runtime
free-list. Knockback feeds in through the normal `DamageEvent` path: the corpse keeps
its `Knockback`, consumed each frame as an upper-body-biased impulse. The
`Dying` sink/despawn tail stays in `monster_model::animate_death`; at
`CORPSE_SINK_BEGINS` the ragdoll freezes and the held pose sinks into the floor.

**Build sketch** — On clean death, snapshot every live (non-severed) bone's world
pose into a particle, wire parent↔child distance constraints plus skip-one bend
constraints (stiff along the spine, floppy across limbs), seed a fall-over velocity
from the kill direction (upper body thrown, feet planted), and claim a collision slot
if one's free. Each frame: Verlet-integrate under gravity, satisfy the constraints,
`depenetrate` each particle out of the world, rewrite the tracking box, and reconstruct
the bone transforms parents-first. Past the slot cap a kill still ragdolls (the visual
drape needs no slot) — it just isn't solid to others until a slot frees.

**Effort** — **M.** Main risks (handled): a corpse box shoving the *player* into
geometry (tight box-size clamp, grounded sleep-snap, the body resolves before the
player each frame, and the slot goes inert on any frame it would overlap the player);
and reconstructing per-bone rotations from particle positions without fighting Bevy's
transform propagation (compute world transforms ourselves in topological order).
