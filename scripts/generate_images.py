#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["openai>=1.0", "pillow"]
# ///
"""
Generate the game's image assets (textures) with OpenAI's `gpt-image-2`.

The pattern: this script is the single registry of every image the game needs.
Running it scans the manifest below and renders only the ones whose PNG does
*not* already exist on disk — so it is safe to run repeatedly and cheap to
re-run after adding a new entry. Delete a PNG (or pass --force) to re-render it.

    uv run scripts/generate_images.py            # render anything missing
    uv run scripts/generate_images.py --list     # show what exists / is missing
    uv run scripts/generate_images.py --only wall # only entries matching "wall"
    uv run scripts/generate_images.py --force     # re-render even if present

The OpenAI key is read from the environment or from the repo-root .env
(OPENAI_API_KEY=...).
"""

from __future__ import annotations

import argparse
import base64
import io
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ASSETS = REPO_ROOT / "assets"

# Shared instruction so every world texture comes back tileable and unlit — a
# flat albedo map, which is what the renderer wants (lighting is done in-engine).
SEAMLESS = (
    "Seamless tileable texture, flat even orthographic lighting with no cast "
    "shadows and no glare, photoreal albedo/diffuse map only, the four edges "
    "wrap perfectly so the image can be tiled in a grid. "
)


@dataclass
class Image:
    """One image the game needs: where it lives + how to draw it."""

    path: str  # relative to assets/
    prompt: str
    size: str = "1024x1024"
    quality: str = "high"
    # World tiles are downscaled to keep them tileable-crisp and the repo small.
    resize: int | None = 512


# ---------------------------------------------------------------------------
# The manifest — every image the game loads.
# ---------------------------------------------------------------------------
# World/level surface textures (these are the new ones — the walls etc.).
WORLD: list[Image] = [
    Image(
        "textures/world/wall.png",
        SEAMLESS
        + "Dark-fantasy Quake-style techbase wall: large rough riveted stone-"
        "and-metal blocks, grime and rust streaks running down, hard angular "
        "facets, muted grey-brown palette, gritty and oppressive.",
    ),
    Image(
        "textures/world/floor.png",
        SEAMLESS
        + "Worn dungeon floor of cracked grey-brown stone flagstones, dirt and "
        "soot packed into the grooves, scuffed and uneven, top-down view.",
    ),
    Image(
        "textures/world/ceiling.png",
        SEAMLESS
        + "Very dark sooty stone ceiling, rough black-brown rock with faint "
        "embedded iron supports, deep shadowed grime, nearly black.",
    ),
    Image(
        "textures/world/trim.png",
        SEAMLESS
        + "Ornate tarnished brass-and-gold runed trim metal, embossed angular "
        "occult Quake sigils, scratched gold patina over dark bronze.",
    ),
    Image(
        "textures/world/metal.png",
        SEAMLESS
        + "Riveted gunmetal steel floor plate, brushed dark panels with bolts, "
        "diamond-tread sections, grime and faint rust in the seams.",
    ),
    Image(
        "textures/world/door.png",
        SEAMLESS
        + "Heavy iron dungeon door panel: banded studded dark metal with thick "
        "vertical seams, big rivets and a central reinforcing plate, grimy.",
    ),
    Image(
        "textures/world/lava.png",
        SEAMLESS
        + "Molten lava surface, bright glowing orange-yellow cracks splitting a "
        "dark cooling basalt crust, fierce emissive magma, top-down view.",
    ),
]

# Per-theme world tilesets for the campaign's six new levels. Each theme has the
# same six surfaces (floor/wall/ceiling/trim/hazard/door); `metal.png` is shared
# from the base set. Folders mirror `ThemeId` in src/level.rs.
def _theme(dir: str, surfaces: dict[str, str]) -> list[Image]:
    return [Image(f"textures/world/{dir}/{name}.png", SEAMLESS + desc) for name, desc in surfaces.items()]


