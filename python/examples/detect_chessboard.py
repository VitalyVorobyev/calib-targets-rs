import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_chessboard.py <image_path>")
        return

    image = load_gray(sys.argv[1])
    result = calib_targets.detect_chessboard(image)

    if result is None:
        print("No chessboard detected")
        return

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"inliers: {len(result.get('inliers', []))}")


if __name__ == "__main__":
    main()
