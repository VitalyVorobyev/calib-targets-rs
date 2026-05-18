"""Render a synthetic PuzzleBoard, detect it, and save a detection-overlay PNG.

The overlay marks:

- The interior grid edges (thin translucent green).
- Every labelled corner as a small filled dot tagged with its master id.

Note: as of 0.9.0 the raw per-edge bit observations moved off
``PuzzleBoardDetectionResult`` onto the Rust ``PuzzleBoardDiagnostics``
channel, which the Python ``puzzleboard`` binding does not expose. The
edge-bit ring overlay this example used to draw therefore cannot be fed
from Python and has been dropped.

The generated PNG is intended for the puzzleboard crate README and the
book's PuzzleBoard chapter. Run:

    uv run python crates/calib-targets-py/examples/puzzleboard_detection_overlay.py

The output lands at ``book/src/img/puzzleboard_detect_overlay.png``; override
with ``--out``.
"""
from __future__ import annotations

import argparse
import io
from pathlib import Path

import numpy as np
from PIL import Image

import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection

import calib_targets as ct


GRID_STROKE = (0.34, 0.91, 0.47, 0.75)  # brighter green mesh


def synthesise(rows: int, cols: int, dpi: int) -> tuple[np.ndarray, ct.PuzzleBoardSpec]:
    target = ct.PuzzleBoardTargetSpec(
        rows=rows,
        cols=cols,
        square_size_mm=12.0,
    )
    page = ct.PageSpec(
        size=ct.PageSize.custom(
            width_mm=cols * 12.0 + 20.0,
            height_mm=rows * 12.0 + 20.0,
        ),
        margin_mm=5.0,
    )
    doc = ct.PrintableTargetDocument(
        target=target,
        page=page,
        render=ct.RenderOptions(png_dpi=dpi),
    )
    bundle = ct.render_target_bundle(doc)
    img = np.asarray(
        Image.open(io.BytesIO(bundle.png_bytes)).convert("L"),
        dtype=np.uint8,
    )
    spec = ct.PuzzleBoardSpec(rows=rows, cols=cols, cell_size=1.0)
    return img, spec


def draw_overlay(ax, image, result) -> None:
    ax.imshow(image, cmap="gray", interpolation="nearest")

    corners_by_grid = {
        (c.grid.i, c.grid.j): c for c in result.detection.corners if c.grid is not None
    }

    # Grid edges (between adjacent labelled corners) — thin translucent mesh.
    grid_segments: list[tuple[tuple[float, float], tuple[float, float]]] = []
    for c in result.detection.corners:
        if c.grid is None:
            continue
        right = corners_by_grid.get((c.grid.i + 1, c.grid.j))
        down = corners_by_grid.get((c.grid.i, c.grid.j + 1))
        if right is not None:
            grid_segments.append((c.position, right.position))
        if down is not None:
            grid_segments.append((c.position, down.position))
    if grid_segments:
        ax.add_collection(
            LineCollection(grid_segments, colors=[GRID_STROKE], linewidths=1.4)
        )

    # NOTE: the per-edge bit-ring overlay was dropped in 0.9.0 — the raw
    # observed-edge dump moved to the Rust `PuzzleBoardDiagnostics` channel,
    # which the Python binding does not expose.

    # Corner dots + master-id labels.
    xs = [c.position[0] for c in result.detection.corners]
    ys = [c.position[1] for c in result.detection.corners]
    ax.scatter(xs, ys, s=14, c="#ef4444", edgecolor="white", linewidths=0.5, zorder=5)
    for c in result.detection.corners:
        if c.id is None:
            continue
        ax.annotate(
            str(c.id),
            xy=c.position,
            xytext=(4, -4),
            textcoords="offset points",
            fontsize=5,
            color="#0f172a",
            bbox=dict(boxstyle="round,pad=0.15", fc="white", ec="none", alpha=0.75),
        )

    ax.set_xlim(0, image.shape[1])
    ax.set_ylim(image.shape[0], 0)
    ax.set_aspect("equal")
    ax.set_xticks([])
    ax.set_yticks([])
    for spine in ax.spines.values():
        spine.set_visible(False)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=10)
    parser.add_argument("--cols", type=int, default=10)
    parser.add_argument("--dpi", type=int, default=200)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("book/src/img/puzzleboard_detect_overlay.png"),
    )
    args = parser.parse_args()

    image, spec = synthesise(args.rows, args.cols, args.dpi)
    params = ct.PuzzleBoardParams.for_board(spec)
    params.decode.search_mode = ct.PuzzleBoardSearchMode.fixed_board()
    result = ct.detect_puzzleboard(image, params=params)
    print(
        f"rendered {args.rows}x{args.cols} @ {args.dpi} DPI -> "
        f"image {image.shape[1]}x{image.shape[0]} px, "
        f"{len(result.detection.corners)} labelled corners, "
        f"BER={result.decode.bit_error_rate:.3f}"
    )

    fig, ax = plt.subplots(figsize=(8, 8), dpi=150)
    draw_overlay(ax, image, result)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.out, dpi=150, bbox_inches="tight", facecolor="white", pad_inches=0.05)
    plt.close(fig)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
