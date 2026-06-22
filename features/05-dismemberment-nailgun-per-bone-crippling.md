# 05. Dismemberment Nailgun — Per-Bone Crippling

> The Nailgun stops shooting at monsters and starts shooting at their limbs — sever the sword-arm to disarm a Knight, cripple a leg to outrun it, or pop the head for a free kill.

**The idea** — Today a nail strikes one body-sized box and dumps damage into the monster's single `Health` pool. This feature makes the Nailgun *limb-aware*: every monster carries a handful of per-bone hurtboxes (head, torso, each arm, each leg) that each track their own damage. Pour enough nails into the sword-arm and it *severs* — the bone detaches, the attack it powered is neutered. Blow a leg and the walk cycle halves; pop the head and it's an instant kill. Cripples persist and stack, so a Knight can end up one-legged *and* disarmed, lurching but harmless.

**Why it's fresh** — Most shooters fake dismemberment with a death-only gib swap. Here the rig *is* the entity hierarchy already, so a limb can leave mid-fight and the monster keeps living, walking and (badly) fighting around the hole. It reframes the precise, low-damage Nailgun as a *surgical* weapon: not "deal damage faster" but "spend nails where they cripple most" — an ammo-economy puzzle no other weapon offers.

**How it plays** — The Nailgun becomes the thinking-player's gun. Facing a charging Knight you can either tank the burst into its chest or thread nails into the sword-arm and walk away from a declawed husk. A leg-shot Scrag still flies but lists; a legless Ogre can't close distance so its chainsaw never lands. New skill: leading a *specific bone* on a moving, animating rig instead of centre-of-mass.

**How it fits QUAKECLONE** — It leans hardest on `monster_model.rs`: the parented `BoneSpec`/`Bone` hierarchy gives named, transform-tracked joints for free, and the `ChildOf` structure means detaching a bone is a reparent + ragdoll-toss. `projectiles.rs` already raycasts each nail against a `target_list` of per-entity AABBs and emits one `DamageEvent` — we widen that list to per-bone boxes and route damage to a new limb-state component instead of straight to `Health`. `enemies.rs` reads cripple flags to gate attacks and scale walk speed; `combat.rs`/`effects.rs` supply the blood, severed-limb gibs and `Sound`-driven crunch via `audio_gen.rs`. `QC_GALLERY` validates per-bone hitboxes visually.

**Build sketch** — Tag each `BoneSpec` with a hurtbox group and HP; each frame, rebuild the nail target list from live bone `GlobalTransform`s. On a group's HP hitting zero, detach that bone subtree, spawn a tumbling gib, and set a cripple flag the AI honours. The honest hard part is *cheap, correct* per-bone hit tests on dozens of animating joints without ballooning the raycast loop, plus AI that degrades gracefully (a one-armed Ogre must still path and idle, not deadlock).

**Effort** — **M.** Main risk: per-frame per-bone hurtbox cost and tuning thresholds so crippling feels deliberate, not random.
