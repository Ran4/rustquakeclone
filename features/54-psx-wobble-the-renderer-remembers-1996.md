# 54. PSX Wobble — The Renderer Remembers 1996

> The art is already low-poly and faceted. Push it off the cliff: snap the verts, warp the textures, and call it a lost disc.

**The idea** — A retro render mode that commits fully to the aesthetic the game already half-wears. Every vertex is snapped to a coarse screen-space grid in the vertex shader, so geometry shimmers and *wobbles* as you move — the seasick PS1 jitter. Texture coordinates are interpolated affine (perspective-incorrect), so floors and walls *swim* and skew as they turn away from you. The whole scene renders into a low-resolution offscreen target and is upscaled hard with point sampling, chunky and aliased — QUAKECLONE as a scratched 1996 console disc pulled from a bargain bin.

**Why it's fresh** — Most "retro" filters bolt a CRT overlay onto a modern frame; this one corrupts the actual rasterisation — snapped verts and affine UVs are *geometric* artefacts, not a screen effect, so they react to motion the way a real PlayStation did. For a Quake-flavoured game that already ships faceted, flat-shaded, vertex-jittered models, it's the natural endgame: the same hard-edged look pushed from "stylised" to "authentically broken."

**How it plays** — Nothing about the rules changes — it's a pure look toggle — but the *feel* shifts hard: distance reads as a swimming haze, charging Knights jitter and pop, and the whole campaign gains a nostalgic, uncanny menace. Flip it off and the same maps snap back to clean modern faceting, so it doubles as a per-run mood knob.

**How it fits QUAKECLONE** — `monster_model.rs` already flat-shades and per-vertex-jitters its faceted rig meshes, so the wobble is the same idea promoted to a world-wide custom material/shader applied to every brush mesh from `level.rs` and every bone mesh from `monster_model.rs`. It runs *under* the existing post chain in `player.rs` (the `Tonemapping` + `Bloom` camera), composes cleanly with the Palette Quantizer pass and the per-theme distance fog from `level.rs`'s `ThemeSpec`, and is gated by a field in `config.ron` plus a `QC_` env flag like the other debug toggles. `QC_GALLERY` and `QC_LEVELSHOT` verify the rig and level looks frame-for-frame.

**Build sketch** — Author one `Material` (extending Bevy's PBR or a bespoke shader) whose vertex stage rounds `clip_position.xy` to a grid and whose fragment stage forces affine UV interpolation, then render the camera into a half- or quarter-res `Image` target upscaled with `ImageSampler::nearest`. The honest hard part is doing snap + affine inside Bevy 0.19's render pipeline without forking its mesh pipeline, and keeping the scene *readable* at low res so pickups and exits still pop.

**Effort** — **M.** Main risk: bending Bevy 0.19's mesh pipeline to a custom vertex-snap + affine shader without fighting the engine.
