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

    # Three configs bracketing the workspace ChESS threshold default
    # (absolute 15.0). Lower floor for blurry inputs, higher for clean.
    base = ct.ChessboardParams()
    loose = ct.ChessboardParams(
        chess=ct.ChessConfig(threshold=ct.Threshold.absolute(8.0)),
    )
    tight = ct.ChessboardParams(
        chess=ct.ChessConfig(threshold=ct.Threshold.absolute(25.0)),
    )

    result = ct.detect_chessboard_best(image, [base, loose, tight])

    if result is None:
        print("No chessboard detected with any config")
        return

    print(f"corners: {len(result.detection.corners)}")


if __name__ == "__main__":
    main()
