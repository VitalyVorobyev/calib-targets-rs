#!/usr/bin/env python3
"""
Visualize chessboard detections:
- Overlay detected corners and grid graph on the image.

Usage:
  python examples/plot_chessboard_overlay.py testdata/chessboard_detection.json
"""

import argparse
import json
import math
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.image as mpimg


def finite_float(value):
    try:
        val = float(value)
    except (TypeError, ValueError):
        return None
    if not math.isfinite(val):
        return None
    return val


def plot_overlay(fig, ax, img, corners, grid_graph, orientations, show_colorbar=True):
    ax.imshow(img, cmap="gray", origin="upper")

    if corners:
        xs = []
        ys = []
        scores = []
        scores_ok = True
        for corner in corners:
            pos = corner.get("position")
            if pos is not None:
                x, y = pos[0], pos[1]
            else:
                x, y = corner.get("x"), corner.get("y")
            xs.append(x)
            ys.append(y)
            score = finite_float(corner.get("score"))
            if score is None:
                scores_ok = False
            scores.append(score)

        if scores_ok and scores:
            scatter = ax.scatter(
                xs,
                ys,
                s=20,
                c=scores,
                cmap="plasma",
                edgecolors="white",
                linewidths=0.6,
                alpha=0.9,
            )
            if show_colorbar:
                cbar = fig.colorbar(scatter, ax=ax, fraction=0.046, pad=0.04)
                cbar.set_label("Score", rotation=270, labelpad=12)
        else:
            ax.scatter(xs, ys, s=20, facecolors="none", edgecolors="r", linewidths=1.0)

        for corner in corners:
            grid = corner.get("grid")
            if grid is None:
                continue
            if isinstance(grid, dict):
                gi = grid.get("i")
                gj = grid.get("j")
                if gi is None or gj is None:
                    continue
                label = f"{gi},{gj}"
            else:
                label = f"{grid[0]},{grid[1]}"
            pos = corner.get("position")
            if pos is not None:
                x, y = pos[0], pos[1]
            else:
                x, y = corner.get("x"), corner.get("y")
            ax.annotate(
                label,
                (x, y),
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
                ax.plot([x0, x1], [y0, y1], color="cyan", linewidth=0.8, alpha=0.7)

    if orientations:
        if isinstance(orientations, dict):
            centers = orientations.get("centers_rad")
        else:
            centers = orientations
        if centers:
            h, w = img.shape[0], img.shape[1]
            cx, cy = w / 2.0, h / 2.0
            span = 0.4 * min(h, w)
            colors = ["lime", "orange"]
            for i, theta in enumerate(centers):
                dx = span * math.cos(theta)
                dy = span * math.sin(theta)
                ax.plot(
                    [cx - dx, cx + dx],
                    [cy - dy, cy + dy],
                    color=colors[i % len(colors)],
                    linewidth=2.0,
                    alpha=0.8,
                )
                ax.annotate(
                    f"{math.degrees(theta):.1f}°",
                    (cx + dx, cy + dy),
                    fontsize=9,
                    color=colors[i % len(colors)],
                )

    ax.set_axis_off()

def make_simple_figure(img, dpi=100):
    h, w = img.shape[0], img.shape[1]
    fig = plt.figure(figsize=(w / dpi, h / dpi), dpi=dpi)
    ax = fig.add_axes([0.0, 0.0, 1.0, 1.0])
    return fig, ax


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
        help="Where to save the PNG overlay (defaults to tmpdata/<detections_json>_overlay.png)",
    )
    parser.add_argument(
        "--show",
        action="store_true",
        help="Also show the overlay window in addition to saving the PNG",
    )
    parser.add_argument(
        "--simple",
        action="store_true",
        help="Save just the input image with overlay (no padding or colorbar)",
    )
    args = parser.parse_args()

    data = json.loads(args.detections_json.read_text())

    image_path = Path(data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    if "detection" in data:
        detection = data.get("detection") or {}
        corners = detection.get("corners", []) if isinstance(detection, dict) else []
        grid_graph = data.get("grid_graph")
        orientations = data.get("orientations")
    else:
        detections = data.get("detections", [])
        if not detections:
            raise SystemExit("No detections found in JSON")
        det_report = detections[0]
        detection = det_report.get("detection", {}) or {}
        corners = detection.get("corners", [])
        grid_graph = det_report.get("grid_graph")
        orientations = det_report.get("orientations")

    img = mpimg.imread(str(image_path))

    if args.simple:
        fig, ax_overlay = make_simple_figure(img)
    else:
        fig, ax_overlay = plt.subplots(1, 1, figsize=(8, 5))
    plot_overlay(
        fig,
        ax_overlay,
        img,
        corners,
        grid_graph,
        None,
        show_colorbar=not args.simple,
    )

    if not args.simple:
        fig.tight_layout()

    suff = '_simple' if args.simple else ''
    output_path = args.output or (
        Path("tmpdata") / f"{args.detections_json.stem}_overlay{suff}.png"
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if args.simple:
        fig.savefig(output_path, dpi=fig.dpi, bbox_inches=None, pad_inches=0)
    else:
        fig.savefig(output_path, dpi=200, bbox_inches="tight", pad_inches=0.2)
    print(f"Saved visualization to {output_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
