"""Multi-config chessboard sweep example.

Tries three threshold configs and returns the result with the most corners.
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
        print("Usage: detect_chessboard_best.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    # ChESS corner detection runs once; the sweep varies only the
    # chessboard-grid detector parameters.
    chess_cfg = ct.ChessConfig(threshold=ct.Threshold.absolute(15.0))
    base = ct.ChessboardParams()
    permissive = ct.ChessboardParams(min_labeled_corners=12)
    single = ct.ChessboardParams(max_components=1)

    result = ct.detect_chessboard_best(
        image, [base, permissive, single], chess_cfg=chess_cfg
    )

    if result is None:
        print("No chessboard detected with any config")
        return

    print(f"corners: {len(result.corners)}")


if __name__ == "__main__":
    main()
