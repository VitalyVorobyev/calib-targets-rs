#!/usr/bin/env python3
import argparse
import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import List, Tuple

import cv2
import numpy as np


# -----------------------------
# Target specification
# -----------------------------

@dataclass(frozen=True)
class CircleSpec:
    sx: int               # square index (0..squares_x-1)
    sy: int               # square index (0..squares_y-1)
    polarity: str         # "white" or "black"


def parse_circle(s: str) -> CircleSpec:
    # format: "sx,sy,polarity"
    parts = [p.strip() for p in s.split(",")]
    if len(parts) != 3:
        raise ValueError(f"Invalid --circle '{s}'. Expected 'sx,sy,polarity'.")
    sx, sy = int(parts[0]), int(parts[1])
    pol = parts[2].lower()
    if pol not in ("white", "black"):
        raise ValueError(f"Invalid polarity '{parts[2]}'. Use 'white' or 'black'.")
    return CircleSpec(sx=sx, sy=sy, polarity=pol)


# -----------------------------
# Board rendering
# -----------------------------

def render_checkerboard_marker_target(
    cols_corners: int,
    rows_corners: int,
    square_px: int,
    circles: List[CircleSpec],
    circle_diameter_frac: float = 0.5,
    bg: int = 127,
) -> Tuple[np.ndarray, dict]:
    """
    Render a checkerboard with given number of INNER corners:
      - squares_x = cols_corners + 1
      - squares_y = rows_corners + 1

    Board image is grayscale uint8.
    Coordinates in "board pixel space" (canonical plane):
      origin (0,0) at top-left corner of board image.
    """
    assert cols_corners >= 2 and rows_corners >= 2
    squares_x = cols_corners + 1
    squares_y = rows_corners + 1

    W = squares_x * square_px
    H = squares_y * square_px

    board = np.full((H, W), bg, dtype=np.uint8)

    # Standard chessboard: top-left square black (OpenCV common)
    for y in range(squares_y):
        for x in range(squares_x):
            is_black = ((x + y) % 2 == 0)
            if is_black:
                x0 = x * square_px
                y0 = y * square_px
                board[y0:y0 + square_px, x0:x0 + square_px] = 0
            else:
                x0 = x * square_px
                y0 = y * square_px
                board[y0:y0 + square_px, x0:x0 + square_px] = 255

    # Draw 3 circles in square centers
    r = 0.5 * circle_diameter_frac * square_px
    r_px = max(1, int(round(r)))

    circle_gt = []
    for c in circles:
        if not (0 <= c.sx < squares_x and 0 <= c.sy < squares_y):
            raise ValueError(f"Circle square index out of range: {c} for {squares_x}x{squares_y} squares.")
        cx = (c.sx + 0.5) * square_px
        cy = (c.sy + 0.5) * square_px
        color = 255 if c.polarity == "white" else 0
        cv2.circle(board, (int(round(cx)), int(round(cy))), r_px, int(color), thickness=-1, lineType=cv2.LINE_AA)
        circle_gt.append({"sx": c.sx, "sy": c.sy, "polarity": c.polarity, "center_board_px": [float(cx), float(cy)]})

    # Inner corner GT positions (corner indices):
    # i in [0..cols_corners-1], j in [0..rows_corners-1]
    corners_board = []
    for j in range(rows_corners):
        for i in range(cols_corners):
            x = (i + 1) * square_px
            y = (j + 1) * square_px
            corners_board.append({"i": i, "j": j, "pt_board_px": [float(x), float(y)]})

    meta = {
        "cols_corners": cols_corners,
        "rows_corners": rows_corners,
        "squares_x": squares_x,
        "squares_y": squares_y,
        "square_px": square_px,
        "circle_diameter_frac": circle_diameter_frac,
        "circles": circle_gt,
        "corners_board": corners_board,
        "board_size_px": [W, H],
    }
    return board, meta


# -----------------------------
# Homography sampling
# -----------------------------

