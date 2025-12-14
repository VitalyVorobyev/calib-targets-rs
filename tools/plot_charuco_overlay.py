#!/usr/bin/env python3
"""
Visualize ChArUco detections produced by `calib-targets-charuco` example.

Usage:
  python tools/plot_charuco_overlay.py path/to/charuco_detect_report.json [-o overlay.png] [--show]
"""

import argparse
import json
from pathlib import Path

import matplotlib.image as mpimg
import matplotlib.pyplot as plt


def draw_charuco(ax, corners):
    for c in corners:
        ax.scatter(
            c["x"],
            c["y"],
            s=28,
            facecolors="none",
            edgecolors="lime",
            linewidths=1.2,
            alpha=0.9,
        )
        cid = c.get("id")
        if cid is not None:
            ax.annotate(
                str(cid),
                (c["x"], c["y"]),
                xytext=(3, -3),
                textcoords="offset points",
                fontsize=8,
                color="yellow",
                bbox=dict(
                    boxstyle="round,pad=0.2",
                    facecolor="black",
                    edgecolor="none",
                    alpha=0.6,
                ),
            )


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


def draw_markers(ax, markers):
    for m in markers:
        pts = m.get("corners_img") or m.get("corners_rect")
        if not pts:
            continue
        xs = [p[0] for p in pts] + [pts[0][0]]
        ys = [p[1] for p in pts] + [pts[0][1]]
        ax.plot(xs, ys, color="cyan", linewidth=1.0, alpha=0.8)

        center = m.get("center_img") or m.get("center_rect")
        if center:
            ax.scatter(
                center[0],
                center[1],
                s=26,
                facecolors="cyan",
                edgecolors="black",
                linewidths=0.6,
                alpha=0.8,
                zorder=5,
            )
            label_pos = (center[0], center[1])
        else:
            label_pos = (pts[0][0], pts[0][1])

        ax.annotate(
            str(m.get("id", "?")),
            label_pos,
            xytext=(2, -2),
            textcoords="offset points",
            fontsize=8,
            color="cyan",
            bbox=dict(
                boxstyle="round,pad=0.2",
                facecolor="black",
                edgecolor="none",
                alpha=0.5,
            ),
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="JSON report from the Rust example")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        help="Where to save the overlay PNG (defaults to <report>_overlay.png)",
    )
    parser.add_argument(
        "--show", action="store_true", help="Show an interactive window as well"
    )
    args = parser.parse_args()

    data = json.loads(args.report.read_text())

    image_path = Path(data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    img = mpimg.imread(str(image_path))

    chessboard = (data.get("chessboard") or {}).get("corners") or []
    charuco = (data.get("charuco") or {}).get("corners") or []
    markers = data.get("markers") or []

    fig, ax = plt.subplots(figsize=(10, 8))
    ax.imshow(img, cmap="gray", origin="upper")

    if chessboard:
        draw_chessboard(ax, chessboard)
    if charuco:
        draw_charuco(ax, charuco)
    if markers:
        draw_markers(ax, markers)

    ax.set_title("ChArUco detection overlay")
    ax.set_axis_off()

    out_path = args.output or args.report.with_name(
        args.report.stem + "_overlay.png"
    )
    plt.savefig(out_path, bbox_inches="tight", dpi=200)
    print(f"overlay saved to {out_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
