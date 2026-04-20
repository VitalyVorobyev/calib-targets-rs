"""End-to-end marker-board roundtrip in Python.

Synthesises a checkerboard-with-3-circles target via the printable-target
pipeline, decodes the PNG bytes into a grayscale numpy array, detects the
board with a matching ``MarkerBoardParams``, and writes the detection to
JSON.

Run:

    uv run python crates/calib-targets-py/examples/markerboard_roundtrip.py

Options:
    --inner-rows N  Inner-corner rows (default 5; board has N+1 square rows)
    --inner-cols N  Inner-corner cols (default 7; board has N+1 square cols)
    --square-mm F   Physical square size in mm (default 25.0)
    --dpi DPI       PNG rasterisation DPI (default 150)
    --out PATH      Write detection JSON to PATH (default prints to stdout)

The three marker circles are placed around the board centre as
(white, black, white) by default.
"""
from __future__ import annotations

import argparse
import io
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

import calib_targets as ct


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inner-rows", type=int, default=5)
    parser.add_argument("--inner-cols", type=int, default=7)
    parser.add_argument("--square-mm", type=float, default=25.0)
    parser.add_argument("--dpi", type=int, default=150)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    square_rows = args.inner_rows + 1
    square_cols = args.inner_cols + 1
    ci, cj = square_cols // 2, square_rows // 2
    circles = (
        ct.MarkerCircleSpec(i=ci,     j=cj,     polarity=ct.CirclePolarity.WHITE),
        ct.MarkerCircleSpec(i=ci + 1, j=cj,     polarity=ct.CirclePolarity.BLACK),
        ct.MarkerCircleSpec(i=ci + 1, j=cj + 1, polarity=ct.CirclePolarity.WHITE),
    )

    # 1. Synthesise the marker board.
    w_mm = square_cols * args.square_mm + 20.0
    h_mm = square_rows * args.square_mm + 20.0
    doc = ct.PrintableTargetDocument(
        target=ct.MarkerBoardTargetSpec(
            inner_rows=args.inner_rows,
            inner_cols=args.inner_cols,
            square_size_mm=args.square_mm,
            circles=circles,
        ),
        page=ct.PageSpec(
            size=ct.PageSize.custom(width_mm=w_mm, height_mm=h_mm),
            margin_mm=10.0,
        ),
        render=ct.RenderOptions(png_dpi=args.dpi),
    )
    bundle = ct.render_target_bundle(doc)
    print(
        f"synthesised {square_rows}x{square_cols} marker board "
        f"({len(bundle.png_bytes) // 1024} KB PNG)"
    )

    # 2. Decode PNG -> grayscale numpy array.
    image = np.asarray(
        Image.open(io.BytesIO(bundle.png_bytes)).convert("L"),
        dtype=np.uint8,
    )

    # 3. Detect. Layout on the detector side must match the generated target.
    layout = ct.MarkerBoardLayout(
        rows=square_rows,
        cols=square_cols,
        cell_size=1.0,
        circles=circles,
    )
    params = ct.MarkerBoardParams(
        layout=layout,
        chessboard=ct.ChessboardParams(),
    )
    result = ct.detect_marker_board(image, params=params)
    if result is None:
        print("no marker board detected", file=sys.stderr)
        return 1
    print(
        f"detected {len(result.detection.corners)} labelled corners, "
        f"{len(result.circle_matches)} marker-circle matches, "
        f"alignment_inliers={result.alignment_inliers}"
    )

    # 4. Export detection to JSON.
    payload = json.dumps(result.to_dict(), indent=2)
    if args.out is not None:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(payload)
        print(f"wrote detection to {args.out}")
    else:
        print(payload[:200] + "\n  ...  (truncated)" if len(payload) > 200 else payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
