from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: detect_puzzleboard.py <image_path>", file=sys.stderr)
        return 2

    image = load_gray(Path(sys.argv[1]))
    params = ct.default_puzzleboard_params(10, 10)
    result = ct.detect_puzzleboard(image, params=params)
    print(
        f"detected {len(result.detection.corners)} corners, "
        f"mean_confidence={result.decode.mean_confidence:.3f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
