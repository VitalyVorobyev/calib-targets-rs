"""End-to-end PuzzleBoard roundtrip in Python.

Synthesises a PuzzleBoard target in memory via ``calib_targets`` (no temp
files), decodes the PNG bytes into a grayscale numpy array, detects the
board, and verifies every returned corner has an absolute master ``(I, J)``
label.

Run:

    uv run python crates/calib-targets-py/examples/puzzleboard_roundtrip.py

Options (all optional):
    --rows N   Number of squares vertically (default 10)
    --cols N   Number of squares horizontally (default 10)
    --dpi DPI  PNG rasterisation DPI (default 300)
    --out PATH Write the synthetic PNG to PATH before detection
"""
from __future__ import annotations

import argparse
import io
import sys
from pathlib import Path

import numpy as np
from PIL import Image

import calib_targets as ct


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=10)
    parser.add_argument("--cols", type=int, default=10)
    parser.add_argument("--dpi", type=int, default=300)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    # 1. Synthesise
    target = ct.PuzzleBoardTargetSpec(
        rows=args.rows,
        cols=args.cols,
        square_size_mm=12.0,
    )
    page = ct.PageSpec(
        size=ct.PageSize.custom(
            width_mm=args.cols * 12.0 + 20.0,
            height_mm=args.rows * 12.0 + 20.0,
        ),
        margin_mm=5.0,
    )
    doc = ct.PrintableTargetDocument(
        target=target,
        page=page,
        render=ct.RenderOptions(png_dpi=args.dpi),
    )
    bundle = ct.render_target_bundle(doc)
    print(
        f"synthesised {args.rows}×{args.cols} PuzzleBoard at {args.dpi} DPI "
        f"({len(bundle.png_bytes) // 1024} KB PNG)"
    )
    if args.out is not None:
        args.out.write_bytes(bundle.png_bytes)
        print(f"wrote synthetic target to {args.out}")

    # 2. Decode PNG → grayscale numpy array
    image = np.asarray(
        Image.open(io.BytesIO(bundle.png_bytes)).convert("L"),
        dtype=np.uint8,
    )

    # 3. Detect
    params = ct.default_puzzleboard_params(args.rows, args.cols)
    result = ct.detect_puzzleboard(image, params=params)
    print(
        f"detected {len(result.detection.corners)} labelled corners "
        f"(mean confidence = {result.decode.mean_confidence:.3f}, "
        f"BER = {result.decode.bit_error_rate:.3f})"
    )
    print(
        f"master origin for local (0, 0): "
        f"({result.decode.master_origin_row}, {result.decode.master_origin_col})"
    )

    # 4. Verify
    inner = (args.rows - 1) * (args.cols - 1)
    coverage = len(result.detection.corners) / inner
    print(
        f"coverage: {len(result.detection.corners)}/{inner} inner corners "
        f"labelled ({coverage * 100:.1f}%)"
    )
    seen: set[tuple[int, int]] = set()
    for corner in result.detection.corners:
        if corner.id is None:
            raise AssertionError("missing id")
        if corner.grid is None:
            raise AssertionError("missing grid")
        key = (corner.grid.i, corner.grid.j)
        if key in seen:
            raise AssertionError(f"duplicate master coord {key}")
        seen.add(key)
    print("every labelled corner has a unique master (I, J) and ID")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
