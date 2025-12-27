#!/usr/bin/env python3
"""
Visualize ChArUco detections produced by `calib-targets-charuco` example.

Usage:
  python tools/plot_charuco_overlay.py path/to/charuco_detect_report.json [-o overlay.png] [--show]

Notes:
  - Updated reports store the detection under `detection` with per-corner
    `grid`, `id`, and `target_position` (mm).
  - Markers are shown on the image only when `corners_img` (or `center_img`)
    is present in the report.
"""

import argparse
import json
from pathlib import Path

import matplotlib.image as mpimg
import matplotlib.pyplot as plt
from matplotlib.colors import Normalize
import matplotlib.patheffects as pe


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


def grid_ij(c):
    g = c.get("grid")
    if isinstance(g, dict):
        return g.get("i"), g.get("j")
    if isinstance(g, (list, tuple)) and len(g) >= 2:
        return g[0], g[1]
    return None, None


def target_xy(c):
    pos = c.get("target_position")
    if pos and isinstance(pos, (list, tuple)) and len(pos) >= 2:
        return pos[0], pos[1]
    return None, None


def build_grid_map(corners):
    out = {}
    for c in corners:
        i, j = grid_ij(c)
        if i is None or j is None:
            continue
        x, y = corner_xy(c)
        if x is None or y is None:
            continue
        out[(int(i), int(j))] = (float(x), float(y))
    return out


def draw_grid_edges(ax, grid_map, color, linewidth=0.8, alpha=0.6):
    for (i, j), p in grid_map.items():
        right = grid_map.get((i + 1, j))
        down = grid_map.get((i, j + 1))
        if right:
            ax.plot(
                [p[0], right[0]],
                [p[1], right[1]],
                color=color,
                linewidth=linewidth,
                alpha=alpha,
            )
        if down:
            ax.plot(
                [p[0], down[0]],
                [p[1], down[1]],
                color=color,
                linewidth=linewidth,
                alpha=alpha,
            )


def label_corner(ax, x, y, text, color, fontsize=7):
    ax.text(
        x,
        y,
        text,
        color=color,
        fontsize=fontsize,
        ha="left",
        va="bottom",
        path_effects=[pe.withStroke(linewidth=2.5, foreground="black", alpha=0.6)],
    )


def format_label(c, mode):
    if mode == "none":
        return None
    cid = c.get("id")
    i, j = grid_ij(c)
    tx, ty = target_xy(c)
    if mode == "id":
        return str(cid) if cid is not None else None
    if mode == "grid":
        return f"{i},{j}" if i is not None and j is not None else None
    if mode == "target":
        if tx is None or ty is None:
            return None
        return f"{tx:.1f},{ty:.1f} mm"
    parts = []
    if cid is not None:
        parts.append(f"id {cid}")
    if i is not None and j is not None:
        parts.append(f"g ({i},{j})")
    if tx is not None and ty is not None:
        parts.append(f"{tx:.1f},{ty:.1f} mm")
    return "\n".join(parts) if parts else None


def extract_detection(data):
    if data.get("detection"):
        return data["detection"]
    if data.get("charuco"):
        return data["charuco"]
    if data.get("chessboard"):
        return data["chessboard"]
    return None


def make_simple_figure(img, dpi=100):
    h, w = img.shape[0], img.shape[1]
    fig = plt.figure(figsize=(w / dpi, h / dpi), dpi=dpi)
    ax = fig.add_axes([0.0, 0.0, 1.0, 1.0])
    return fig, ax


