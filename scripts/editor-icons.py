#!/usr/bin/env python3
"""Derive the editor file-tree icons from the brand assets, with room to breathe.

**Why this is a script and not three files somebody cropped.** The artwork is a designer's, but
the PADDING is a derivation, and it has one number in it: how much of the icon's box the ink is
allowed to fill. Every icon in the family has to agree on that number or the tree looks ragged —
`.bx`, `.bmx` and `.sbmx` sit on consecutive rows, and an eye reads inconsistent margins as
misalignment rather than as three different logos.

**The number is 70%, and it came from a complaint rather than taste.** The shipped `.bx` icon
filled 86% of its height: 4 pixels of clear space at 48px, which in a VS Code row puts the glyph
against the filename. At 70% there are 7 clear pixels above and below, which is what every other
language icon in a tree has.

The source PNGs are cropped to their own ink first, so the margin is measured from the ARTWORK
and not from whatever canvas the exporter happened to use — two of the three sources had
different amounts of built-in slack, which is exactly how a family drifts.

    python3 scripts/editor-icons.py            # write them
    python3 scripts/editor-icons.py --check     # fail if what is committed is not what this makes
"""
import sys
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent

# The fraction of the box the ink may fill. See the docstring — this is the whole decision.
INK = 0.70

# source, and where the icons go. The `.bx` one lands in the extension that ships; the other two
# are for the extensions BMX and star-burxt own, so they land in `assets/subprojects/` where those
# repositories can take them without this one reaching into theirs.
ICONS = [
    ("assets/burxt-b-favicon-512.png", "editors/vscode/file-icon.png", 48),
    ("assets/burxt-b-favicon-512.png", "assets/subprojects/burxt-bx-icon-128.png", 128),
    ("assets/subprojects/bmx_fileicon_256.png", "assets/subprojects/bmx-file-icon-48.png", 48),
    ("assets/subprojects/bmx_fileicon_256.png", "assets/subprojects/bmx-file-icon-128.png", 128),
    ("assets/subprojects/star_gearicon_256.png", "assets/subprojects/sbmx-gear-icon-48.png", 48),
    ("assets/subprojects/star_gearicon_256.png", "assets/subprojects/sbmx-gear-icon-128.png", 128),
]


def derive(source: Path, box: int) -> Image.Image:
    src = Image.open(source).convert("RGBA")
    bbox = src.getbbox()
    if bbox is None:
        raise SystemExit("%s is entirely transparent" % source)
    ink = src.crop(bbox)
    height = int(round(box * INK))
    width = max(1, int(round(height * ink.width / ink.height)))
    if width > box:                      # a wide mark is bounded by the width instead
        width = int(round(box * INK))
        height = max(1, int(round(width * ink.height / ink.width)))
    out = Image.new("RGBA", (box, box), (0, 0, 0, 0))
    out.paste(ink.resize((width, height), Image.LANCZOS),
              ((box - width) // 2, (box - height) // 2))
    return out


def main() -> int:
    check = "--check" in sys.argv
    stale = []
    for source, target, box in ICONS:
        made = derive(ROOT / source, box)
        path = ROOT / target
        if check:
            if not path.exists():
                stale.append("%s does not exist" % target)
                continue
            if list(Image.open(path).convert("RGBA").getdata()) != list(made.getdata()):
                stale.append("%s is not what this script makes" % target)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        made.save(path)
        clear = made.getbbox()[1]
        print("wrote %s (%dpx, %d clear above and below)" % (target, box, clear))
    if check:
        if stale:
            print("editor icons are stale — run: python3 scripts/editor-icons.py")
            for s in stale:
                print("  " + s)
            return 1
        print("editor icons are current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
