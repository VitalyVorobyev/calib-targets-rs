"""End-to-end ChArUco roundtrip in Python.

Synthesises a ChArUco target via the printable-target pipeline, decodes the
PNG bytes into a grayscale numpy array, detects the board with matching
``CharucoParams``, and writes the detection (including marker IDs) to JSON.

Run:

    uv run python crates/calib-targets-py/examples/charuco_roundtrip.py

Options:
    --rows N        Square rows (default 5)
    --cols N        Square cols (default 7)
    --square-mm F   Physical square size in mm (default 25.0)
    --marker-rel F  Marker size relative to square (default 0.75)
    --dict NAME     ArUco dictionary (default DICT_4X4_50)
    --dpi DPI       PNG rasterisation DPI (default 150)
    --out PATH      Write detection JSON to PATH (default prints to stdout)
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
    parser.add_argument("--rows", type=int, default=5)
    parser.add_argument("--cols", type=int, default=7)
    parser.add_argument("--square-mm", type=float, default=25.0)
    parser.add_argument("--marker-rel", type=float, default=0.75)
    parser.add_argument("--dict", dest="dictionary", default="DICT_4X4_50")
    parser.add_argument("--dpi", type=int, default=150)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    # 1. Synthesise the ChArUco target.
    w_mm = args.cols * args.square_mm + 20.0
    h_mm = args.rows * args.square_mm + 20.0
    doc = ct.PrintableTargetDocument(
        target=ct.CharucoTargetSpec(
            rows=args.rows,
            cols=args.cols,
            square_size_mm=args.square_mm,
            marker_size_rel=args.marker_rel,
            dictionary=args.dictionary,
        ),
        page=ct.PageSpec(
            size=ct.PageSize.custom(width_mm=w_mm, height_mm=h_mm),
            margin_mm=10.0,
        ),
        render=ct.RenderOptions(png_dpi=args.dpi),
    )
    bundle = ct.render_target_bundle(doc)
    print(
        f"synthesised {args.rows}x{args.cols} ChArUco ({args.dictionary}) "
        f"({len(bundle.png_bytes) // 1024} KB PNG)"
    )

    # 2. Decode PNG -> grayscale numpy array.
    image = np.asarray(
        Image.open(io.BytesIO(bundle.png_bytes)).convert("L"),
        dtype=np.uint8,
    )

    # 3. Detect. Board spec on the detector side must match the generated
    #    target: same rows/cols/dictionary/marker-size-rel.
    board = ct.CharucoBoardSpec(
        rows=args.rows,
        cols=args.cols,
        cell_size=1.0,
        marker_size_rel=args.marker_rel,
        dictionary=args.dictionary,
        marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
    )
    params = ct.CharucoParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(),
        max_hamming=2,
        min_marker_inliers=4,
    )
    try:
        result = ct.detect_charuco(image, params=params)
    except RuntimeError as exc:
        print(f"detection failed: {exc}", file=sys.stderr)
        return 1
    print(
        f"detected {len(result.detection.corners)} labelled corners, "
        f"{len(result.markers)} markers "
        f"(raw_marker_count={result.raw_marker_count})"
    )

    # 4. Export detection (corners + markers + alignment) to JSON.
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
