#!/usr/bin/env python3
"""
Generate an OpenCV ChArUco board image and overlay:
  - marker IDs (at marker centers)
  - ChArUco corner IDs (at internal chessboard intersections)

This is useful for verifying OpenCV's *ground-truth* indexing / origin for a given
board shape and dictionary.

Dependencies:
  - opencv-contrib-python (cv2.aruco)
  - matplotlib

Example:
  python tools/render_opencv_charuco_labeled.py
"""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Iterable, Tuple


def require_aruco(cv2) -> None:
    if not hasattr(cv2, "aruco"):
        raise SystemExit(
            "cv2.aruco is missing. Install opencv-contrib-python, not opencv-python."
        )


def get_dictionary(cv2, dict_name: str):
    require_aruco(cv2)
    dict_id = getattr(cv2.aruco, dict_name, None)
    if dict_id is None:
        raise SystemExit(f"Unknown/unsupported dictionary: {dict_name}")
    return cv2.aruco.getPredefinedDictionary(dict_id)


def create_charuco_board(cv2, squares_x: int, squares_y: int, square_len: float, marker_len: float, dictionary):
    require_aruco(cv2)
    # OpenCV Python bindings differ by version.
    if hasattr(cv2.aruco, "CharucoBoard"):
        try:
            return cv2.aruco.CharucoBoard(
                (int(squares_x), int(squares_y)),
                float(square_len),
                float(marker_len),
                dictionary,
            )
        except Exception:
            pass
    if hasattr(cv2.aruco, "CharucoBoard_create"):
        return cv2.aruco.CharucoBoard_create(
            int(squares_x), int(squares_y), float(square_len), float(marker_len), dictionary
        )
    raise SystemExit("OpenCV build does not support ChArUco boards (cv2.aruco.CharucoBoard).")


def draw_board(cv2, board, width: int, height: int, margin: int, border_bits: int):
    require_aruco(cv2)
    size = (int(width), int(height))
    if hasattr(board, "generateImage"):
        return board.generateImage(size, marginSize=int(margin), borderBits=int(border_bits))
    if hasattr(board, "draw"):
        return board.draw(size, marginSize=int(margin), borderBits=int(border_bits))
    if hasattr(cv2.aruco, "drawPlanarBoard"):
        return cv2.aruco.drawPlanarBoard(board, size, int(margin), int(border_bits))
    raise SystemExit("No supported board drawing API found (generateImage/draw/drawPlanarBoard).")


def board_marker_ids(board) -> Iterable[int]:
    ids = getattr(board, "ids", None)
    if ids is None and hasattr(board, "getIds"):
        ids = board.getIds()
    if ids is None:
        return []
    # ids may be an Nx1 array in some versions.
    try:
        return [int(x) for x in ids.reshape(-1).tolist()]
    except Exception:
        return [int(x) for x in ids]


def board_marker_obj_points(board):
    pts = getattr(board, "objPoints", None)
    if pts is None and hasattr(board, "getObjPoints"):
        pts = board.getObjPoints()
    return pts


def board_charuco_obj_points(board):
    pts = getattr(board, "chessboardCorners", None)
    if pts is None and hasattr(board, "getChessboardCorners"):
        pts = board.getChessboardCorners()
    return pts


def obj_to_px_transform(
    squares_x: int,
    squares_y: int,
    square_len: float,
    width: int,
    height: int,
    margin: int,
) -> Tuple[float, float, float]:
    board_w = float(squares_x) * float(square_len)
    board_h = float(squares_y) * float(square_len)

    usable_w = float(width - 2 * margin)
    usable_h = float(height - 2 * margin)
    if usable_w <= 0 or usable_h <= 0:
        raise SystemExit("Invalid output size/margin.")

    scale_x = usable_w / board_w if board_w > 0 else 0.0
    scale_y = usable_h / board_h if board_h > 0 else 0.0
    scale = min(scale_x, scale_y)
    if scale <= 0:
        raise SystemExit("Invalid scale (board too large for output size?).")

    # OpenCV centers the board inside the image (margin acts like a minimum border).
    ox = (float(width) - board_w * scale) * 0.5
    oy = (float(height) - board_h * scale) * 0.5
    return scale, ox, oy