THEMES: list[Image] = (
    # --- Frostspire Keep: a winter ice fortress ---
    _theme("frost", {
        "floor": "Snow-dusted pale blue-grey ice flagstones, hairline frost "
        "cracks, packed snow settled into the grooves, glittering rime, top-down.",
        "wall": "Frost-rimed grey stone fortress blocks, sheets of pale-blue ice "
        "and small icicles clinging to the joints, snow on the ledges, cold.",
        "ceiling": "Dark blue-grey frozen stone ceiling crusted with frost "
        "crystals and thin icicles, shadowed and icy, near-black blue.",
        "trim": "Frosted silver-blue carved metal trim with angular runic "
        "engravings, glinting ice glaze over pale steel.",
        "hazard": "Cracked frozen-lake surface, jagged broken ice plates over "
        "glowing pale-cyan freezing water in the cracks, top-down.",
        "door": "Heavy frost-covered iron portcullis gate, banded studded metal "
        "sheathed in pale-blue ice with hanging icicles.",
    })
    # --- The Brass Leviathan: a steampunk clockwork foundry ---
    + _theme("brass", {
        "floor": "Riveted brass and copper deck plating etched with gear teeth, "
        "oily warm-metal sheen, bolt heads, faint soot, top-down.",
        "wall": "Steampunk machine wall of interlocking brass pipes, copper "
        "panels, pressure gauges and big rivets, warm tarnished gold-bronze.",
        "ceiling": "Dark sooty iron machine ceiling crossed by brass steam pipes "
        "and bolted girders, grimy and shadowed.",
        "trim": "Polished brass gear-toothed ornate trim, interlocking cogs and "
        "filigree, bright golden machined metal.",
        "hazard": "Channel of glowing molten metal in a foundry, bright orange-"
        "yellow liquid brass over a dark cooling crust, sparks, top-down.",
        "door": "Massive riveted brass bulkhead hatch with a central iron wheel "
        "valve, banded copper plates and bolts.",
    })
    # --- Tomb of the Sunken King: an Egyptian desert tomb ---
    + _theme("tomb", {
        "floor": "Sandstone tomb floor of worn golden blocks, drifted sand packed "
        "into the seams, faint faded hieroglyph carvings, top-down.",
        "wall": "Carved sandstone tomb wall densely covered in Egyptian "
        "hieroglyphs and figures with weathered gold and turquoise inlay.",
        "ceiling": "Sandstone tomb ceiling painted with faded gold stars on deep "
        "ochre, cracked plaster, dim and dusty.",
        "trim": "Pharaonic gold trim band with lapis-blue inlay, ankh and eye-of-"
        "Horus sigils, polished gilt over sandstone.",
        "hazard": "Cursed glowing quicksand pit, swirling golden-ochre sand with "
        "faint amber glow seeping up, top-down.",
        "door": "Massive carved sandstone tomb door with a central scarab seal, "
        "gold-inlaid hieroglyphs and cracked stone.",
    })
    # --- The Verdant Rot: an alien bio-hive / toxic lab ---
    + _theme("hive", {
        "floor": "Organic alien hive floor of ridged fleshy membrane laced with "
        "glowing green veins, slick and wet, biomechanical, top-down.",
        "wall": "Biomechanical alien wall: ribbed dark chitin and pulsing green-"
        "veined flesh fused with corroded rusty sci-fi metal panels.",
        "ceiling": "Dark organic hive ceiling of dripping membranes and hanging "
        "tendrils, black-green and glistening.",
        "trim": "Corroded sci-fi metal trim threaded with glowing toxic-green "
        "energy conduits and warning stripes.",
        "hazard": "Bubbling radioactive toxic sludge, bright acid-green glowing "
        "ooze with rising bubbles and froth, top-down.",
        "door": "Corroded sci-fi blast door fused with alien bio-growth, green "
        "ooze seeping from the seams, ribbed organic plating.",
    })
    # --- The Salt Wraith: a sci-fi sky pirate ship / airship ---
    + _theme("pirate", {
        "floor": "Weathered ship-deck wooden planks, caulked tar seams, salt-"
        "bleached grey-brown oak, brass nail heads, top-down.",
        "wall": "Dark tarred ship hull planking with brass bracing straps, "
        "porthole rivets and rope lashings, nautical and worn.",
        "ceiling": "Wooden ship overhead deck beams with rope rigging and a hung "
        "lantern glow, dark varnished oak.",
        "trim": "Ornate brass-and-gold ship railing filigree with twisted rope "
        "molding, polished nautical metal.",
        "hazard": "Glowing electric-blue plasma engine exhaust vent, crackling "
        "cyan energy over dark grating, top-down.",
        "door": "Heavy galleon hatch of brass-bound dark oak planks with a ship's "
        "wheel and iron studs.",
    })
    # --- Sanctum of the Void: the cosmic crystal endgame ---
    + _theme("void", {
        "floor": "Polished black obsidian floor shot through with glowing violet "
        "crystal veins and faint starlight flecks, mirror-dark, top-down.",
        "wall": "Obsidian void-temple wall embedded with glowing purple crystals "
        "and etched glowing silver arcane runes, deep cosmic black.",
        "ceiling": "Cosmic starfield ceiling, deep black-purple nebula scattered "
        "with stars and faint violet light, voidlike.",
        "trim": "Silver arcane runed trim glowing violet, geometric eldritch "
        "engraving over polished dark metal.",
        "hazard": "Swirling violet void rift, churning magenta-purple antimatter "
        "energy with glowing filaments, top-down.",
        "door": "Obsidian gate inlaid with glowing violet sigils around a central "
        "glowing purple crystal core, eldritch.",
    })
)

