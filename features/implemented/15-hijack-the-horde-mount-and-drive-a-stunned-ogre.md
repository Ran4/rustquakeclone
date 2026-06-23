# 15. Hijack the Horde — Mount and Drive a Stunned Ogre

> Whip-stagger an Ogre, vault onto its shoulders, and drive the bruiser itself as a living, chainsaw-revving battering ram — until it dies and bucks you off.

**The idea** — Crack the Whip across an Ogre to land a heavy stagger, then press **E** while standing on it to *mount* it. Now you're not in a truck — you're riding a monster. **W** drives it forward as a charging ram, **A/D** steers its lumbering gait, and **left mouse** fires its own chainsaw arm into anything ahead. You ride until the Ogre's health hits zero, at which point its death-topple animation literally throws you off.

**Why it's fresh** — Mountable enemies exist, but here the mount is the same procedural skeletal rig that was just fighting you — there's no separate "vehicle model." You commandeer a live actor mid-AI, its bone animator still running, and turn its own chainsaw attack into your weapon. It's the truck's moving-collider trick fused with the monster rig: a vehicle that bleeds, flinches, and dies.

**How it plays** — It's a risk-reward grab. The stagger window from a Whip crack is short, so you have to land the lash and close the gap before `pain` ticks back to zero and the Ogre shrugs you off. Once aboard you trade your full arsenal for one tool — a slow, heavy, splash-immune ram that pulps Grunts and shoulders Knights aside — while soaking hits *as* the Ogre's health pool. You're choosing the Ogre as a temporary tank, and timing your dismount before it dies under you in a bad spot.

**How it fits QUAKECLONE** — It leans directly on `vehicle.rs`: `ActiveVehicle`, the reserved collider slot rewritten each frame from the transform, the rebound/spin-on-wall integration, and the rider-carry step. The mount target is a standard `enemies.rs` monster, so its `en.pain` stagger gate, knockback, and Idle→Chase→Attack state already exist. `monster_model.rs` supplies the bone animator — we drive its walk-cycle and attack-swing roles from throttle and fire input instead of AI, and reuse the death topple as the eject. The chainsaw fires through `combat.rs` melee/splash. Engine and chainsaw audio reuse `audio_gen.rs` synthesis.

**Build sketch** — Add a "mountable" flag the Whip sets during stagger; on **E** near a flagged Ogre, suppress its AI, reparent control to the player, and feed the existing vehicle drive loop a monster-tuned accel/turn profile that writes the Ogre's collider slot. Throttle and fire map onto bone roles instead of WASD-on-a-truck; HP routes through the rider, and the death topple triggers the dismount. The hard part is the seam between the AI animator and player-driven animation — the rig must read as *driven* (walk speed from throttle, chainsaw swing on click) without the AI re-grabbing it or the bones snapping between pose sources.

**Effort** — **M**. Main risk: the animator hand-off — sharing `monster_model.rs` bone control between AI and player input without pose pops or AI re-acquiring the body.
