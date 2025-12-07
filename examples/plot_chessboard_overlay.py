#!/usr/bin/env python3
"""
Plot an overlay of detected chessboard corners on top of the input image.

Usage:
  python examples/plot_chessboard_overlay.py testdata/chessboard_detection.json
"""

import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.image as mpimg


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Visualize chessboard detections from the Rust example."
    )
    parser.add_argument(
        "detections_json",
        type=Path,
        help="Path to detection JSON produced by examples/chessboard.rs",
    )
    args = parser.parse_args()

    data = json.loads(args.detections_json.read_text())

    image_path = Path(data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    detections = data.get("detections", [])

    img = mpimg.imread(str(image_path))

    fig, ax = plt.subplots()
    ax.imshow(img, cmap="gray", origin="upper")

    for det in detections:
        corners = det.get("corners", [])
        if not corners:
            continue

        xs = [c["x"] for c in corners]
        ys = [c["y"] for c in corners]

        ax.scatter(xs, ys, s=20, facecolors="none", edgecolors="r", linewidths=1.0)

    ax.set_title(f"Detected corners: {sum(len(d.get('corners', [])) for d in detections)}")
    ax.set_xlabel("x (pixels)")
    ax.set_ylabel("y (pixels)")

    plt.tight_layout()
    plt.show()


if __name__ == "__main__":
    main()

