#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Generate the game's *non-procedural* sound effects with ElevenLabs.

Most of QUAKECLONE's SFX are synthesised at startup by `src/audio_gen.rs` (no
binary assets). A few sounds — the truck's engine, tyre screech and crash hits —
want real recorded-sounding audio, so we render them with ElevenLabs'
text-to-sound-effect API instead.

The pattern mirrors `generate_images.py`: this script is the single registry of
every ElevenLabs sound the game needs. Running it renders only the ones whose
WAV does *not* already exist on disk, so it is safe to re-run and cheap after
adding a new entry. These WAVs are committed to the repo (unlike the procedural
ones, which are gitignored build artifacts) because they are not reproducible
without an API key.

    uv run scripts/generate_sounds.py            # render anything missing
    uv run scripts/generate_sounds.py --list     # show what exists / is missing
    uv run scripts/generate_sounds.py --only engine
    uv run scripts/generate_sounds.py --force     # re-render even if present

The key is read from the environment or the repo-root .env (ELEVENLABS_API_KEY).
ElevenLabs returns MP3; we transcode to mono 22 050 Hz WAV (matching the
procedural set) with `ffmpeg`, which must be on PATH.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SOUNDS = REPO_ROOT / "assets" / "sounds"
API_URL = "https://api.elevenlabs.io/v1/sound-generation?output_format=mp3_44100_128"
# Match the procedural WAVs (see SR in src/audio_gen.rs): mono, 22.05 kHz.
SR = 22_050


@dataclass
class Sfx:
    """One ElevenLabs sound: filename, prompt, length and whether it loops."""

    name: str  # written to assets/sounds/<name>.wav
    prompt: str
    seconds: float
    loop: bool = False
    # prompt_influence: higher = stick closer to the prompt, lower = more varied.
    influence: float = 0.5


# ---------------------------------------------------------------------------
# The manifest — every ElevenLabs sound the game loads (see src/vehicle.rs).
# ---------------------------------------------------------------------------
MANIFEST: list[Sfx] = [
    # Continuous engine note. Kept a steady mid-RPM rumble (not a lumpy idle) so
    # the engine reads well across the whole pitch range the game scrubs it
    # through at runtime — low/slow = idle, sped up = revving "vroom".
    Sfx(
        "engine_loop",
        "A petrol car engine running steadily at medium RPM, deep continuous "
        "mechanical rumble and burble, close exhaust, no music, no voices, "
        "seamless loop.",
        seconds=5.0,
        loop=True,
        influence=0.6,
    ),
    # One-shot ignition played when you take the wheel.
    Sfx(
        "engine_start",
        "A car engine ignition: the starter motor cranks and the engine catches "
        "and rumbles to life, then settles, single take, no music.",
        seconds=2.5,
        loop=False,
        influence=0.55,
    ),
    # Looping tyre skid, faded in by lateral slip while cornering hard.
    Sfx(
        "tire_screech",
        "Car tyres screeching and skidding on asphalt, continuous high-pitched "
        "squealing rubber skid, no music, seamless loop.",
        seconds=3.0,
        loop=True,
        influence=0.6,
    ),
    # One-shot metallic clang when the truck slams a wall.
    Sfx(
        "crash",
        "A heavy vehicle crashing hard into a metal wall, loud metallic crunch, "
        "bang and clang of buckling sheet metal, single impact.",
        seconds=1.6,
        loop=False,
        influence=0.6,
    ),
    # One-shot meaty thud when the truck rams a monster.
    Sfx(
        "ram_hit",
        "A heavy truck slamming into a large creature, a meaty heavy thud and "
        "bone crunch with a dull metallic bang, single impact.",
        seconds=1.2,
        loop=False,
        influence=0.6,
    ),
]


def load_env_key() -> str | None:
    """Return ELEVENLABS_API_KEY from the environment or the repo-root .env."""
    if os.environ.get("ELEVENLABS_API_KEY"):
        return os.environ["ELEVENLABS_API_KEY"]
    env = REPO_ROOT / ".env"
    if env.exists():
        for line in env.read_text().splitlines():
            line = line.strip()
            if line.startswith("ELEVENLABS_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return None


def fetch_mp3(key: str, sfx: Sfx) -> bytes:
    """Call the ElevenLabs sound-generation API and return MP3 bytes."""
    body = json.dumps(
        {
            "text": sfx.prompt,
            "duration_seconds": sfx.seconds,
            "prompt_influence": sfx.influence,
            "loop": sfx.loop,
        }
    ).encode()
    req = urllib.request.Request(
        API_URL,
        data=body,
        headers={"xi-api-key": key, "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read()


def to_wav(mp3: bytes, out: Path) -> None:
    """Transcode MP3 bytes to a mono SR-Hz 16-bit WAV with ffmpeg."""
    with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as tmp:
        tmp.write(mp3)
        tmp_path = tmp.name
    try:
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", tmp_path,
             "-ar", str(SR), "-ac", "1", "-sample_fmt", "s16", str(out)],
            check=True,
        )
    finally:
        os.unlink(tmp_path)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--force", action="store_true", help="re-render even if the WAV exists")
    ap.add_argument("--only", default=None, help="only entries whose name contains this substring")
    ap.add_argument("--list", action="store_true", help="list manifest status and exit")
    ap.add_argument("--dry-run", action="store_true", help="show what would render, but don't call the API")
    args = ap.parse_args()

    items = [s for s in MANIFEST if not args.only or args.only in s.name]

    if args.list:
        for s in items:
            exists = (SOUNDS / f"{s.name}.wav").exists()
            print(f"  [{'x' if exists else ' '}] sounds/{s.name}.wav")
        return 0

    todo = [s for s in items if args.force or not (SOUNDS / f"{s.name}.wav").exists()]
    skipped = len(items) - len(todo)
    if skipped:
        print(f"✓ {skipped} already present (skipping)")
    if not todo:
        print("Nothing to render — all sounds present.")
        return 0

    print(f"→ {len(todo)} to render:")
    for s in todo:
        print(f"    {s.name}.wav")
    if args.dry_run:
        return 0

    key = load_env_key()
    if not key:
        print("ERROR: ELEVENLABS_API_KEY not set (env or .env).", file=sys.stderr)
        return 1

    SOUNDS.mkdir(parents=True, exist_ok=True)
    failures = 0
    for s in todo:
        out = SOUNDS / f"{s.name}.wav"
        print(f"… rendering {s.name}.wav ({s.seconds}s, loop={s.loop}) …", flush=True)
        try:
            to_wav(fetch_mp3(key, s), out)
            print(f"  ✓ wrote {out.relative_to(REPO_ROOT)} ({out.stat().st_size // 1024} KB)")
        except urllib.error.HTTPError as e:  # surface the API's error body
            failures += 1
            print(f"  ✗ failed: HTTP {e.code} {e.read().decode(errors='replace')[:300]}", file=sys.stderr)
        except Exception as e:  # keep going; report at the end
            failures += 1
            print(f"  ✗ failed: {e}", file=sys.stderr)

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
