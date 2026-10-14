"""Builds assets/app.ico and assets/icon.png from a 16x16 pixel-art source.

Run from the repo root: python tools/make_icon.py
Every size is a nearest-neighbour multiple of the source, so the icon stays crisp.
"""
from pathlib import Path

from PIL import Image

# . transparent, F frame, S sky, U sun, M near hills, D far hills, K shadow
ART = [
    "................",
    ".FFFFFFFFFFFFFF.",
    ".FSSSSSSSSSSSSFK",
    ".FSSSSSSSSUUSSFK",
    ".FSSSSSSSSUUSSFK",
    ".FSSSSSSSSSSSSFK",
    ".FSSSSDSSSSSSSFK",
    ".FSSSDDDSSSSSSFK",
    ".FSSDDDDDSSSMSFK",
    ".FSDDDDDDDSMMMFK",
    ".FDDDDDDDMMMMMFK",
    ".FDDDDDDMMMMMMFK",
    ".FMMMMMMMMMMMMFK",
    ".FFFFFFFFFFFFFFK",
    "..KKKKKKKKKKKKKK",
    "................",
]

PALETTE = {
    ".": (0, 0, 0, 0),
    "F": (201, 211, 168, 255),
    "S": (58, 79, 87, 255),
    "U": (238, 226, 160, 255),
    "M": (111, 143, 122, 255),
    "D": (85, 115, 100, 255),
    "K": (20, 22, 26, 140),
}


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    src = Image.new("RGBA", (16, 16))
    for y, row in enumerate(ART):
        for x, ch in enumerate(row):
            src.putpixel((x, y), PALETTE[ch])

    sizes = [16, 32, 48, 64, 128, 256]
    frames = [src.resize((s, s), Image.Resampling.NEAREST) for s in sizes]
    out = root / "assets"
    out.mkdir(exist_ok=True)
    frames[-1].save(out / "app.ico", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    frames[3].save(out / "icon.png")


if __name__ == "__main__":
    main()
