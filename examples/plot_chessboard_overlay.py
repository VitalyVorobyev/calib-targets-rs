#!/usr/bin/env python3
"""
Visualize chessboard detections:
- Overlay detected corners and grid graph on the image.
- Plot orientation histogram and estimated board orientations.

Usage:
  python examples/plot_chessboard_overlay.py testdata/chessboard_detection.json
"""

import argparse
import json
import math
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
    if not detections:
        raise SystemExit("No detections found in JSON")

    det_report = detections[0]
    detection = det_report.get("detection", {})
    corners = detection.get("corners", [])

    grid_graph = det_report.get("grid_graph")
    orientations = det_report.get("orientations")
    orientation_hist = det_report.get("orientation_histogram")

    img = mpimg.imread(str(image_path))

    fig, (ax_overlay, ax_hist) = plt.subplots(1, 2, figsize=(12, 6))
    ax_overlay.imshow(img, cmap="gray", origin="upper")

    if corners:
        xs = [c["x"] for c in corners]
        ys = [c["y"] for c in corners]

        ax_overlay.scatter(xs, ys, s=20, facecolors="none", edgecolors="r", linewidths=1.0)
        for corner in corners:
            grid = corner.get("grid")
            if grid is None:
                continue
            label = f"{grid[0]},{grid[1]}"
            ax_overlay.annotate(
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

    if grid_graph:
        for node in grid_graph.get("nodes", []):
            x0, y0 = node["x"], node["y"]
            for neigh in node.get("neighbors", []):
                idx = neigh["index"]
                # avoid double-drawing by only drawing edges to higher-index nodes
                if idx < node["index"]:
                    continue
                if idx >= len(grid_graph["nodes"]):
                    continue
                x1 = grid_graph["nodes"][idx]["x"]
                y1 = grid_graph["nodes"][idx]["y"]
                ax_overlay.plot([x0, x1], [y0, y1], color="cyan", linewidth=0.8, alpha=0.7)

    if orientations:
        centers = orientations.get("centers_rad")
        if centers:
            h, w = img.shape[0], img.shape[1]
            cx, cy = w / 2.0, h / 2.0
            span = 0.4 * min(h, w)
            colors = ["lime", "orange"]
            for i, theta in enumerate(centers):
                dx = span * math.cos(theta)
                dy = span * math.sin(theta)
                ax_overlay.plot(
                    [cx - dx, cx + dx],
                    [cy - dy, cy + dy],
                    color=colors[i % len(colors)],
                    linewidth=2.0,
                    alpha=0.8,
                )
                ax_overlay.annotate(
                    f"{math.degrees(theta):.1f}°",
                    (cx + dx, cy + dy),
                    fontsize=9,
                    color=colors[i % len(colors)],
                )

    ax_overlay.set_title("Corners + Grid")
    ax_overlay.set_axis_off()

    if orientation_hist:
        bins = orientation_hist.get("bins", [])
        angles_deg = [b["angle_deg"] for b in bins]
        values = [b["value"] for b in bins]
        span = 180.0 / max(len(bins), 1)
        ax_hist.bar(angles_deg, values, width=span, color="lightgray", edgecolor="black")
        if orientations and orientations.get("centers_deg"):
            for theta in orientations["centers_deg"]:
                ax_hist.axvline(theta, color="red", linestyle="--", linewidth=1.5, label="orientation center")
        ax_hist.set_xlabel("Angle (deg)")
        ax_hist.set_ylabel("Smoothed weight")
        ax_hist.set_title("Orientation Histogram")
    else:
        ax_hist.text(0.5, 0.5, "No histogram in JSON", ha="center", va="center")
        ax_hist.set_axis_off()

    fig.tight_layout()

    output_path = args.output or args.detections_json.with_name(
        f"{args.detections_json.stem}_overlay.png"
    )
    fig.savefig(output_path, dpi=200, bbox_inches="tight", pad_inches=0.2)
    print(f"Saved visualization to {output_path}")

    if args.show:
        plt.show()
    else:
        plt.close(fig)


if __name__ == "__main__":
    main()
