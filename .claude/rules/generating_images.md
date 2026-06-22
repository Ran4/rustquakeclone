All textures are produced by one idempotent script — the single registry of every image the game needs:

```
uv run scripts/generate_images.py          # render anything whose PNG is missing
uv run scripts/generate_images.py --list   # show what exists vs. is missing
uv run scripts/generate_images.py --force  # re-render everything
```

It scans the manifest, skips any image that already has a PNG on disk, and renders only the rest
with `gpt-image-2` (reading `OPENAI_API_KEY` from the env or `.env`). Add an entry to the manifest
to introduce a new texture; delete a PNG to regenerate it.