def draw_detection(ax, corners, label_mode, label_step, use_confidence=True):
    if not corners:
        return None
    xs = []
    ys = []
    conf = []
    for c in corners:
        x, y = corner_xy(c)
        if x is None or y is None:
            continue
        xs.append(x)
        ys.append(y)
        score = c.get("score")
        if score is None:
            score = c.get("confidence", 1.0)
        conf.append(float(score))

    if not xs:
        return None

    conf_max = max(conf)
    if not use_confidence or conf_max <= 0.0:
        sc = ax.scatter(
            xs,
            ys,
            s=36,
            facecolors="none",
            edgecolors="#00e5a8",
            linewidths=1.0,
            alpha=0.9,
        )
    else:
        norm = Normalize(vmin=0.0, vmax=max(conf_max, 1e-6))
        sc = ax.scatter(
            xs,
            ys,
            s=40,
            c=conf,
            cmap="viridis",
            norm=norm,
            edgecolors="black",
            linewidths=0.3,
            alpha=0.9,
        )

    if label_mode != "none":
        for idx, c in enumerate(corners):
            if label_step > 1 and idx % label_step != 0:
                continue
            x, y = corner_xy(c)
            if x is None or y is None:
                continue
            label = format_label(c, label_mode)
            if label:
                label_corner(ax, x + 2.0, y + 2.0, label, color="white", fontsize=7)

    return sc


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
    parser.add_argument(
        "--labels",
        choices=["id", "grid", "target", "all", "none"],
        default="id",
        help="Which labels to draw on detected corners",
    )
    parser.add_argument(
        "--label-step",
        type=int,
        default=1,
        help="Label every Nth corner to reduce clutter",
    )
    parser.add_argument(
        "--no-grid",
        action="store_true",
        help="Disable drawing grid connectivity",
    )
    parser.add_argument(
        "--simple",
        action="store_true",
        help="Save just the input image with overlay (no padding or colorbar)",
    )
    args = parser.parse_args()

    data = json.loads(args.report.read_text())

    image_path = resolve_path(args.report, data["image_path"])
    if not image_path.is_file():
        raise SystemExit(f"Image not found: {image_path}")

    img = mpimg.imread(str(image_path))

    detection = extract_detection(data) or {}
    corners = detection.get("corners") or []
    markers = data.get("markers") or []
    board = data.get("board") or {}
    cell_size = board.get("cell_size")

    grid_map_img = build_grid_map(corners)

    if args.simple:
        fig, ax_img = make_simple_figure(img)
    else:
        fig, ax_img = plt.subplots(figsize=(10, 8))
    ax_img.imshow(img, cmap="gray", origin="upper")
    if grid_map_img and not args.no_grid:
        draw_grid_edges(ax_img, grid_map_img, color="#00c7e6", linewidth=0.8, alpha=0.5)

    sc = draw_detection(
        ax_img,
        corners,
        label_mode=args.labels,
        label_step=max(1, args.label_step),
        use_confidence=True,
    )

    summary_lines = [
        f"detected corners: {len(corners)}",
        f"markers: {len(markers)}",
    ]
    if board:
        rows = board.get("rows")
        cols = board.get("cols")
        if rows and cols and cell_size:
            summary_lines.append(f"board: {rows} x {cols}, cell {cell_size:g} mm")
    ax_img.text(
        0.02,
        0.02,
        "\n".join(summary_lines),
        transform=ax_img.transAxes,
        ha="left",
        va="bottom",
        fontsize=9,
        color="white",
        bbox=dict(
            boxstyle="round,pad=0.3",
            facecolor="black",
            edgecolor="none",
            alpha=0.45,
        ),
    )

    px_per_mm = None
    if grid_map_img and cell_size:
        lengths = []
        for (i, j), p in grid_map_img.items():
            for di, dj in ((1, 0), (0, 1)):
                q = grid_map_img.get((i + di, j + dj))
                if q:
                    dx = p[0] - q[0]
                    dy = p[1] - q[1]
                    lengths.append((dx * dx + dy * dy) ** 0.5)
        if lengths:
            lengths.sort()
            median = lengths[len(lengths) // 2]
            if cell_size > 0:
                px_per_mm = median / float(cell_size)

    ax_img.set_axis_off()
    markers_img = [m for m in markers if m.get("corners_img") or m.get("center_img")]
    if markers_img:
        draw_markers(ax_img, markers_img, prefer_img=True, color="deepskyblue")
    elif markers:
        print(
            "markers are in rectified coordinates; "
            "skipping markers on the original image"
        )

    if not args.simple and sc is not None and sc.get_array() is not None:
        fig.colorbar(sc, ax=ax_img, fraction=0.035, pad=0.02, label="Corner score")
    if not args.simple:
        fig.tight_layout()

    out_path = args.output or args.report.with_name(args.report.stem + "_overlay.png")
    if args.simple:
        fig.savefig(out_path, dpi=fig.dpi, bbox_inches=None, pad_inches=0)
    else:
        fig.savefig(out_path, bbox_inches="tight", dpi=300)
    print(f"overlay saved to {out_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
