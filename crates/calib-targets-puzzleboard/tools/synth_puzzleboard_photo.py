#!/usr/bin/env python3
"""Generate deterministic author-like PuzzleBoard photo fixtures.

The generator intentionally uses the same physical convention as
`calib-targets-print`:

- black checker square iff `(master_row + master_col) % 2 == 0`;
- horizontal edge dots are between square rows and use `map_b`;
- vertical edge dots are between square columns and use `map_a`;
- dot polarity is black = 0, white = 1.

It renders a clean board texture, projects it through a perspective camera,
adds radial distortion and mild camera artefacts, and writes both images and a
JSON manifest with exact corner ground truth.
"""

from __future__ import annotations

import argparse
import json
import math
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import cv2
import numpy as np


REPO_ROOT = Path(__file__).resolve().parents[3]
MAP_DIR = REPO_ROOT / "crates/calib-targets-puzzleboard/src/data"
DEFAULT_OUT = REPO_ROOT / "testdata/puzzleboard_synthetic_author_like"


@dataclass(frozen=True)
class Scenario:
    name: str
    seed: int
    rows: int
    cols: int
    origin_row: int
    origin_col: int
    texture_px_per_square: int
    image_width: int
    image_height: int
    quad: tuple[tuple[float, float], tuple[float, float], tuple[float, float], tuple[float, float]]
    k1: float
    k2: float
    blur_sigma: float
    noise_sigma: float
    vignette: float
    brightness: float
    contrast: float
    jpeg_quality: int


SCENARIOS = [
    Scenario(
        name="author_like_oblique",
        seed=0xC001,
        rows=20,
        cols=20,
        origin_row=18,
        origin_col=219,
        texture_px_per_square=52,
        image_width=640,
        image_height=480,
        quad=((36.0, 18.0), (610.0, 40.0), (590.0, 444.0), (18.0, 410.0)),
        k1=-0.075,
        k2=0.018,
        blur_sigma=0.75,
        noise_sigma=2.2,
        vignette=0.18,
        brightness=2.0,
        contrast=1.00,
        jpeg_quality=92,
    ),
    Scenario(
        name="author_like_foreshortened",
        seed=0xC002,
        rows=20,
        cols=20,
        origin_row=30,
        origin_col=8,
        texture_px_per_square=54,
        image_width=640,
        image_height=480,
        quad=((128.0, 4.0), (566.0, 32.0), (632.0, 470.0), (24.0, 392.0)),
        k1=-0.115,
        k2=0.032,
        blur_sigma=0.95,
        noise_sigma=2.8,
        vignette=0.24,
        brightness=1.0,
        contrast=0.96,
        jpeg_quality=90,
    ),
    Scenario(
        name="small_rotated_fragment",
        seed=0xC003,
        rows=9,
        cols=9,
        origin_row=453,
        origin_col=376,
        texture_px_per_square=72,
        image_width=640,
        image_height=480,
        quad=((186.0, 22.0), (504.0, 84.0), (444.0, 394.0), (112.0, 320.0)),
        k1=-0.045,
        k2=0.006,
        blur_sigma=0.45,
        noise_sigma=1.4,
        vignette=0.10,
        brightness=4.0,
        contrast=1.03,
        jpeg_quality=94,
    ),
]


