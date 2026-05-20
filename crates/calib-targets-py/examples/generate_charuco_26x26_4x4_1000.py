"""Generate a 26 x 26 ChArUco target with DICT_4X4_1000 at 1.5 mm cell size.

Layout: 26 × 26 squares of 1.5 mm → 39 × 39 mm metrology surface (≈338
ArUco markers placed on the white squares per the OpenCV ChArUco
convention). The page is a tight 50 × 50 mm custom size so the
hardware-handoff DXF carries a compact bbox.

This board is intended for chrome-on-glass photolithography. With a
1.5 mm cell and ``marker_size_rel = 0.75`` the marker side is 1.125 mm,
each bit cell is 0.1875 mm (≈188 µm) — well within photolith feature
limits but far below what printing or laser cutting can reproduce.

Usage::

    uv run python crates/calib-targets-py/examples/generate_charuco_26x26_4x4_1000.py \\
        [output_stem]

Defaults to ``tmpdata/printable/charuco_26x26_4x4_1000``. The script
writes four files (``stem.json``, ``stem.svg``, ``stem.png``,
``stem.dxf``); the SVG and DXF are the production-relevant artifacts,
the JSON is the spec for reproducibility, and the PNG is a low-DPI
visual check.
"""

from __future__ import annotations

import sys
from pathlib import Path

import calib_targets as ct


# Custom 50 × 50 mm page with a 5 mm margin centers the 39 × 39 mm board
# on a tight photomask-sized substrate. The default A4 would also work
# but ships ~80 KB of empty whitespace in the SVG bbox.
PAGE_WIDTH_MM = 50.0
PAGE_HEIGHT_MM = 50.0
PAGE_MARGIN_MM = 5.0


def build_document() -> ct.PrintableTargetDocument:
    doc = ct.charuco_document(
        rows=26,
        cols=26,
        square_size_mm=1.5,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_1000",
    )
    doc.page = ct.PageSpec(
        size=ct.PageSize.custom(width_mm=PAGE_WIDTH_MM, height_mm=PAGE_HEIGHT_MM),
        margin_mm=PAGE_MARGIN_MM,
    )
    # 600 DPI keeps the PNG visual-check readable at this fine pitch
    # (each cell is ~35 px wide at 600 DPI).
    doc.render = ct.RenderOptions(png_dpi=600)
    return doc


def main() -> None:
    out_stem = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else Path("tmpdata/printable/charuco_26x26_4x4_1000")
    )
    doc = build_document()
    written = ct.write_target_bundle(doc, out_stem)
    print(written.json_path)
    print(written.svg_path)
    print(written.png_path)
    print(written.dxf_path)


if __name__ == "__main__":
    main()
