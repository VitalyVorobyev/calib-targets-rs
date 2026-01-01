import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_marker_board.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    params = {
        "layout": {
            "rows": 22,
            "cols": 22,
            "cell_size": 1.0,
            "circles": [
                {"cell": {"i": 11, "j": 11}, "polarity": "white"},
                {"cell": {"i": 12, "j": 11}, "polarity": "black"},
                {"cell": {"i": 12, "j": 12}, "polarity": "white"},
            ],
        }
    }

    result = calib_targets.detect_marker_board(image, params=params)

    if result is None:
        print("No marker board detected")
        return
    
    print(result.keys())
    print(result['detection']['corners'][0])

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"inliers: {len(result.get('inliers', []))}")


if __name__ == "__main__":
    main()