def sample_homography(
    board_w: int,
    board_h: int,
    img_w: int,
    img_h: int,
    rng: np.random.Generator,
    scale_range=(0.55, 0.95),
    rot_deg_range=(-25.0, 25.0),
    persp_jitter=0.08,
    margin=30,
    max_tries=200,
) -> np.ndarray:
    """
    Create random homography mapping board corners -> image points.

    Approach:
      - start with a rotated/scaled rectangle around image center
      - add perspective jitter on the 4 corners
      - ensure corners are inside image with margin; retry if not
    """
    src = np.array([
        [0.0, 0.0],
        [board_w, 0.0],
        [board_w, board_h],
        [0.0, board_h],
    ], dtype=np.float32)

    cx = img_w * 0.5
    cy = img_h * 0.5

    for _ in range(max_tries):
        s = rng.uniform(*scale_range)
        rot = math.radians(rng.uniform(*rot_deg_range))
        cr = math.cos(rot)
        sr = math.sin(rot)

        # base rectangle half-extents in image
        half_w = 0.5 * s * img_w * 0.9
        half_h = half_w * (board_h / board_w)

        rect = np.array([
            [-half_w, -half_h],
            [ half_w, -half_h],
            [ half_w,  half_h],
            [-half_w,  half_h],
        ], dtype=np.float32)

        # rotate
        R = np.array([[cr, -sr], [sr, cr]], dtype=np.float32)
        rect = rect @ R.T

        # translate with some random shift
        tx = rng.uniform(-0.15, 0.15) * img_w
        ty = rng.uniform(-0.15, 0.15) * img_h
        rect[:, 0] += cx + tx
        rect[:, 1] += cy + ty

        # perspective jitter (as fraction of size)
        jitter = np.array([
            [rng.uniform(-1, 1), rng.uniform(-1, 1)],
            [rng.uniform(-1, 1), rng.uniform(-1, 1)],
            [rng.uniform(-1, 1), rng.uniform(-1, 1)],
            [rng.uniform(-1, 1), rng.uniform(-1, 1)],
        ], dtype=np.float32)
        rect += jitter * persp_jitter * min(img_w, img_h)

        # validate inside image
        if (rect[:, 0].min() >= margin and rect[:, 0].max() <= img_w - margin and
            rect[:, 1].min() >= margin and rect[:, 1].max() <= img_h - margin):
            H = cv2.getPerspectiveTransform(src, rect.astype(np.float32))
            return H

    raise RuntimeError("Failed to sample a valid homography after many tries.")


def apply_homography_to_points(H: np.ndarray, pts_xy: np.ndarray) -> np.ndarray:
    """
    pts_xy: Nx2 float
    returns Nx2 float
    """
    pts = pts_xy.reshape(-1, 1, 2).astype(np.float32)
    out = cv2.perspectiveTransform(pts, H).reshape(-1, 2)
    return out


# -----------------------------
# Image corruption
# -----------------------------

def add_gaussian_noise(img: np.ndarray, sigma: float, rng: np.random.Generator) -> np.ndarray:
    if sigma <= 0:
        return img
    noise = rng.normal(0.0, sigma, size=img.shape).astype(np.float32)
    out = img.astype(np.float32) + noise
    return np.clip(out, 0, 255).astype(np.uint8)


def add_blur(img: np.ndarray, ksize: int) -> np.ndarray:
    if ksize <= 1:
        return img
    k = ksize if (ksize % 2 == 1) else (ksize + 1)
    return cv2.GaussianBlur(img, (k, k), 0)


# -----------------------------
# Main generator
# -----------------------------

def default_center_circles(cols_corners: int, rows_corners: int) -> List[CircleSpec]:
    """
    Default: 3 circles near the center squares:
      - two white + one black
    Uses square indices (sx,sy) where squares_x = cols_corners+1.
    """
    squares_x = cols_corners + 1
    squares_y = rows_corners + 1
    cx = squares_x // 2
    cy = squares_y // 2

    # A simple stable pattern around center
    return [
        CircleSpec(cx - 1, cy,     "white"),
        CircleSpec(cx + 1, cy,     "white"),
        CircleSpec(cx,     cy + 1, "black"),
    ]


