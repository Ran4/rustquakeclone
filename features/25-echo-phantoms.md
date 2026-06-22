# 25. Echo Phantoms — Your Last Death Haunts the Run

> Every level you enter is already occupied by a solid ghost of your last attempt — walking your old route, firing your old shots, and standing guard exactly where you died.

**The idea** — When you die, QUAKECLONE saves the full path of that doomed run: a per-level timeline of where you stood, where you aimed, what you fired, and the spot where you fell. On your next attempt, each level replays that timeline as an **Echo Phantom** — a faceted, faintly-glowing duplicate of the player avatar that walks your exact old route in real time, fires the weapons you fired down the angles you aimed, and — at the moment its clock reaches your death-time — stops and *guards the spot where you died*. It is **solid**: it blocks doorways, body-blocks the key, and can be rocket-jumped over or rammed by the truck. Kill it for a small reward; ignore it and it dogs your old path.

**Why it's fresh** — Racing games have ghost cars and roguelikes have grave-robbing, but a ghost that is a *damage-dealing, collision-solid combatant replaying your own inputs* — and that camps your literal death coordinate as a boss-let — is its own thing. It turns your worst mistake into the level's newest hazard.

**How it plays** — You learn to read yourself. The Echo telegraphs your habits: it always over-peeks that lava ledge, always reloads in that doorway. You exploit the choreography you can predict, then break your own pattern so the *next* Echo can't predict you. Doorway-blocking forces detours; the death-spot guard makes you approach your grave differently than the panic that killed you there.

**How it fits QUAKECLONE** — It extends the existing `player.rs` `PlayerHistory` ring-buffer (already a recorded eye-position trail) from 0.6s into a persisted full-run timeline. Playback drives `physics.rs` `move_and_slide` along recorded positions and reserves a `WorldColliders` slot exactly like `vehicle.rs`'s moving brush, so collision/pathing/LoS treat it as solid for free. The body reuses `monster_model.rs`'s procedural rig and `enemies.rs` damage/death; firing replays through `weapons.rs`/`projectiles.rs`. `gamestate.rs` owns the save-on-death and per-level spawn.

**Build sketch** — Record a compact per-frame event stream (position, yaw/pitch, fire flags, weapon id) into a run buffer, serialize on death (RON, beside `config.ron`), and on level entry spawn a playback driver that interpolates the timeline. The honest hard part is **fidelity of replayed combat**: projectiles and hitscan must fire from recorded angles deterministically without re-simulating player input, and a desynced or drifting Echo must still feel intentional rather than buggy.

**Effort** — **M.** Main risk: making replayed fire and movement read as a believable opponent, not a glitchy mannequin, given the variable-timestep, non-deterministic frame loop.
