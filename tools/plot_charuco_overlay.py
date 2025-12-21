#!/usr/bin/env python3
"""
Visualize ChArUco detections produced by `calib-targets-charuco` example.

Usage:
  python tools/plot_charuco_overlay.py path/to/charuco_detect_report.json [-o overlay.png] [--show]

Notes:
  - The report now stores standard crate types. Corner positions live in
    `position: [x, y]` arrays.
  - Marker corners are in rectified-grid coordinates unless `corners_img`
    is present. If a rectified image path is available, this script will
    generate a second overlay for markers in rectified space.
"""

import argparse
import json
from pathlib import Path

import matplotlib.image as mpimg
import matplotlib.pyplot as plt


def resolve_path(report_path: Path, path_str: str) -> Path:
    p = Path(path_str)
    if p.is_absolute():
        return p
    return (report_path.parent / p).resolve()


def corner_xy(c):
    pos = c.get("position")
    if pos and isinstance(pos, (list, tuple)) and len(pos) >= 2:
        return pos[0], pos[1]
    return c.get("x"), c.get("y")


def draw_charuco(ax, corners):
    for c in corners:
        x, y = corner_xy(c)
        if x is None or y is None:
            continue
        ax.scatter(
            x,
            y,
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
                (x, y),
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
        x, y = corner_xy(c)
        if x is None or y is None:
            continue
        ax.scatter(
            x,
            y,
            s=14,
            facecolors="none",
            edgecolors="0.6",
            linewidths=0.6,
            alpha=0.7,
        )


def polygon_center(pts):
    if not pts:
        return None
    cx = sum(p[0] for p in pts) / len(pts)
    cy = sum(p[1] for p in pts) / len(pts)
    return [cx, cy]


def marker_corners(m, prefer_img):
    if prefer_img:
        pts = m.get("corners_img")
        if pts:
            return pts
        return None
    pts = m.get("corners_rect")
    if pts:
        return pts
    return m.get("corners_img")


def draw_markers(ax, markers, prefer_img, color="cyan"):
    for m in markers:
        pts = marker_corners(m, prefer_img=prefer_img)
        if not pts:
            continue
        xs = [p[0] for p in pts] + [pts[0][0]]
        ys = [p[1] for p in pts] + [pts[0][1]]
        ax.plot(xs, ys, color=color, linewidth=1.0, alpha=0.8)

        center = m.get("center_img") if prefer_img else None
        if center is None:
            center = polygon_center(pts)
        if center is not None:
            ax.scatter(
                center[0],
                center[1],
                s=26,
                facecolors=color,
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
            color=color,
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

    image_path = resolve_path(args.report, data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    img = mpimg.imread(str(image_path))

    chessboard = (data.get("chessboard") or {}).get("corners") or []
    charuco = (data.get("charuco") or {}).get("corners") or []
    markers = data.get("markers") or []
    rectified_path_raw = (data.get("rectified") or {}).get("path")
    rectified_path = (
        resolve_path(args.report, rectified_path_raw)
        if rectified_path_raw
        else None
    )

    fig, ax = plt.subplots(figsize=(10, 8))
    ax.imshow(img, cmap="gray", origin="upper")

    if chessboard:
        draw_chessboard(ax, chessboard)
    if charuco:
        draw_charuco(ax, charuco)
    markers_img = [
        m for m in markers if m.get("corners_img") or m.get("center_img")
    ]
    if markers_img:
        draw_markers(ax, markers_img, prefer_img=True)
    elif markers:
        print(
            "markers are in rectified coordinates; "
            "skipping markers on the original image"
        )

    ax.set_title("ChArUco detection overlay")
    ax.set_axis_off()
    fig.tight_layout()

    out_path = args.output or args.report.with_name(
        args.report.stem + "_overlay.png"
    )
    plt.savefig(out_path, bbox_inches="tight", dpi=200)
    print(f"overlay saved to {out_path}")

    if rectified_path and markers:
        if not rectified_path.is_file():
            print(f"rectified image not found: {rectified_path}")
        else:
            rect_img = mpimg.imread(str(rectified_path))
            fig_rect, ax_rect = plt.subplots(figsize=(10, 8))
            ax_rect.imshow(rect_img, cmap="gray", origin="upper")
            draw_markers(ax_rect, markers, prefer_img=False, color="deepskyblue")
            ax_rect.set_title("ChArUco marker overlay (rectified)")
            ax_rect.set_axis_off()
            fig_rect.tight_layout()

            if args.output:
                rect_out = out_path.with_name(out_path.stem + "_rectified" + out_path.suffix)
            else:
                rect_out = args.report.with_name(args.report.stem + "_rectified_overlay.png")
            plt.savefig(rect_out, bbox_inches="tight", dpi=200)
            print(f"rectified overlay saved to {rect_out}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
