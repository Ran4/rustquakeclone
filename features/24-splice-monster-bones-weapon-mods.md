# 24. Splice — Looted Monster Bones Become Weapon Mods

> Blow the chainsaw arm off an Ogre, snatch the still-twitching bone, and graft its bite onto your shotgun.

**The idea** — When a monster gibs, the signature part you destroyed it with drops as a glowing **bone pickup**: the Ogre's chainsaw-arm, the Scrag's spit-gland, the Knight's sword, the Enforcer's bolt-emitter. Walk over it to bank it, then **slot it into a weapon** to graft that creature's behaviour onto your gun. Chainsaw-arm on the shotgun adds a melee bite-and-stagger on contact; spit-gland on the nailgun makes every fifth nail a homing acid glob; Knight sword on the Whip turns the fling into a cleaving lunge. One bone per weapon — splicing is a tradeoff, not a stack.

**Why it's fresh** — Loot shooters drop stat-sticks; this drops *behaviours pulled off the things you killed*, and it reuses the actual procedural rig parts as the currency. The bone you loot is literally the mesh that was animating on the monster a second ago. Your loadout becomes a trophy wall of how you've been fighting.

**How it plays** — A new target-priority skill emerges: gibbing the *arm* versus the *body* now matters, so you aim limbs to farm the mod you want. Splices are spent on death-or-glory builds — do you bolt the rare Death-Knight bone onto the rocket launcher now, or hoard it for the boss? Because bones drop mid-fight, you make hot loadout calls between waves, and carry the best splices forward between levels.

**How it fits QUAKECLONE** — Leans hard on `monster_model.rs`: each bone is already a named, parented entity (chainsaw, `WingL/R`, sword, spit-gland), so the drop *is* that detached part with its texture intact. `combat.rs` overkill gibbing and `effects.rs::spawn_gibs` decide when/what drops; `pickups.rs` adds a bone pickup kind and model; the slot lives in the `Weapon` inventory in `common.rs` and is consumed by firing in `weapons.rs`/`projectiles.rs`. Splices ride the existing `run.carry` / `carry_inventory` persistence in `gamestate.rs`, with new SFX synthesised in `audio_gen.rs`.

**Build sketch** — Tag each rig's signature bone; on overkill, despawn it from the corpse and spawn a matching pickup. A small `Splice` enum hangs off each weapon and the firing code branches on it. The honest hard part is *behaviour composition* — every splice has to layer cleanly onto seven existing fire paths without a combinatorial mess, so route them through a few shared hooks (on-hit, on-spawn-projectile, on-melee) rather than per-pair special cases.

**Effort** — **M**, trending L if you splice all seven weapons. Main risk: balance and feel — making each graft read instantly and not turn the sandbox into soup.
