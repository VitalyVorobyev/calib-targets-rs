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

    # Three configs with different ChESS thresholds.
    base = ct.ChessboardParams()
    high = ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.15))
    low = ct.ChessboardParams(chess=ct.ChessConfig(threshold_value=0.08))

    result = ct.detect_chessboard_best(image, [base, high, low])

    if result is None:
        print("No chessboard detected with any config")
        return

    print(f"corners: {len(result.detection.corners)}")


if __name__ == "__main__":
    main()
