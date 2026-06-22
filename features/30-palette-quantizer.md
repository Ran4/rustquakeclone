# 30. Palette Quantizer — The Dimension Decides the Colors

> Each dimension paints in eight colors, and only the things that want to kill you (or save you) are allowed to glow brighter than its palette.

**The idea** — A post-process pass that, between the lit HDR frame and the existing bloom step, quantizes every pixel to a small indexed palette (8–16 swatches) chosen by the active theme. Frostspire renders in cold blue-greys; the Verdant Rot in sick greens; the Tomb in dusty golds. The trick is the threshold: pixels under an HDR brightness cutoff get snapped to the nearest palette swatch and flattened; pixels *over* the cutoff — emissive lava, pickup glints, muzzle flashes, explosion lights — are passed through untouched, so they punch out of the flat dimension and bloom into pure white-hot. Readability becomes a property of the renderer, not of memorized item silhouettes.

**Why it's fresh** — Retro palette reduction usually fights HDR and bloom; here it *feeds* them. The quantizer is the gate that decides what gets to be bright. One shader gives every dimension a distinct, instantly-legible look and guarantees that anything dangerous or grabbable is the only thing that glows — style and signposting from the same knob.

**How it plays** — The world reads as a flat, graphic poster, so a glowing rocket arc, a key on a pedestal, or an Ogre's grenade fuse pops impossibly hard against it. You learn to scan for the bloom, not the shape. Crossing a slipgate is a full repaint — the whole color language of the level changes in one frame, reinforcing "new dimension."

**How it fits QUAKECLONE** — It slots onto the player camera already running `Tonemapping::AcesFitted` + `Bloom::NATURAL` in `player.rs`, inserted just before bloom. The palette and brightness cutoff come straight from each `ThemeSpec` in `level.rs` (which already carries `tint`, `accent`, `fog`, `ambient`, `clear`), so `apply_theme_and_build` wires it like fog. Emissive hazards (`hazard_emissive`) and glowing pickups (`pickups.rs`) already sit above the cutoff for free.

**Build sketch** — A fullscreen post-process node sampling the HDR target, a per-theme palette uniform (small swatch array), and a luminance threshold. The hard part is the cutoff curve: too low and pickups stop bleeding; too high and the whole frame quantizes to mud. Tune per theme, validate via `QC_LEVELSHOT`.

**Effort** — **M**. Main risk: a wgpu/Bevy 0.19 render-graph node ordering against the existing bloom pass.
