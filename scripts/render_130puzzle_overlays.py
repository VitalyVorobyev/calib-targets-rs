#!/usr/bin/env python3
"""Render side-by-side detection overlays for the 130x130_puzzle dataset.

For each requested ``target_<idx>.png`` snap, runs the chessboard detector
twice — once with ``GraphBuildAlgorithm::ChessboardV2`` and once with
``GraphBuildAlgorithm::Topological`` — and saves a 2-up PNG showing
labelled corners and cardinal grid edges on top of the (2× upscaled) image.

Output layout::

    docs/img/130x130_puzzle/<target>-<snap>/00-input.png
    docs/img/130x130_puzzle/<target>-<snap>/01-chessboard-v2.png
    docs/img/130x130_puzzle/<target>-<snap>/02-topological.png
    docs/img/130x130_puzzle/<target>-<snap>/03-side-by-side.png
    docs/img/130x130_puzzle/manifest.json

The dataset is private; the script skips missing frames with a warning so
running on a fresh public clone does not error.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from PIL import Image

import calib_targets as ct


SNAP_WIDTH = 720
SNAP_HEIGHT = 540
SNAPS_PER_IMAGE = 6
DEFAULT_UPSCALE = 2

# Representative snaps: a clean detection on both methods, two cases that
# topological used to fail before the cluster gate / 15° / per-component
# cell-size filters landed, and the smoke-test frame for orientation.
DEFAULT_FRAMES = [
    (0, 0),
    (15, 0),
    (18, 3),
    (10, 2),
]


@dataclass(frozen=True)
class FrameSpec:
    target_idx: int
    snap_idx: int

    @property
    def stem(self) -> str:
        return f"target_{self.target_idx}_snap_{self.snap_idx}"


def parse_frame(spec: str) -> FrameSpec:
    if "/" in spec:
        a, b = spec.split("/", 1)
    elif "_" in spec and spec.count("_") >= 1:
        # Accept "15_0" shorthand.
        parts = spec.split("_")
        a, b = parts[-2], parts[-1]
    else:
        raise SystemExit(f"unrecognised frame spec: {spec!r} (use 'TARGET/SNAP' or 'TARGET_SNAP')")
    return FrameSpec(int(a), int(b))


def load_snap(path: Path, snap_idx: int, upscale: int) -> np.ndarray:
    img = Image.open(path).convert("L")
    full = np.asarray(img, dtype=np.uint8)
    if full.shape[0] != SNAP_HEIGHT:
        raise SystemExit(
            f"{path} has height {full.shape[0]}, expected {SNAP_HEIGHT}"
        )
    expected_width = SNAP_WIDTH * SNAPS_PER_IMAGE
    if full.shape[1] != expected_width:
        raise SystemExit(
            f"{path} has width {full.shape[1]}, expected {expected_width}"
        )
    if not (0 <= snap_idx < SNAPS_PER_IMAGE):
        raise SystemExit(f"snap_idx {snap_idx} out of range [0, {SNAPS_PER_IMAGE})")
    x0 = snap_idx * SNAP_WIDTH
    snap = full[:, x0 : x0 + SNAP_WIDTH]
    if upscale != 1:
        if upscale <= 0:
            raise SystemExit(f"--upscale must be positive, got {upscale}")
        snap_img = Image.fromarray(snap)
        snap_img = snap_img.resize(
            (SNAP_WIDTH * upscale, SNAP_HEIGHT * upscale), Image.Resampling.BILINEAR
        )
        snap = np.asarray(snap_img, dtype=np.uint8)
    return snap


def run_detector(
    image: np.ndarray,
    algorithm: str,
    min_corner_strength: float,
) -> dict | None:
    params = ct.ChessboardParams(
        graph_build_algorithm=algorithm,
        min_corner_strength=min_corner_strength,
    )
    result = ct.detect_chessboard(image, params=params)
    if result is None:
        return None
    return result.to_dict()


def labels_to_xy(labels: list[dict]) -> dict[tuple[int, int], tuple[float, float]]:
    by_grid: dict[tuple[int, int], tuple[float, float]] = {}
    for entry in labels:
        # ChessboardDetectionResult.target.corners is a list of LabeledCorner
        # dicts; each has "grid": {"i": int, "j": int} and "position": [x, y].
        grid = entry.get("grid")
        if grid is None:
            continue
        pos = entry.get("position")
        if pos is None:
            continue
        i = int(grid.get("i") if isinstance(grid, dict) else grid[0])
        j = int(grid.get("j") if isinstance(grid, dict) else grid[1])
        by_grid[(i, j)] = (float(pos[0]), float(pos[1]))
    return by_grid


def draw_overlay(ax, image: np.ndarray, detection: dict | None, title: str) -> int:
    ax.imshow(image, cmap="gray", origin="upper")
    ax.set_title(title, fontsize=10)
    ax.axis("off")
    if detection is None:
        ax.text(
            0.5,
            0.5,
            "no detection",
            transform=ax.transAxes,
            ha="center",
            va="center",
            color="#ffeb3b",
            fontsize=12,
            bbox=dict(boxstyle="round,pad=0.3", fc="black", alpha=0.6),
        )
        return 0
    target = detection.get("target") or {}
    corners = target.get("corners") or []
    by_grid = labels_to_xy(corners)
    if not by_grid:
        return 0
    for (i, j), (x, y) in by_grid.items():
        right = by_grid.get((i + 1, j))
        if right is not None:
            ax.plot([x, right[0]], [y, right[1]], color="#1b9e77", lw=0.45, alpha=0.85)
        down = by_grid.get((i, j + 1))
        if down is not None:
            ax.plot([x, down[0]], [y, down[1]], color="#377eb8", lw=0.45, alpha=0.85)
    xs = [p[0] for p in by_grid.values()]
    ys = [p[1] for p in by_grid.values()]
    ax.scatter(xs, ys, s=2.5, c="#fdd835", edgecolors="black", linewidths=0.15, zorder=4)
    ax.text(
        0.02,
        0.98,
        f"{len(by_grid)} corners",
        transform=ax.transAxes,
        ha="left",
        va="top",
        color="white",
        fontsize=10,
        bbox=dict(boxstyle="round,pad=0.3", fc="black", alpha=0.5),
    )
    return len(by_grid)


def render_frame(
    image: np.ndarray,
    frame: FrameSpec,
    out_dir: Path,
    min_corner_strength: float,
) -> dict[str, object]:
    out_dir.mkdir(parents=True, exist_ok=True)
    Image.fromarray(image).save(out_dir / "00-input.png")

    methods = [
        ("ChessboardV2", "chessboard_v2", "01-chessboard-v2.png"),
        ("Topological", "topological", "02-topological.png"),
    ]
    counts: dict[str, int] = {}
    detections: dict[str, dict | None] = {}
    for label, algorithm, filename in methods:
        det = run_detector(image, algorithm, min_corner_strength)
        detections[label] = det
        fig, ax = plt.subplots(figsize=(6, 5), dpi=180)
        title = f"{label}: target_{frame.target_idx} snap {frame.snap_idx}"
        n = draw_overlay(ax, image, det, title)
        counts[label] = n
        fig.tight_layout()
        fig.savefig(out_dir / filename, bbox_inches="tight", pad_inches=0.05)
        plt.close(fig)

    fig, axes = plt.subplots(1, 2, figsize=(12, 5), dpi=180)
    for ax, (label, _, _) in zip(axes, methods):
        title = f"{label}: target_{frame.target_idx} snap {frame.snap_idx}"
        draw_overlay(ax, image, detections[label], title)
    fig.suptitle(
        f"130x130_puzzle target_{frame.target_idx} snap {frame.snap_idx} — chessboard detection",
        fontsize=11,
    )
    fig.tight_layout()
    fig.savefig(out_dir / "03-side-by-side.png", bbox_inches="tight", pad_inches=0.05)
    plt.close(fig)
    return {
        "frame": asdict(frame),
        "labelled": counts,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dataset-dir",
        type=Path,
        default=Path("privatedata/130x130_puzzle"),
        help="Directory containing target_*.png stitched 6-snap images.",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path("docs/img/130x130_puzzle"),
        help="Output directory for per-frame overlays + manifest.json.",
    )
    parser.add_argument(
        "--frames",
        nargs="+",
        default=None,
        help='Override default frames; each is "TARGET/SNAP" or "TARGET_SNAP".',
    )
    parser.add_argument(
        "--upscale", type=int, default=DEFAULT_UPSCALE, help="2× matches the regression test."
    )
    parser.add_argument(
        "--min-corner-strength",
        type=float,
        default=25.0,
        help=(
            "ChESS-strength floor passed to the chessboard detector for both "
            "methods. The workspace default is `0.0` (no filter); we use "
            "25.0 here to drop the blurred-region corners that make the "
            "overlay look noisier than the actual signal. Empirically this "
            "trades ~15%% labelled-count for noticeably cleaner detection "
            "extent (focused, in-focus corners only). The Rust regression "
            "contracts run with the workspace default, not this override."
        ),
    )
    return parser.parse_args()


def iter_frames(args: argparse.Namespace) -> Iterable[FrameSpec]:
    if args.frames:
        for spec in args.frames:
            yield parse_frame(spec)
    else:
        for tgt, snap in DEFAULT_FRAMES:
            yield FrameSpec(tgt, snap)


def main() -> None:
    args = parse_args()
    if not args.dataset_dir.is_dir():
        print(
            f"[skip] dataset directory missing: {args.dataset_dir}. The 130x130_puzzle dataset is private."
        )
        return
    args.out_dir.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, object]] = []
    for frame in iter_frames(args):
        path = args.dataset_dir / f"target_{frame.target_idx}.png"
        if not path.exists():
            print(f"[skip] {path} missing")
            continue
        image = load_snap(path, frame.snap_idx, args.upscale)
        out_dir = args.out_dir / frame.stem
        row = render_frame(image, frame, out_dir, args.min_corner_strength)
        print(
            f"target_{frame.target_idx} snap {frame.snap_idx}: "
            f"v2={row['labelled']['ChessboardV2']} topo={row['labelled']['Topological']}"
        )
        rows.append(row)
    manifest = {
        "schema": 1,
        "dataset_dir": str(args.dataset_dir),
        "out_dir": str(args.out_dir),
        "upscale": args.upscale,
        "min_corner_strength": args.min_corner_strength,
        "frames": rows,
    }
    (args.out_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2), encoding="utf-8"
    )
    print(f"rendered {len(rows)} frame(s) to {args.out_dir}")


if __name__ == "__main__":
    main()
