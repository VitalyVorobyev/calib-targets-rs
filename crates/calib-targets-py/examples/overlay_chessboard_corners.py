"""Phase-1 overlay: raw ChESS corner cloud on each snap.

Consumes the per-snap `CompactFrame` JSON emitted by the chessboard
`run_dataset` example (with `--upscale N`) and renders the input
corner cloud on the native target image. Only the ChESS corner
detection stage is exercised at this phase — chessboard grid labels
and edges are intentionally NOT drawn here.

Each corner is plotted as a dot colored by strength, with two short
tangent segments (length proportional to `1 / sigma`) indicating the
two axis estimates. Overlays render in the detector's coordinate
frame: the background image is upscaled to match the JSON `width` /
`height`.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_chessboard_corners.py \\
        --dataset <dir-of-target-pngs> \\
        --frames  <run-dataset-json-dir> \\
        --out     <overlay-output-dir>
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image


NATIVE_SNAP_WIDTH = 720
NATIVE_SNAP_HEIGHT = 540


def extract_snap(full: np.ndarray, snap_idx: int) -> np.ndarray:
    x0 = snap_idx * NATIVE_SNAP_WIDTH
    if full.shape[1] < x0 + NATIVE_SNAP_WIDTH or full.shape[0] < NATIVE_SNAP_HEIGHT:
        raise ValueError(
            f"image {full.shape} too small for snap {snap_idx} "
            f"(need x0+{NATIVE_SNAP_WIDTH}, h={NATIVE_SNAP_HEIGHT})"
        )
    return full[:NATIVE_SNAP_HEIGHT, x0 : x0 + NATIVE_SNAP_WIDTH]


def upscale_snap(snap: np.ndarray, factor: int) -> np.ndarray:
    if factor == 1:
        return snap
    h, w = snap.shape
    pil = Image.fromarray(snap, mode="L").resize(
        (w * factor, h * factor), resample=Image.BILINEAR
    )
    return np.asarray(pil, dtype=np.uint8)


def parse_frame_name(name: str) -> tuple[int, int] | None:
    """Parse `t{T}s{S}.json` -> (T, S)."""
    stem = name.split(".")[0]
    if not stem.startswith("t"):
        return None
    try:
        t_part, s_part = stem[1:].split("s")
        return int(t_part), int(s_part)
    except ValueError:
        return None


def load_frame(path: Path) -> dict[str, Any]:
    with path.open("r") as f:
        return json.load(f)


def draw_corner_overlay(
    image: np.ndarray,
    frame: dict[str, Any],
    title: str,
    out_path: Path,
) -> dict[str, float]:
    """Render a single-snap corner-cloud overlay.

    Returns a stats dict for caller aggregation.
    """
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)

    corners = frame.get("input_corners", [])
    xs = np.array([c["x"] for c in corners], dtype=float)
    ys = np.array([c["y"] for c in corners], dtype=float)
    strengths = np.array([c["strength"] for c in corners], dtype=float)
    strength_min = float(strengths.min()) if strengths.size else 0.0
    strength_max = float(strengths.max()) if strengths.size else 1.0

    if corners:
        sc = ax.scatter(
            xs,
            ys,
            c=strengths,
            cmap="viridis",
            s=14,
            edgecolors="black",
            linewidths=0.3,
            zorder=5,
        )
        cbar = plt.colorbar(sc, ax=ax, fraction=0.03, pad=0.02)
        cbar.set_label("corner strength", fontsize=8)

        # Short axis tangents: length proportional to (1 / sigma), capped.
        axis_segments: list[tuple[tuple[float, float], tuple[float, float]]] = []
        axis_colors: list[str] = []
        for c in corners:
            for slot, color in (("axes_0", "red"), ("axes_1", "cyan")):
                angle, sigma = c[slot]
                if not math.isfinite(angle) or not math.isfinite(sigma):
                    continue
                # sigma ≈ π means "no info" — skip.
                if sigma >= math.pi - 1e-3:
                    continue
                length = min(8.0, 3.0 / max(sigma, 0.15))
                dx = length * math.cos(angle)
                dy = length * math.sin(angle)
                axis_segments.append(
                    ((c["x"] - dx, c["y"] - dy), (c["x"] + dx, c["y"] + dy))
                )
                axis_colors.append(color)
        for seg, color in zip(axis_segments, axis_colors):
            (x0, y0), (x1, y1) = seg
            ax.plot([x0, x1], [y0, y1], color=color, lw=0.5, alpha=0.55, zorder=4)

    width = frame["width"]
    height = frame["height"]
    upscale = frame["upscale"]
    ax.set_xlim(0, width)
    ax.set_ylim(height, 0)
    ax.set_aspect("equal")
    ax.axis("off")
    ax.set_title(
        f"{title}\n"
        f"corners={len(corners)}   "
        f"strength min={strength_min:.3f} max={strength_max:.3f}   "
        f"upscale={upscale}  frame={width}×{height}",
        fontsize=9,
    )
    handles = [
        Line2D([0], [0], color="red", lw=2, label="axis[0] tangent"),
        Line2D([0], [0], color="cyan", lw=2, label="axis[1] tangent"),
    ]
    ax.legend(handles=handles, loc="lower right", fontsize=7, framealpha=0.9)
    fig.tight_layout()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)

    return {
        "corners": float(len(corners)),
        "strength_min": strength_min,
        "strength_max": strength_max,
        "strength_mean": float(strengths.mean()) if strengths.size else 0.0,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--dataset", required=True, type=Path,
                        help="directory containing target_*.png files")
    parser.add_argument("--frames", required=True, type=Path,
                        help="directory of per-snap CompactFrame JSONs")
    parser.add_argument("--out", required=True, type=Path,
                        help="directory to write overlay PNGs")
    args = parser.parse_args()

    if not args.frames.is_dir():
        raise SystemExit(f"--frames directory not found: {args.frames}")
    if not args.dataset.is_dir():
        raise SystemExit(f"--dataset directory not found: {args.dataset}")

    frame_paths = sorted(
        (p for p in args.frames.iterdir() if p.suffix == ".json" and parse_frame_name(p.name) is not None),
        key=lambda p: parse_frame_name(p.name) or (0, 0),
    )
    if not frame_paths:
        raise SystemExit(f"no t{{T}}s{{S}}.json files in {args.frames}")

    args.out.mkdir(parents=True, exist_ok=True)

    image_cache: dict[int, np.ndarray] = {}
    rows: list[str] = []
    n_written = 0
    for fp in frame_paths:
        ts = parse_frame_name(fp.name)
        if ts is None:
            continue
        target_idx, snap_idx = ts
        frame = load_frame(fp)

        img = image_cache.get(target_idx)
        if img is None:
            target_path = args.dataset / f"target_{target_idx}.png"
            if not target_path.is_file():
                print(f"skip t{target_idx}s{snap_idx}: {target_path} missing")
                continue
            img = np.asarray(Image.open(target_path).convert("L"), dtype=np.uint8)
            image_cache[target_idx] = img

        snap = extract_snap(img, snap_idx)
        up = upscale_snap(snap, int(frame["upscale"]))
        out_path = args.out / f"t{target_idx}s{snap_idx}.png"
        stats = draw_corner_overlay(up, frame, f"t{target_idx}s{snap_idx}", out_path)
        rows.append(
            f"t{target_idx}s{snap_idx}\tcorners={int(stats['corners'])}\t"
            f"strength[min/mean/max]={stats['strength_min']:.3f}/"
            f"{stats['strength_mean']:.3f}/{stats['strength_max']:.3f}"
        )
        n_written += 1

    summary_path = args.out / "_corners_summary.tsv"
    summary_path.write_text("\n".join(rows) + "\n")
    print(f"wrote {n_written} overlays to {args.out}")
    print(f"per-frame stats: {summary_path}")


if __name__ == "__main__":
    main()