# Monster skins live here too so the manifest is the *complete* registry. These
# PNGs already ship in the repo, so a normal run skips them; they are listed so
# the set is reproducible if one is ever deleted.
MONSTERS: list[Image] = [
    Image("textures/monsters/grunt_skin.png",
          "Seamless dark-fantasy albedo texture: pale grey rotting human soldier "
          "flesh, veined and bruised, no lighting.", resize=None),
    Image("textures/monsters/grunt_armor.png",
          "Seamless dark-fantasy albedo texture: scuffed olive-brown military "
          "flak armor plating with buckles, no lighting.", resize=None),
    Image("textures/monsters/enforcer_skin.png",
          "Seamless dark-fantasy albedo texture: sickly grey-green augmented "
          "soldier skin with scars, no lighting.", resize=None),
    Image("textures/monsters/enforcer_armor.png",
          "Seamless dark-fantasy albedo texture: dark riveted gunmetal enforcer "
          "armor with glowing energy conduits, no lighting.", resize=None),
    Image("textures/monsters/knight_steel.png",
          "Seamless dark-fantasy albedo texture: battered polished steel knight "
          "plate armor, scratched, no lighting.", resize=None),
    Image("textures/monsters/knight_mail.png",
          "Seamless dark-fantasy albedo texture: dark interlocking chainmail "
          "over black cloth, no lighting.", resize=None),
    Image("textures/monsters/scrag_flesh.png",
          "Seamless dark-fantasy albedo texture: pale translucent floating-demon "
          "flesh, purple veins, clammy, no lighting.", resize=None),
    Image("textures/monsters/scrag_membrane.png",
          "Seamless dark-fantasy albedo texture: thin veined bat-like wing "
          "membrane, purplish-grey, no lighting.", resize=None),
    Image("textures/monsters/ogre_hide.png",
          "Seamless dark-fantasy albedo texture: thick warty brown ogre hide, "
          "leathery and scarred, no lighting.", resize=None),
    Image("textures/monsters/ogre_apron.png",
          "Seamless dark-fantasy albedo texture: filthy bloodstained leather "
          "butcher's apron, no lighting.", resize=None),
    Image("textures/monsters/dk_armor.png",
          "Seamless dark-fantasy albedo texture: obsidian black hell-plate armor "
          "with dull red glowing runes, no lighting.", resize=None),
    Image("textures/monsters/dk_cloth.png",
          "Seamless dark-fantasy albedo texture: tattered blood-red demonic cape "
          "cloth, frayed, no lighting.", resize=None),
    Image("textures/monsters/bone.png",
          "Seamless dark-fantasy albedo texture: aged yellow-grey bone, cracked "
          "and pitted, no lighting.", resize=None),
    Image("textures/monsters/demon_metal.png",
          "Seamless dark-fantasy albedo texture: dark demonic blade steel, "
          "blued metal with etched runes, no lighting.", resize=None),
]

