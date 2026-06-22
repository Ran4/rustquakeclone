# 04. Momentum Bank — Spend Speed You Never Stopped Earning

> Keep moving fast and a meter fills with a rising whine; tap it to cash all that stored speed into one violent burst — a second wall-jump, a wheels-pulping ram, or a full-arc ledge dive.

**The idea** — A passive **Momentum Bank** that charges off *sustained* horizontal velocity above a threshold (roughly the air-strafe cap). Drift slow or hit a wall and the meter bleeds; bunny-hop a clean line and it climbs toward full. When charged, a single keypress **discharges** it into one of three context-sensitive bursts: a **double wall-jump** (a second mid-air kick when you'd normally have spent your only one), a **pulp-ram** (a melee-range kinetic shove that gibs a Knight or flings a Grunt with truck-grade knockback), or a **full-ledge dive** (a flat forward launch that clears gaps a normal jump can't). The bank empties on use — you don't get speed for free, you *bank* the speed you already earned.

**Why it's fresh** — Most movement shooters *reward* speed with more speed (a feedback loop). The Momentum Bank instead makes velocity a **spendable currency** with a deliberate hoard-or-spend tension. You're never just fast; you're fast *and deciding when to convert it*. The rising synthesised whine turns your own momentum into an audible gauge you read without looking down.

**How it plays** — A whole skill layer appears: *do I burn the bank now to gib this Knight, or hold it to clear the lava gap ahead?* Veterans chain a bunny-hop run into a banked double wall-jump to reach a ledge the level never "intended"; reckless players blow it on a ram and find themselves slow and exposed. Every full meter is a held breath.

**How it fits QUAKECLONE** — It rides the velocity already tracked in **physics.rs**' swept-AABB solver, and the discharge is just an impulse applied through the same move-and-slide path as **player.rs**' wall-jump. The pulp-ram reuses **combat.rs** radius knockback/gib and **vehicle.rs**' ram-fling math; the dive is a flat velocity set. The fill whine is a looping note in **audio_gen.rs**, pitch/volume scrubbed by charge like the truck engine. A meter bar slots into **hud.rs**.

**Build sketch** — Add a charge resource updated each frame from the player's banked horizontal speed, with thresholds for fill/decay. On the burst key, branch on context (near wall / near monster / over a gap) and inject the matching impulse before the solver runs. The hard part is **tuning** the fill/decay curve and burst magnitudes so the bank feels earned, not exploitable, and doesn't break level sequencing — and disambiguating which of the three bursts fires.

**Effort** — **M.** Main risk: balance — a too-generous bank lets players skip whole map sections via the dive or double wall-jump.
