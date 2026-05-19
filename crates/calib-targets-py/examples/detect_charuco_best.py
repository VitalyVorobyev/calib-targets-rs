"""Multi-config ChArUco sweep example.

Tries three threshold configs and returns the result with the most markers,
breaking ties by corner count.
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
        print("Usage: detect_charuco_best.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    board = ct.CharucoBoardSpec(
        rows=22,
        cols=22,
        cell_size=1.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_1000",
        marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
    )

    # ChESS corner detection runs once in detect_charuco_best; the sweep
    # varies only ChArUco / chessboard-grid detector parameters.
    base = ct.CharucoParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=0.5),
        max_hamming=2,
        min_marker_inliers=8,
    )
    loose = ct.CharucoParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=0.0),
        max_hamming=2,
        min_marker_inliers=4,
    )
    tight = ct.CharucoParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=1.0),
        max_hamming=1,
        min_marker_inliers=8,
    )

    try:
        result = ct.detect_charuco_best(image, [base, loose, tight])
    except RuntimeError as exc:
        print(f"all configs failed: {exc}")
        return

    print(f"corners: {len(result.corners)}")
    print(f"markers: {len(result.markers)}")


if __name__ == "__main__":
    main()
