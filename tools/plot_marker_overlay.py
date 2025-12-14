#!/usr/bin/env python3
"""
Visualize checkerboard-marker detections from the marker_detect example.

Usage:
  python tools/plot_marker_overlay.py tmpdata/marker_detect_report.json [-o overlay.png] [--show]
"""

import argparse
import json
from pathlib import Path

import matplotlib.image as mpimg
import matplotlib.pyplot as plt


def draw_chessboard(ax, corners):
    for c in corners:
        ax.scatter(
            c["x"],
            c["y"],
            s=14,
            facecolors="none",
            edgecolors="0.6",
            linewidths=0.6,
            alpha=0.7,
        )


def draw_circles(ax, candidates, matches):
    pol_color = {"white": "cyan", "black": "magenta"}
    for idx, c in enumerate(candidates):
        color = pol_color.get(c.get("polarity"), "yellow")
        ax.scatter(
            c["center_img"][0],
            c["center_img"][1],
            s=32,
            facecolors="none",
            edgecolors=color,
            linewidths=1.2,
            alpha=0.9,
        )
        ax.annotate(
            str(idx),
            (c["center_img"][0], c["center_img"][1]),
            xytext=(3, -3),
            textcoords="offset points",
            fontsize=8,
            color=color,
            bbox=dict(
                boxstyle="round,pad=0.2",
                facecolor="black",
                edgecolor="none",
                alpha=0.5,
            ),
        )

    for m in matches:
        if m.get("matched_index") is None:
            continue
        center = m.get("center_img")
        if not center:
            continue
        ax.scatter(
            center[0],
            center[1],
            s=64,
            marker="x",
            color="lime",
            linewidths=1.6,
            alpha=0.9,
            zorder=5,
        )
        ax.annotate(
            f"cell {m['expected_cell']}",
            (center[0], center[1]),
            xytext=(4, 4),
            textcoords="offset points",
            fontsize=8,
            color="lime",
            bbox=dict(
                boxstyle="round,pad=0.2",
                facecolor="black",
                edgecolor="none",
                alpha=0.4,
            ),
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="JSON report from marker_detect example")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        help="Where to save the overlay PNG (defaults to <report>_overlay.png)",
    )
    parser.add_argument("--show", action="store_true", help="Show an interactive window as well")
    args = parser.parse_args()

    data = json.loads(args.report.read_text())

    image_path = Path(data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    chessboard = (data.get("chessboard") or {}).get("corners") or []
    candidates = data.get("circle_candidates") or []
    matches = data.get("matches") or []

    img = mpimg.imread(str(image_path))

    fig, ax = plt.subplots(figsize=(10, 8))
    ax.imshow(img, cmap="gray", origin="upper")

    if chessboard:
        draw_chessboard(ax, chessboard)
    if candidates:
        draw_circles(ax, candidates, matches)

    ax.set_title("Marker board detection overlay")
    ax.set_axis_off()

    out_path = args.output or args.report.with_name(args.report.stem + "_overlay.png")
    plt.savefig(out_path, bbox_inches="tight", dpi=200)
    print(f"overlay saved to {out_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