def obj_xy(p) -> Tuple[float, float]:
    # Works for (x,y), (x,y,z), and numpy-ish shapes.
    try:
        x = float(p[0])
        y = float(p[1])
        return x, y
    except Exception:
        raise SystemExit(f"Unexpected point format: {p!r}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--squares-x", type=int, default=22, help="Number of squares in X (cols)")
    ap.add_argument("--squares-y", type=int, default=22, help="Number of squares in Y (rows)")
    ap.add_argument("--dict", type=str, default="DICT_4X4_1000", help="OpenCV predefined dict name")
    ap.add_argument("--marker-size-rel", type=float, default=0.75, help="marker_len / square_len")
    ap.add_argument("--px-per-square", type=int, default=80, help="Rendering resolution")
    ap.add_argument("--margin", type=int, default=40, help="Margin (pixels) around the board")
    ap.add_argument("--border-bits", type=int, default=1, help="Aruco marker border bits")
    ap.add_argument("--out", type=Path, default=None, help="Output PNG path")
    ap.add_argument("--no-show", action="store_true", help="Do not open a window")
    ap.add_argument("--no-save", action="store_true", help="Do not write output file")
    ap.add_argument("--corner-font", type=float, default=5.0, help="Font size for corner IDs")
    ap.add_argument("--marker-font", type=float, default=10.0, help="Font size for marker IDs")
    args = ap.parse_args()

    import cv2  # type: ignore
    import matplotlib.pyplot as plt
    import matplotlib.patheffects as pe

    dictionary = get_dictionary(cv2, args.dict)
    square_len = 1.0
    marker_len = float(args.marker_size_rel) * square_len

    board = create_charuco_board(
        cv2,
        squares_x=args.squares_x,
        squares_y=args.squares_y,
        square_len=square_len,
        marker_len=marker_len,
        dictionary=dictionary,
    )

    width = int(args.squares_x) * int(args.px_per_square) + 2 * int(args.margin)
    height = int(args.squares_y) * int(args.px_per_square) + 2 * int(args.margin)
    img = draw_board(cv2, board, width, height, args.margin, args.border_bits)

    scale, ox, oy = obj_to_px_transform(
        squares_x=args.squares_x,
        squares_y=args.squares_y,
        square_len=square_len,
        width=width,
        height=height,
        margin=args.margin,
    )

    def to_px(x: float, y: float) -> Tuple[float, float]:
        return (ox + x * scale, oy + y * scale)

    fig, ax = plt.subplots(figsize=(12, 9))
    ax.imshow(img, cmap="gray", origin="upper")
    ax.set_axis_off()
    ax.set_title(f"OpenCV ChArUco {args.squares_x}x{args.squares_y} ({args.dict})")

    stroke = [pe.withStroke(linewidth=2.0, foreground="black", alpha=0.7)]

    # Marker IDs.
    mids = list(board_marker_ids(board))
    mpts = list(board_marker_obj_points(board))
    for mid, corners in zip(mids, mpts):
        corners_xy = [obj_xy(p) for p in corners]
        cx = sum(p[0] for p in corners_xy) / len(corners_xy)
        cy = sum(p[1] for p in corners_xy) / len(corners_xy)
        px, py = to_px(cx, cy)
        ax.text(
            px,
            py,
            str(int(mid)),
            color="#ff5252",
            fontsize=float(args.marker_font),
            ha="center",
            va="center",
            path_effects=stroke,
        )

    # ChArUco corner IDs (internal chessboard intersections).
    cpts = list(board_charuco_obj_points(board))
    for cid, p in enumerate(cpts):
        x, y = obj_xy(p)
        px, py = to_px(x, y)
        ax.text(
            px + 1.5,
            py + 1.5,
            str(cid),
            color="#00e5ff",
            fontsize=float(args.corner_font),
            ha="left",
            va="bottom",
            path_effects=stroke,
        )

    fig.tight_layout()

    out = args.out
    if out is None:
        out = Path(f"tmpdata/opencv_charuco_{args.squares_x}x{args.squares_y}_{args.dict}_labeled.png")

    if not args.no_save:
        out.parent.mkdir(parents=True, exist_ok=True)
        fig.savefig(out, dpi=300, bbox_inches="tight")
        print(f"Saved: {out}")

    if not args.no_show:
        plt.show()


if __name__ == "__main__":
    main()
