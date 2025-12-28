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


def make_simple_figure(img, dpi=100):
    h, w = img.shape[0], img.shape[1]
    fig = plt.figure(figsize=(w / dpi, h / dpi), dpi=dpi)
    ax = fig.add_axes([0.0, 0.0, 1.0, 1.0])
    return fig, ax


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


def parse_int(value):
    if isinstance(value, int):
        return value
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return None


def load_layout_dims(data, report_path):
    config_path = data.get("config_path")
    if not config_path:
        return None
    path = Path(config_path)
    if not path.is_file():
        candidate = (report_path.parent / config_path).resolve()
        if candidate.is_file():
            path = candidate
        else:
            return None
    config = json.loads(path.read_text())
    layout = (config.get("marker") or {}).get("layout") or {}
    rows = parse_int(layout.get("rows"))
    cols = parse_int(layout.get("cols"))
    if rows is None or cols is None:
        return None
    return {"rows": rows, "cols": cols, "path": path}


def compute_id_frame_info(corners, board_cols):
    entries = []
    for c in corners:
        if not isinstance(c, dict):
            continue
        grid = parse_cell(c.get("grid"))
        if grid is None:
            continue
        i = parse_int(grid[0])
        j = parse_int(grid[1])
        corner_id = parse_int(c.get("id"))
        if i is None or j is None or corner_id is None:
            continue
        entries.append((i, j, corner_id))

    total = len(entries)
    if total == 0:
        return {"status": "unknown", "total": 0, "entries": [], "board_cols": board_cols}

    min_i = min(i for i, _, _ in entries)
    max_i = max(i for i, _, _ in entries)
    min_j = min(j for _, j, _ in entries)
    max_j = max(j for _, j, _ in entries)
    visible_cols = max_i - min_i + 1

    board_matches = 0
    visible_matches = 0
    for i, j, corner_id in entries:
        if board_cols is not None:
            expected_board = j * board_cols + i
            if corner_id == expected_board:
                board_matches += 1
        expected_visible = (j - min_j) * visible_cols + (i - min_i)
        if corner_id == expected_visible:
            visible_matches += 1

    status = "mixed"
    if board_cols is not None and board_matches == total:
        status = "board"
    elif visible_matches == total:
        status = "visible"
    elif board_cols is None:
        status = "unknown"

    return {
        "status": status,
        "total": total,
        "entries": entries,
        "board_cols": board_cols,
        "visible_cols": visible_cols,
        "min_i": min_i,
        "min_j": min_j,
        "max_i": max_i,
        "max_j": max_j,
        "board_matches": board_matches,
        "visible_matches": visible_matches,
    }


def draw_chessboard(ax, corners, id_info=None):
    board_cols = id_info.get("board_cols") if id_info else None
    visible_cols = id_info.get("visible_cols") if id_info else None
    min_i = id_info.get("min_i") if id_info else None
    min_j = id_info.get("min_j") if id_info else None

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

        if not isinstance(c, dict):
            continue
        corner_id = c.get("id")
        if corner_id is None:
            continue
        corner_id = parse_int(corner_id)
        if corner_id is None:
            continue

        text_color = "yellow"
        grid = parse_cell(c.get("grid"))
        if grid is not None and board_cols is not None and visible_cols is not None:
            i = parse_int(grid[0])
            j = parse_int(grid[1])
            if i is not None and j is not None and min_i is not None and min_j is not None:
                expected_board = j * board_cols + i
                expected_visible = (j - min_j) * visible_cols + (i - min_i)
                if corner_id == expected_board:
                    text_color = "yellow"
                elif corner_id == expected_visible:
                    text_color = "orange"
                else:
                    text_color = "red"

        ax.annotate(
            str(corner_id),
            (center[0], center[1]),
            xytext=(3, 3),
            textcoords="offset points",
            fontsize=6,
            color=text_color,
            bbox=dict(
                boxstyle="round,pad=0.15",
                facecolor="black",
                edgecolor="none",
                alpha=0.4,
            ),
            zorder=6,
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
    parser.add_argument(
        "--simple",
        action="store_true",
        help="Save just the input image with overlay (no padding or title)",
    )
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

    layout = load_layout_dims(data, args.report)
    board_cols = layout["cols"] if layout else None
    id_info = compute_id_frame_info(chessboard, board_cols)
    if id_info["total"] > 0:
        if id_info["status"] == "board":
            id_label = f"ids: board frame (cols={id_info['board_cols']})"
        elif id_info["status"] == "visible":
            id_label = (
                "ids: visible frame "
                f"(min_i={id_info['min_i']}, min_j={id_info['min_j']}, cols={id_info['visible_cols']})"
            )
        elif id_info["board_cols"] is None:
            id_label = "ids: unknown frame (no layout cols)"
        else:
            id_label = (
                "ids: mixed "
                f"(board {id_info['board_matches']}/{id_info['total']}, "
                f"visible {id_info['visible_matches']}/{id_info['total']})"
            )
        print(id_label)
    else:
        id_label = "ids: n/a"

    img = mpimg.imread(str(image_path))

    if args.simple:
        fig, ax = make_simple_figure(img)
    else:
        fig, ax = plt.subplots(figsize=(10, 8))
    ax.imshow(img, cmap="gray", origin="upper")

    if chessboard:
        draw_chessboard(ax, chessboard, id_info=id_info)
    if candidates:
        draw_circles(ax, candidates, matches)

    if not args.simple:
        ax.set_title(f"Marker board detection overlay ({id_label})")
    ax.set_axis_off()
    if not args.simple:
        fig.tight_layout()

    out_path = args.output or args.report.with_name(args.report.stem + "_overlay.png")
    if args.simple:
        fig.savefig(out_path, dpi=fig.dpi, bbox_inches=None, pad_inches=0)
    else:
        fig.savefig(out_path, bbox_inches="tight", dpi=200)
    print(f"overlay saved to {out_path}")

    if args.show:
        plt.show()


if __name__ == "__main__":
    main()