# Weapon-material albedo maps. The ground-pickup weapon models and the
# first-person view-models are skinned with these by material role (not one per
# weapon): receivers/barrels/stocks pick gunmetal, brass, steel or wood, while
# the painted body sheet is tinted (green/red/blue) per weapon at material build
# time — so it must stay a light, even mid-grey so the tint reads true.
WEAPONS: list[Image] = [
    Image(
        "textures/weapons/gunmetal.png",
        SEAMLESS
        + "Dark-fantasy Quake-style weapon material: blued gunmetal steel, "
        "brushed dark panels with rivets and scratched edge wear, faint grime "
        "in the seams, muted neutral grey-blue, matte.",
    ),
    Image(
        "textures/weapons/brass.png",
        SEAMLESS
        + "Dark-fantasy Quake-style weapon material: aged polished brass and "
        "bronze gun-barrel metal, warm golden hue with darker patina, fine "
        "scratches and tarnish.",
    ),
    Image(
        "textures/weapons/steel.png",
        SEAMLESS
        + "Dark-fantasy Quake-style weapon material: bright brushed stainless "
        "steel, fine parallel grain, cold light grey, clean with a few "
        "scratches.",
    ),
    Image(
        "textures/weapons/wood.png",
        SEAMLESS
        + "Dark-fantasy Quake-style weapon material: dark varnished walnut "
        "gunstock wood, fine straight grain, deep warm brown, subtle sheen.",
    ),
    Image(
        "textures/weapons/painted.png",
        SEAMLESS
        + "Dark-fantasy Quake-style weapon material: even light neutral grey "
        "matte painted military metal, uniform mid-tone with subtle chips, "
        "scuffs and scratches revealing darker metal beneath, no strong color "
        "so it can be tinted.",
    ),
]

MANIFEST: list[Image] = WORLD + THEMES + MONSTERS + WEAPONS


def load_env_key() -> str | None:
    """Return OPENAI_API_KEY from the environment or the repo-root .env."""
    if os.environ.get("OPENAI_API_KEY"):
        return os.environ["OPENAI_API_KEY"]
    env = REPO_ROOT / ".env"
    if env.exists():
        for line in env.read_text().splitlines():
            line = line.strip()
            if line.startswith("OPENAI_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return None


def render(client, img: Image) -> bytes:
    """Call gpt-image-2 and return PNG bytes (optionally downscaled)."""
    resp = client.images.generate(
        model="gpt-image-2",
        prompt=img.prompt,
        size=img.size,
        quality=img.quality,
        n=1,
    )
    data = base64.b64decode(resp.data[0].b64_json)
    if img.resize:
        from PIL import Image as PILImage

        im = PILImage.open(io.BytesIO(data)).convert("RGB")
        im = im.resize((img.resize, img.resize), PILImage.LANCZOS)
        buf = io.BytesIO()
        im.save(buf, format="PNG")
        data = buf.getvalue()
    return data


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--force", action="store_true", help="re-render even if the PNG exists")
    ap.add_argument("--only", default=None, help="only entries whose path contains this substring")
    ap.add_argument("--list", action="store_true", help="list manifest status and exit")
    ap.add_argument("--dry-run", action="store_true", help="show what would render, but don't call the API")
    ap.add_argument("--quality", default=None, help="override quality (low/medium/high)")
    args = ap.parse_args()

    items = [i for i in MANIFEST if not args.only or args.only in i.path]

    if args.list:
        for i in items:
            exists = (ASSETS / i.path).exists()
            print(f"  [{'x' if exists else ' '}] {i.path}")
        return 0

    todo = [i for i in items if args.force or not (ASSETS / i.path).exists()]
    skipped = len(items) - len(todo)
    if skipped:
        print(f"✓ {skipped} already present (skipping)")
    if not todo:
        print("Nothing to render — all images present.")
        return 0

    print(f"→ {len(todo)} to render:")
    for i in todo:
        print(f"    {i.path}")
    if args.dry_run:
        return 0

    key = load_env_key()
    if not key:
        print("ERROR: OPENAI_API_KEY not set (env or .env).", file=sys.stderr)
        return 1
    from openai import OpenAI

    client = OpenAI(api_key=key)

    failures = 0
    for i in todo:
        if args.quality:
            i.quality = args.quality
        out = ASSETS / i.path
        out.parent.mkdir(parents=True, exist_ok=True)
        print(f"… rendering {i.path} ({i.size}, q={i.quality}) …", flush=True)
        try:
            out.write_bytes(render(client, i))
            print(f"  ✓ wrote {out.relative_to(REPO_ROOT)} ({out.stat().st_size // 1024} KB)")
        except Exception as e:  # keep going; report at the end
            failures += 1
            print(f"  ✗ failed: {e}", file=sys.stderr)

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
