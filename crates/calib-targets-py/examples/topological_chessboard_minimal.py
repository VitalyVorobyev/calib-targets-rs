"""Minimal topological chessboard detection from Python.

Run:
    uv run python crates/calib-targets-py/examples/topological_chessboard_minimal.py IMAGE
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image

import calib_targets as ct


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--threshold", type=float, default=100.0)
    args = parser.parse_args()

    image = np.asarray(Image.open(args.image).convert("L"), dtype=np.uint8)
    chess_cfg = ct.ChessConfig(threshold=ct.Threshold.absolute(args.threshold))
    params = ct.ChessboardParams.for_topological()

    result = ct.detect_chessboard(image, chess_cfg=chess_cfg, params=params)
    if result is None:
        print("no board detected", file=sys.stderr)
        return 1

    print(
        f"detected {len(result.corners)} labelled corners; "
        f"cell size {result.cell_size:.2f}px"
    )
    print("i\tj\tx\ty")
    for corner in result.corners:
        if corner.grid is None:
            continue
        x, y = corner.position
        print(f"{corner.grid.i}\t{corner.grid.j}\t{x:.2f}\t{y:.2f}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
