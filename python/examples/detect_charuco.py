import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_charuco.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    board = {
        "rows": 22,
        "cols": 22,
        "cell_size": 1.0,
        "marker_size_rel": 0.75,
        "dictionary": "DICT_4X4_1000",
        "marker_layout": "opencv_charuco",
    }

    try:
        result = calib_targets.detect_charuco(image, board=board)
    except RuntimeError as exc:
        print(f"detect_charuco failed: {exc}")
        return

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"markers: {len(result.get('markers', []))}")


if __name__ == "__main__":
    main()
