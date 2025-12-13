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


def finite_float(value):
    try:
        val = float(value)
    except (TypeError, ValueError):
        return None
    if not math.isfinite(val):
        return None
    return val


def plot_overlay(fig, ax, img, corners, grid_graph, orientations):
    ax.imshow(img, cmap="gray", origin="upper")

    corner_confidences = []

    if corners:
        xs = [c["x"] for c in corners]
        ys = [c["y"] for c in corners]

        overlay_confidences = []
        for corner in corners:
            conf = finite_float(corner.get("confidence"))
            if conf is not None:
                conf = min(max(conf, 0.0), 1.0)
                corner_confidences.append(conf)
                if overlay_confidences is not None:
                    overlay_confidences.append(conf)
            elif overlay_confidences is not None:
                overlay_confidences = None

        if overlay_confidences is not None and overlay_confidences:
            scatter = ax.scatter(
                xs,
                ys,
                s=24,
                c=overlay_confidences,
                cmap="plasma",
                vmin=0.0,
                vmax=1.0,
                edgecolors="white",
                linewidths=0.6,
                alpha=0.9,
            )
            cbar = fig.colorbar(scatter, ax=ax, fraction=0.046, pad=0.04)
            cbar.set_label("Confidence", rotation=270, labelpad=12)
        else:
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
        centers = orientations.get("centers_rad")
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

    ax.set_title("Corners + Grid")
    ax.set_axis_off()
    return corner_confidences


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
    raw_corners = data.get("raw_corners") or []

    img = mpimg.imread(str(image_path))

    fig, (ax_overlay, ax_hist, ax_conf) = plt.subplots(1, 3, figsize=(18, 6))

    all_confidences = []

    if raw_corners:
        raw_strengths = []
        for rc in raw_corners:
            strength = finite_float(rc.get("strength"))
            if strength is not None:
                raw_strengths.append(strength)

        max_strength = max(raw_strengths) if raw_strengths else 0.0
        for rc in raw_corners:
            conf = finite_float(rc.get("confidence"))
            if conf is None:
                strength = finite_float(rc.get("strength"))
                if strength is not None and max_strength > 0.0:
                    conf = strength / max_strength

            if conf is not None:
                all_confidences.append(min(max(conf, 0.0), 1.0))

    corner_confidences = plot_overlay(fig, ax_overlay, img, corners, grid_graph, orientations)

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

    hist_confidences = all_confidences if all_confidences else corner_confidences

    if hist_confidences:
        ax_conf.hist(
            hist_confidences,
            bins=20,
            range=(0.0, 1.0),
            color="skyblue",
            edgecolor="black",
        )
        ax_conf.set_xlim(0.0, 1.0)
        ax_conf.set_xlabel("Corner confidence")
        ax_conf.set_ylabel("Count")
        title = (
            "Confidence Distribution (all corners)"
            if all_confidences
            else "Confidence Distribution (detected grid)"
        )
        ax_conf.set_title(title)
        ax_conf.grid(axis="y", alpha=0.25)
    else:
        ax_conf.text(0.5, 0.5, "No confidences in JSON", ha="center", va="center")
        ax_conf.set_axis_off()

    fig.tight_layout()

    output_path = args.output or args.detections_json.with_name(
        f"{args.detections_json.stem}_overlay.png"
    )
    fig.savefig(output_path, dpi=200, bbox_inches="tight", pad_inches=0.2)
    print(f"Saved visualization to {output_path}")
    plt.close(fig)

    if args.show:
        h, w = img.shape[0], img.shape[1]
        aspect = h / w if w > 0 else 1.0
        overlay_fig, overlay_ax = plt.subplots(
            figsize=(8, 8 * aspect),
            constrained_layout=True,
        )
        plot_overlay(overlay_fig, overlay_ax, img, corners, grid_graph, orientations)
        plt.show()
        plt.close(overlay_fig)


if __name__ == "__main__":
    main()
