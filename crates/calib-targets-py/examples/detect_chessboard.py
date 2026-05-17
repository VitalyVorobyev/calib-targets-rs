"""Chessboard detection example with explicit configuration.

Mirrors the Rust ``calib_targets::detect`` facade: a top-level
``DetectorConfig`` (here ``ct.ChessConfig`` on the Python side) drives
ChESS corner detection, and ``ChessboardParams`` drives the grid /
labelling pipeline. Defaults are sensible for typical inputs; this
example shows where to override them.
"""

import sys

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_chessboard.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    # ChESS-detector configuration (Rust: chess_corners::DetectorConfig).
    # The default is single-scale ChESS + Threshold::Absolute(15.0). Drop
    # the threshold for blurry inputs; raise for clean ones.
    chess_cfg = ct.ChessConfig(
        threshold=ct.Threshold.absolute(15.0),
    )

    # Chessboard grid / labelling pipeline. Sticks to the workspace
    # defaults except for a small pre-filter on corner strength.
    params = ct.ChessboardParams(
        min_corner_strength=0.0,
        min_labeled_corners=16,
    )

    result = ct.detect_chessboard(image, chess_cfg=chess_cfg, params=params)

    if result is None:
        print("No chessboard detected")
        return

    corners = result.detection.corners
    print(f"corners: {len(corners)}")
    if corners:
        print(corners[0])


if __name__ == "__main__":
    main()
