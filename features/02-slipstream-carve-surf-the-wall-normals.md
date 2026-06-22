# 02. Slipstream Carve — Surf the Wall Normals

> The solver already tells you which way the wall faces — let the player ride that face, trading a fall into a screaming sideways carve.

**The idea** — Steep brush faces (think 30–55° tilted slabs) stop being dead surfaces you bonk and slide off, and become *ride-able ramps*. When you land against one, gravity gets projected down the slope instead of pinning you, and your tangential speed is preserved rather than clipped. You stick to the face, accelerate down its fall-line, and launch off the lip carrying all of it — Source/Tony-Hawk surf, native to a Quake brushset.

**Why it's fresh** — Surf is normally a community mod hack grafted onto Quake/Source movement. Here it falls straight out of geometry the engine *already computes*: `slide_move` in `physics.rs` returns the summed outward `wall_normal` every frame, and today the player throws it away (only `nearby_wall_normal` touches it, for wall-jumps). Promoting that vector from a wall-jump probe to a first-class movement state turns the whole level set into a skate park for free — no new collision math.

**How it plays** — A new third state joins grounded and airborne: *surfing*. Off a wall-jump you can chain into a carve down an angled buttress; on level 7's floating obsidian islands you surf a crystal bridge's canted side instead of falling into the void rift; on the level 8 dam face the convex arch becomes one long descending line. The skill is reading slope steepness and choosing your exit angle — leave early for height, ride to the lip for raw speed, or weave wall-jump → surf → air-strafe into one unbroken combo.

**How it fits QUAKECLONE** — Leans entirely on `physics.rs`: consume `MoveResult.wall_normal` and the existing slope test (`best_n.y` thresholds already split floor/wall/ceiling). Player integration lives in `player.rs` beside the wall-jump branch and the `GRAVITY`/`AIR_CAP` logic. Authors opt brushes in via a tilt flag on the `level.rs` Build API; a soft wind loop from `audio_gen.rs` fades in with carve speed.

**Build sketch** — Add a surf band: when a hit normal sits between the wall and floor cutoffs, don't clip velocity flat — split gravity into normal + tangent, kill the normal component (stick), keep the tangent (slide), and skip ground friction. The honest hard part is the boundary: detangling surf-stick from the swept solver's penetration heal and step-up so you ride smoothly without snapping to ground or rattling off the lip.

**Effort** — **M**. Main risk: tuning the stick/release thresholds so a glancing wall-graze doesn't trap the player mid-air, and keeping it from fighting `depenetrate` and step-up at slope seams.
