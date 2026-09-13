#!/usr/bin/env python3
"""Render packaging/appimage/icon.png (256x256) from the barcode motif.

Usage: python3 packaging/appimage/make_icon.py [out.png]
Requires: Pillow (pip install pillow).
"""

import sys

from PIL import Image, ImageDraw

SIZE = 256
BG = (10, 10, 11)
FG = (242, 242, 240)
FAINT = (85, 85, 90)
BORDER = (43, 43, 48)

# (x, width, dim?) barcode bars, mirroring icon.svg
BARS = [
    (52, 14, False),
    (72, 6, False),
    (84, 10, False),
    (100, 6, True),
    (112, 18, False),
    (136, 6, False),
    (148, 12, False),
    (166, 6, True),
    (178, 14, False),
    (198, 6, False),
]


def main() -> None:
    out = sys.argv[1] if len(sys.argv) > 1 else "icon.png"
    img = Image.new("RGB", (SIZE, SIZE), BG)
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=56, outline=BORDER, width=6)
    for x, w, dim in BARS:
        d.rectangle([x, 64, x + w - 1, 191], fill=FAINT if dim else FG)
    img.save(out)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
