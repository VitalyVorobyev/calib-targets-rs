#!/usr/bin/env python3
"""Compose the repo README hero image from existing detection overlays.

Builds a 2x2 gallery of the four supported target types using the overlay
PNGs in ``book/src/img/``. Writes the result to ``docs/img/target_gallery.png``.
Re-run this script when any source overlay changes.
"""
from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_DIR = REPO_ROOT / "book" / "src" / "img"
OUT_DIR = REPO_ROOT / "docs" / "img"
OUT_PATH = OUT_DIR / "target_gallery.png"

TILES: list[tuple[str, str]] = [
    ("Chessboard",  "chessboard_detection_mid_overlay.png"),
    ("ChArUco",     "charuco_detect_report_small2_overlay.png"),
    ("PuzzleBoard", "puzzleboard_detect_overlay.png"),
    ("Marker board", "marker_detect_report_crop_overlay.png"),
]

TILE_W, TILE_H = 900, 520          # photo area per tile
LABEL_H      = 56                  # caption band below photo
PADDING      = 16                  # gap between tiles and around border
BG           = (18, 18, 20)        # near-black background
BAND         = (32, 32, 38)        # caption band
FG           = (240, 240, 244)     # caption text


def _font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for candidate in [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ]:
        if Path(candidate).exists():
            try:
                return ImageFont.truetype(candidate, size=size)
            except OSError:
                pass
    return ImageFont.load_default()


def _fit(image: Image.Image, w: int, h: int) -> Image.Image:
    """Resize `image` to fit inside `w x h`, keep aspect, center on BG."""
    image = image.convert("RGB")
    ratio = min(w / image.width, h / image.height)
    new_w = max(1, int(round(image.width * ratio)))
    new_h = max(1, int(round(image.height * ratio)))
    resized = image.resize((new_w, new_h), Image.LANCZOS)
    canvas = Image.new("RGB", (w, h), BG)
    canvas.paste(resized, ((w - new_w) // 2, (h - new_h) // 2))
    return canvas


def _make_tile(label: str, src: Path) -> Image.Image:
    photo = _fit(Image.open(src), TILE_W, TILE_H)
    tile = Image.new("RGB", (TILE_W, TILE_H + LABEL_H), BAND)
    tile.paste(photo, (0, 0))
    draw = ImageDraw.Draw(tile)
    font = _font(28)
    bbox = draw.textbbox((0, 0), label, font=font)
    tx = (TILE_W - (bbox[2] - bbox[0])) // 2
    ty = TILE_H + (LABEL_H - (bbox[3] - bbox[1])) // 2 - 2
    draw.text((tx, ty), label, fill=FG, font=font)
    return tile


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    tiles = [_make_tile(label, SRC_DIR / fname) for label, fname in TILES]

    cols, rows = 2, 2
    cell_w = TILE_W
    cell_h = TILE_H + LABEL_H
    total_w = cols * cell_w + (cols + 1) * PADDING
    total_h = rows * cell_h + (rows + 1) * PADDING

    gallery = Image.new("RGB", (total_w, total_h), BG)
    for i, tile in enumerate(tiles):
        r, c = divmod(i, cols)
        x = PADDING + c * (cell_w + PADDING)
        y = PADDING + r * (cell_h + PADDING)
        gallery.paste(tile, (x, y))

    gallery.save(OUT_PATH, optimize=True)
    print(f"wrote {OUT_PATH.relative_to(REPO_ROOT)} ({gallery.size[0]}x{gallery.size[1]})")


if __name__ == "__main__":
    main()