def unpack_map(path: Path, rows: int, cols: int) -> np.ndarray:
    data = path.read_bytes()
    out = np.zeros((rows, cols), dtype=np.uint8)
    for idx in range(rows * cols):
        out.flat[idx] = (data[idx // 8] >> (idx % 8)) & 1
    return out


def horizontal_bit(map_b: np.ndarray, row: int, col: int) -> int:
    return int(map_b[row % 167, col % 3])


def vertical_bit(map_a: np.ndarray, row: int, col: int) -> int:
    return int(map_a[row % 3, col % 167])


def render_texture(s: Scenario, map_a: np.ndarray, map_b: np.ndarray) -> np.ndarray:
    px = s.texture_px_per_square
    h = s.rows * px
    w = s.cols * px
    img = np.full((h, w), 222, dtype=np.uint8)
    black = 34
    white = 222
    dot_black = 18
    dot_white = 244
    radius = max(2, round(px / 6.0))

    for r in range(s.rows):
        y0 = r * px
        y1 = (r + 1) * px
        for c in range(s.cols):
            x0 = c * px
            x1 = (c + 1) * px
            master_r = s.origin_row + r
            master_c = s.origin_col + c
            img[y0:y1, x0:x1] = black if (master_r + master_c) % 2 == 0 else white

    for r in range(s.rows - 1):
        for c in range(s.cols):
            bit = horizontal_bit(map_b, s.origin_row + r, s.origin_col + c)
            color = dot_white if bit == 1 else dot_black
            center = (round((c + 0.5) * px), round((r + 1.0) * px))
            cv2.circle(img, center, radius, color, thickness=-1, lineType=cv2.LINE_AA)

    for r in range(s.rows):
        for c in range(s.cols - 1):
            bit = vertical_bit(map_a, s.origin_row + r, s.origin_col + c)
            color = dot_white if bit == 1 else dot_black
            center = (round((c + 1.0) * px), round((r + 0.5) * px))
            cv2.circle(img, center, radius, color, thickness=-1, lineType=cv2.LINE_AA)

    return img


def distort_points(points: np.ndarray, width: int, height: int, k1: float, k2: float) -> np.ndarray:
    cx = 0.5 * (width - 1)
    cy = 0.5 * (height - 1)
    fx = max(width, height)
    fy = fx
    x = (points[:, 0] - cx) / fx
    y = (points[:, 1] - cy) / fy
    r2 = x * x + y * y
    scale = 1.0 + k1 * r2 + k2 * r2 * r2
    out = np.empty_like(points, dtype=np.float32)
    out[:, 0] = cx + fx * x * scale
    out[:, 1] = cy + fy * y * scale
    return out


def apply_radial_distortion(img: np.ndarray, k1: float, k2: float) -> np.ndarray:
    height, width = img.shape[:2]
    cx = 0.5 * (width - 1)
    cy = 0.5 * (height - 1)
    fx = max(width, height)
    fy = fx
    yy, xx = np.indices((height, width), dtype=np.float32)
    xd = (xx - cx) / fx
    yd = (yy - cy) / fy
    r2d = xd * xd + yd * yd
    # First-order inverse is sufficient for these mild synthetic distortions.
    scale = 1.0 + k1 * r2d + k2 * r2d * r2d
    xu = xd / scale
    yu = yd / scale
    map_x = cx + fx * xu
    map_y = cy + fy * yu
    return cv2.remap(img, map_x, map_y, interpolation=cv2.INTER_LINEAR, borderValue=182)


def apply_camera_effects(img: np.ndarray, s: Scenario) -> np.ndarray:
    rng = np.random.default_rng(s.seed)
    out = img.astype(np.float32)
    h, w = out.shape
    yy, xx = np.indices((h, w), dtype=np.float32)
    cx = 0.54 * w
    cy = 0.47 * h
    r = np.sqrt(((xx - cx) / w) ** 2 + ((yy - cy) / h) ** 2)
    illum = 1.0 - s.vignette * (r / max(r.max(), 1e-6)) ** 2
    illum += 0.035 * ((xx / max(w - 1, 1)) - 0.5)
    out = (out - 128.0) * s.contrast + 128.0
    out = out * illum + s.brightness
    if s.blur_sigma > 0.0:
        out = cv2.GaussianBlur(out, (0, 0), s.blur_sigma)
    if s.noise_sigma > 0.0:
        out += rng.normal(0.0, s.noise_sigma, out.shape).astype(np.float32)
    out = np.clip(out, 0.0, 255.0).astype(np.uint8)
    ok, enc = cv2.imencode(".jpg", out, [int(cv2.IMWRITE_JPEG_QUALITY), s.jpeg_quality])
    if not ok:
        raise RuntimeError("JPEG compression failed")
    return cv2.imdecode(enc, cv2.IMREAD_GRAYSCALE)


def scenario_image(s: Scenario, map_a: np.ndarray, map_b: np.ndarray) -> tuple[np.ndarray, list[dict[str, Any]]]:
    tex = render_texture(s, map_a, map_b)
    src = np.array(
        [[0.0, 0.0], [tex.shape[1] - 1.0, 0.0], [tex.shape[1] - 1.0, tex.shape[0] - 1.0], [0.0, tex.shape[0] - 1.0]],
        dtype=np.float32,
    )
    dst = np.array(s.quad, dtype=np.float32)
    h_mat = cv2.getPerspectiveTransform(src, dst)
    warped = cv2.warpPerspective(
        tex,
        h_mat,
        (s.image_width, s.image_height),
        flags=cv2.INTER_LINEAR,
        borderMode=cv2.BORDER_CONSTANT,
        borderValue=182,
    )
    distorted = apply_radial_distortion(warped, s.k1, s.k2)
    photo = apply_camera_effects(distorted, s)

    local = []
    for r in range(s.rows + 1):
        for c in range(s.cols + 1):
            local.append([c * s.texture_px_per_square, r * s.texture_px_per_square])
    local_np = np.asarray(local, dtype=np.float32).reshape(-1, 1, 2)
    projected = cv2.perspectiveTransform(local_np, h_mat).reshape(-1, 2)
    projected = distort_points(projected, s.image_width, s.image_height, s.k1, s.k2)
    corners = []
    idx = 0
    for r in range(s.rows + 1):
        for c in range(s.cols + 1):
            x, y = projected[idx]
            corners.append(
                {
                    "local_row": r,
                    "local_col": c,
                    "master_row": s.origin_row + r,
                    "master_col": s.origin_col + c,
                    "pixel_x": float(x),
                    "pixel_y": float(y),
                }
            )
            idx += 1
    return photo, corners


def write_overlay(img: np.ndarray, corners: list[dict[str, Any]], path: Path) -> None:
    color = cv2.cvtColor(img, cv2.COLOR_GRAY2BGR)
    for corner in corners:
        x = int(round(corner["pixel_x"]))
        y = int(round(corner["pixel_y"]))
        if 0 <= x < color.shape[1] and 0 <= y < color.shape[0]:
            cv2.circle(color, (x, y), 2, (0, 0, 255), thickness=-1, lineType=cv2.LINE_AA)
    cv2.imwrite(str(path), color)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--preview-dir", type=Path)
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    if args.preview_dir:
        args.preview_dir.mkdir(parents=True, exist_ok=True)

    map_a = unpack_map(MAP_DIR / "map_a.bin", 3, 167)
    map_b = unpack_map(MAP_DIR / "map_b.bin", 167, 3)

    manifest: dict[str, Any] = {
        "schema": "calib-targets-puzzleboard.synthetic-author-like.v1",
        "generator": "crates/calib-targets-puzzleboard/tools/synth_puzzleboard_photo.py",
        "map_source": "crates/calib-targets-puzzleboard/src/data/map_a.bin + map_b.bin",
        "pixel_frame": "image pixels, origin top-left, x right, y down; pixel centers at integer coordinates",
        "scenarios": [],
    }
    for s in SCENARIOS:
        img, corners = scenario_image(s, map_a, map_b)
        image_name = f"{s.name}.png"
        cv2.imwrite(str(args.out_dir / image_name), img)
        if args.preview_dir:
            write_overlay(img, corners, args.preview_dir / f"{s.name}_truth_corners.png")
        entry = asdict(s)
        entry["image"] = image_name
        entry["corners"] = corners
        manifest["scenarios"].append(entry)

    (args.out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