def main():
    """
    Usage examplse:
        python tools/synth_marker_target.py --out ./synthetic --num 1 --cols-corners 22 --rows-corners 22
        --circle "11,11,white" --circle "12,11,black" --circle "12,12,white"
    """
    ap = argparse.ArgumentParser('Synthetic marker target images')
    ap.add_argument("--out", type=str, required=True, help="Output folder")
    ap.add_argument("--num", type=int, default=50, help="Number of images")
    ap.add_argument("--img-w", type=int, default=1280)
    ap.add_argument("--img-h", type=int, default=960)

    ap.add_argument("--cols-corners", type=int, default=22, help="Inner corners in x (i)")
    ap.add_argument("--rows-corners", type=int, default=22, help="Inner corners in y (j)")
    ap.add_argument("--square-px", type=int, default=60, help="Square size of canonical board rendering")

    ap.add_argument("--circle", type=str, action="append",
                    help="Circle spec 'sx,sy,polarity'. Repeat 3x. If omitted, uses a default center pattern.")
    ap.add_argument("--circle-diam-frac", type=float, default=0.5, help="Circle diameter / square size")

    ap.add_argument("--noise-sigma", type=float, default=6.0)
    ap.add_argument("--blur-ksize", type=int, default=7)

    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    rng = np.random.default_rng(args.seed)

    circles = [parse_circle(s) for s in args.circle] if args.circle else default_center_circles(args.cols_corners, args.rows_corners)
    if len(circles) != 3:
        raise ValueError(f"Expected exactly 3 circles, got {len(circles)}. Use --circle 3 times.")

    board, board_meta = render_checkerboard_marker_target(
        cols_corners=args.cols_corners,
        rows_corners=args.rows_corners,
        square_px=args.square_px,
        circles=circles,
        circle_diameter_frac=args.circle_diam_frac,
    )

    board_h, board_w = board.shape[:2]

    for idx in range(args.num):
        H = sample_homography(
            board_w=board_w, board_h=board_h,
            img_w=args.img_w, img_h=args.img_h,
            rng=rng,
        )

        # Warp board onto output canvas
        warped = cv2.warpPerspective(
            board, H, (args.img_w, args.img_h),
            flags=cv2.INTER_LINEAR,
            borderMode=cv2.BORDER_CONSTANT,
            borderValue=127,
        )

        warped = add_blur(warped, args.blur_ksize)
        warped = add_gaussian_noise(warped, args.noise_sigma, rng)

        # Ground truth: corners + circle centers in image pixels
        corners_board_xy = np.array([c["pt_board_px"] for c in board_meta["corners_board"]], dtype=np.float32)
        corners_img_xy = apply_homography_to_points(H, corners_board_xy)

        circles_board_xy = np.array([c["center_board_px"] for c in board_meta["circles"]], dtype=np.float32)
        circles_img_xy = apply_homography_to_points(H, circles_board_xy)

        meta = {
            "index": idx,
            "image_size": [args.img_w, args.img_h],
            "homography_board_to_img": H.tolist(),
            "board": board_meta,
            "corners_img": [
                {
                    "i": board_meta["corners_board"][k]["i"],
                    "j": board_meta["corners_board"][k]["j"],
                    "pt_img_px": [float(corners_img_xy[k, 0]), float(corners_img_xy[k, 1])]
                }
                for k in range(corners_img_xy.shape[0])
            ],
            "circles_img": [
                {
                    "sx": board_meta["circles"][k]["sx"],
                    "sy": board_meta["circles"][k]["sy"],
                    "polarity": board_meta["circles"][k]["polarity"],
                    "center_img_px": [float(circles_img_xy[k, 0]), float(circles_img_xy[k, 1])]
                }
                for k in range(circles_img_xy.shape[0])
            ],
        }

        img_path = out_dir / f"img_{idx:04d}.png"
        json_path = out_dir / f"img_{idx:04d}.json"
        cv2.imwrite(str(img_path), warped)
        json_path.write_text(json.dumps(meta, indent=2))

    print(f"Saved {args.num} images to {out_dir}")


if __name__ == "__main__":
    main()
