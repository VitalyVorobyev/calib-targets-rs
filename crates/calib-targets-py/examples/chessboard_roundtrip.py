"""End-to-end chessboard roundtrip in Python.

Synthesises a chessboard via the printable-target pipeline, decodes the PNG
bytes into a grayscale numpy array, detects the board with a default
multi-config sweep, and writes the detection result to JSON.

Run:

    uv run python crates/calib-targets-py/examples/chessboard_roundtrip.py

Options:
    --inner-rows N  Number of inner corner rows (default 7)
    --inner-cols N  Number of inner corner cols (default 9)
    --square-mm F   Physical square size in mm (default 20.0)
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
    parser.add_argument("--inner-rows", type=int, default=7)
    parser.add_argument("--inner-cols", type=int, default=9)
    parser.add_argument("--square-mm", type=float, default=20.0)
    parser.add_argument("--dpi", type=int, default=150)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    # 1. Synthesise a chessboard at a custom page size that fits the board.
    w_mm = (args.inner_cols + 1) * args.square_mm + 20.0
    h_mm = (args.inner_rows + 1) * args.square_mm + 20.0
    doc = ct.PrintableTargetDocument(
        target=ct.ChessboardTargetSpec(
            inner_rows=args.inner_rows,
            inner_cols=args.inner_cols,
            square_size_mm=args.square_mm,
        ),
        page=ct.PageSpec(
            size=ct.PageSize.custom(width_mm=w_mm, height_mm=h_mm),
            margin_mm=10.0,
        ),
        render=ct.RenderOptions(png_dpi=args.dpi),
    )
    bundle = ct.render_target_bundle(doc)
    print(
        f"synthesised {args.inner_rows}x{args.inner_cols} chessboard "
        f"({len(bundle.png_bytes) // 1024} KB PNG)"
    )

    # 2. Decode PNG -> grayscale numpy array.
    image = np.asarray(
        Image.open(io.BytesIO(bundle.png_bytes)).convert("L"),
        dtype=np.uint8,
    )

    # 3. Detect with a small multi-config sweep. detect_chessboard_best picks
    #    the config that labels the most corners.
    configs = [
        ct.ChessboardParams(),
        ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.15)),
        ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.08)),
    ]
    result = ct.detect_chessboard_best(image, configs)
    if result is None:
        print("no chessboard detected", file=sys.stderr)
        return 1
    expected = args.inner_rows * args.inner_cols
    print(
        f"detected {len(result.detection.corners)} labelled corners "
        f"(expected {expected}; cell size {result.cell_size:.1f}px)"
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
