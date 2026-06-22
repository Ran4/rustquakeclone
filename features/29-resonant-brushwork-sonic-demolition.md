# 29. Resonant Brushwork — Sonic Demolition

> Every surface has a ringing pitch, and holding the Lightning beam on a brush sweeps it up to its shatter note to crack open a shortcut.

**The idea** — Each brush material carries an acoustic profile: a fundamental tone, a damping factor, and a *shatter pitch*. Strike a surface — a pellet, a nail, a body slammed into it — and it *rings* at its tone, briefly, with the impact velocity setting the volume. A subset of brushes are flagged **resonant** (cracked ice, a brass bulkhead, a tomb wall, a dam sluice plate). Hold the Lightning beam on one and its hum climbs a continuous frequency sweep; reach the shatter pitch and the brush detonates into gibs, vanishing from the collider list and opening a shortcut, a vault, or a flood.

**Why it's fresh** — Destructible walls are old; *tuned* ones are not. The surface isn't a hitpoint bar — it's an instrument you have to play to its breaking note, and you can *hear* how close you are. The world becomes audibly material: ice tinks, brass bongs, stone thuds, and the same beam that fries a Knight is now a resonator you sweep like a theremin.

**How it plays** — You learn each level's palette by ear. A held beam on the wrong wall just hums and wastes Cells; the right wall whines higher and higher until it shatters — a tense ammo-vs-time bet under fire. Tap-fire to keep the charge from decaying, or commit the whole magazine. Resonant brushes become optional routes: skip a Death Knight by ringing open the dam sluice, or trade Cells for a wall-jump shortcut.

**How it fits QUAKECLONE** — The Lightning path in `weapons.rs` (its 0.06s continuous-beam cooldown) drives the sweep; `physics.rs` `ray_aabb`/`raycast_world` already hand back the hit brush and its normal. `level.rs`'s `Build` API and `ThemeId` palette gain a per-material acoustic profile; `audio_gen.rs` synthesises the ring and the rising sweep procedurally. Shattering removes the `Aabb` from `WorldColliders.solids` and spawns gibs via `effects.rs`/`combat.rs`; `gamestate.rs` ties opened brushes to objectives.

**Build sketch** — Add an acoustic profile to themed materials and tag resonant brushes in the Build API. The honest hard part: `WorldColliders.solids` is a flat untagged `Vec<Aabb>`, so brushes need a parallel index back to material + resonance, and the beam needs to track sustained dwell on one brush (a charge that decays off-target). Synthesising a convincing rising sweep without it grating after the tenth shot is the real tuning grind.

**Effort** — **M**. Main risk: the sweep audio feeling like a kazoo, and balancing Cell cost so shortcuts tempt without trivialising the key-and-door loop.
