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


def parse_point(value):
    if isinstance(value, dict):
        if "x" in value and "y" in value:
            return [value["x"], value["y"]]
        if "coords" in value:
            coords = value["coords"]
            if isinstance(coords, list):
                if len(coords) == 2 and all(isinstance(v, (int, float)) for v in coords):
                    return coords
                if len(coords) == 1 and isinstance(coords[0], list) and len(coords[0]) == 2:
                    return coords[0]
    if isinstance(value, list):
        if len(value) == 2 and all(isinstance(v, (int, float)) for v in value):
            return value
        if len(value) == 1 and isinstance(value[0], list) and len(value[0]) == 2:
            return value[0]
        if len(value) == 2 and all(isinstance(v, list) and len(v) == 1 for v in value):
            return [value[0][0], value[1][0]]
    return None


def parse_cell(value):
    if isinstance(value, dict) and "i" in value and "j" in value:
        return [value["i"], value["j"]]
    if isinstance(value, list) and len(value) == 2:
        return value
    return None


def draw_chessboard(ax, corners):
    for c in corners:
        center = parse_point(c.get("position")) if isinstance(c, dict) else None
        if center is None and isinstance(c, dict):
            center = [c.get("x"), c.get("y")]
        if center is None:
            continue
        ax.scatter(
            center[0],
            center[1],
            s=14,
            facecolors="none",
            edgecolors="green",
            linewidths=0.8,
            alpha=0.9,
        )


def draw_circles(ax, candidates, matches):
    pol_color = {"white": "cyan", "black": "magenta"}
    if len(matches) == 0:
        for idx, c in enumerate(candidates):
            color = pol_color.get(c.get("polarity"), "yellow")
            center = parse_point(c.get("center_img")) if isinstance(c, dict) else None
            if center is None:
                continue
            ax.scatter(
                center[0],
                center[1],
                s=32,
                facecolors="none",
                edgecolors=color,
                linewidths=1.2,
                alpha=0.9,
            )
            ax.annotate(
                str(idx),
                (center[0], center[1]),
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
        if not isinstance(m, dict):
            continue
        matched_index = m.get("matched_index")
        if matched_index is None:
            continue
        center = None
        if "center_img" in m:
            center = parse_point(m.get("center_img"))
        if center is None and 0 <= matched_index < len(candidates):
            center = parse_point(candidates[matched_index].get("center_img"))
        if center is None:
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
        expected_cell = None
        if "expected_cell" in m:
            expected_cell = parse_cell(m.get("expected_cell"))
        elif "expected" in m:
            expected_cell = parse_cell(m.get("expected", {}).get("cell"))
        ax.annotate(
            f"cell {expected_cell}",
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

    detection = data.get("detection")
    if detection:
        chessboard = (detection.get("detection") or {}).get("corners") or []
        candidates = detection.get("circle_candidates") or []
        matches = detection.get("circle_matches") or []
    else:
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
    fig.tight_layout()

    out_path = args.output or args.report.with_name(args.report.stem + "_overlay.png")
    plt.savefig(out_path, bbox_inches="tight", dpi=200)
    print(f"overlay saved to {out_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
