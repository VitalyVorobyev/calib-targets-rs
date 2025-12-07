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
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        help="Where to save the PNG overlay (defaults to <detections_json>_overlay.png)",
    )
    parser.add_argument(
        "--show",
        action="store_true",
        help="Also show the overlay window in addition to saving the PNG",
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
        for corner in corners:
            grid = corner.get("grid")
            if grid is None:
                continue
            label = f"{grid[0]},{grid[1]}"
            ax.annotate(
                label,
                (corner["x"], corner["y"]),
                xytext=(3, -3),
                textcoords="offset points",
                fontsize=8,
                color="yellow",
                ha="left",
                va="top",
                bbox=dict(
                    boxstyle="round,pad=0.2",
                    facecolor="black",
                    edgecolor="none",
                    alpha=0.5,
                ),
            )

    ax.set_axis_off()
    fig.subplots_adjust(left=0, right=1, top=1, bottom=0)

    output_path = args.output or args.detections_json.with_name(
        f"{args.detections_json.stem}_overlay.png"
    )
    fig.savefig(output_path, dpi=200, bbox_inches="tight", pad_inches=0)
    print(f"Saved overlay to {output_path}")

    if args.show:
        plt.show()
    else:
        plt.close(fig)


if __name__ == "__main__":
    main()
