"""Render ChArUco detector diagnostics produced by
`calib-targets-charuco/examples/run_dataset.rs --emit-diag --save-snaps`.

For each `t{T}s{S}_diag.json` it reads the companion `t{T}s{S}.png` (the
pre-upscaled snap) and overlays:

  * marker-cell quads, filled by match status
      green  = expected_id set & score ≥ expected_score_threshold
      orange = expected_id set & score <  threshold  (weak match)
      gray   = mapped to a black square (no marker expected)
      red    = not sampled (too small / off-image)
  * a per-bit log-likelihood mini-heatmap for the TOP_N best + TOP_N worst
    cells (identified by `expected_score`).
  * chess-corner dots derived from the union of all cell corners.
  * header text: chosen / runner-up hypothesis, margin, rejection reason.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_charuco.py \\
        --dir bench_results/charuco/target_0_diag \\
        --out bench_results/charuco/target_0_diag/overlay

Or single-frame:
    uv run python crates/calib-targets-py/examples/overlay_charuco.py \\
        --diag <path/to/t0s1_diag.json> --snap <path/to/t0s1.png> \\
        --out <path/to/out.png>
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any, Iterable

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.patches import Polygon
from matplotlib.collections import PatchCollection
from PIL import Image


SCORE_OK_THRESHOLD = -1.0  # per-cell LL ≥ this counts as "strong match"
TOP_N_HEATMAPS = 12


def load_diag(path: Path) -> dict[str, Any]:
    with path.open() as f:
        return json.load(f)


def iter_cells(diag: dict[str, Any]) -> Iterable[dict[str, Any]]:
    for comp in diag["detect"]["components"]:
        board = comp.get("board")
        if not board:
            continue
        for cell in board.get("cells", []):
            yield cell


def chess_corner_points(diag: dict[str, Any]) -> np.ndarray:
    pts: set[tuple[float, float]] = set()
    for cell in iter_cells(diag):
        for p in cell["corners_img"]:
            pts.add((round(p[0], 1), round(p[1], 1)))
    if not pts:
        return np.zeros((0, 2), dtype=float)
    return np.array(sorted(pts), dtype=float)


def classify_cell(cell: dict[str, Any]) -> tuple[str, str]:
    """Return (fill_color, legend_label)."""
    if not cell.get("sampled", False):
        return "#ff4d4d", "not sampled"
    expected_id = cell.get("expected_id")
    expected_score = cell.get("expected_score")
    if expected_id is None or expected_score is None or not math.isfinite(expected_score):
        return "#8a8a8a", "black square / unmapped"
    if expected_score >= SCORE_OK_THRESHOLD:
        return "#2ca02c", f"strong match ≥{SCORE_OK_THRESHOLD:g}"
    return "#ff7f0e", f"weak match <{SCORE_OK_THRESHOLD:g}"


def draw_cells(ax: plt.Axes, diag: dict[str, Any]) -> None:
    patches = []
    colors = []
    for cell in iter_cells(diag):
        poly = Polygon(cell["corners_img"], closed=True)
        fill, _ = classify_cell(cell)
        patches.append(poly)
        colors.append(fill)
    if not patches:
        return
    pc = PatchCollection(patches, alpha=0.35)
    pc.set_facecolor(colors)
    pc.set_edgecolor("black")
    pc.set_linewidth(0.6)
    ax.add_collection(pc)

    # cell id labels
    for cell in iter_cells(diag):
        if cell.get("expected_id") is None:
            continue
        centroid = np.mean(cell["corners_img"], axis=0)
        ax.text(
            centroid[0],
            centroid[1],
            str(cell["expected_id"]),
            fontsize=5,
            ha="center",
            va="center",
            color="white",
            path_effects=None,
        )


def inset_heatmap(
    ax: plt.Axes,
    cell: dict[str, Any],
    bits: int,
    size_px: float,
) -> None:
    """Draw a tiny bit-LL heatmap over the cell quad."""
    ll = cell.get("expected_bit_ll") or []
    if len(ll) != bits * bits:
        return
    arr = np.array(ll, dtype=float).reshape(bits, bits)
    # Normalize: ~0 = right, very negative = wrong.
    # Use a diverging-ish cmap with center at 0.
    vmin = min(-6.0, arr.min())
    vmax = 0.0
    centroid = np.mean(cell["corners_img"], axis=0)
    half = size_px * 0.5
    extent = [centroid[0] - half, centroid[0] + half, centroid[1] + half, centroid[1] - half]
    ax.imshow(arr, cmap="RdYlGn", vmin=vmin, vmax=vmax, extent=extent, interpolation="nearest", alpha=0.85, zorder=10)


def header_text(diag: dict[str, Any]) -> str:
    detect = diag["detect"]
    if not detect["components"]:
        return "no components"
    comp = detect["components"][0]
    board = comp.get("board") or {}
    chosen = board.get("chosen")
    runner = board.get("runner_up")
    margin = board.get("margin")
    reject = board.get("rejection")
    outcome = comp.get("outcome", {})
    status = outcome.get("status", "?")
    matcher = comp.get("matcher", "?")

    lines = [
        f"matcher={matcher} status={status} cells={comp.get('candidate_cell_count')} chess_corners={comp.get('chess_corner_count')}",
    ]
    if chosen:
        lines.append(
            f"chosen: rot={chosen['rotation']} t={tuple(chosen['translation'])} score={chosen['score']:.2f} contrib={chosen['contributing_cells']}"
        )
    if runner:
        lines.append(
            f"runner: rot={runner['rotation']} t={tuple(runner['translation'])} score={runner['score']:.2f} contrib={runner['contributing_cells']}"
        )
    if margin is not None:
        lines.append(f"margin={margin:.4f}")
    if reject:
        lines.append(f"rejection: {reject}")
    if status == "ok":
        lines.append(
            f"ok: markers={outcome.get('markers')} charuco_corners={outcome.get('charuco_corners')}"
        )
    return "\n".join(lines)


def render_one(diag_path: Path, snap_path: Path, out_path: Path) -> None:
    diag = load_diag(diag_path)
    img = np.array(Image.open(snap_path).convert("L"))
    h, w = img.shape

    fig, ax = plt.subplots(1, 1, figsize=(w / 120, h / 120 + 1.2), dpi=150)
    ax.imshow(img, cmap="gray", vmin=0, vmax=255)
    ax.set_xlim(0, w)
    ax.set_ylim(h, 0)

    draw_cells(ax, diag)

    # pick top/bot cells by expected_score for heatmap insets
    cells = list(iter_cells(diag))
    scored = [
        c for c in cells if c.get("expected_score") is not None and math.isfinite(c["expected_score"])
    ]
    scored.sort(key=lambda c: c["expected_score"])
    worst = scored[:TOP_N_HEATMAPS]
    best = scored[-TOP_N_HEATMAPS:]
    bits = diag["detect"]["components"][0].get("board", {}).get("bits_per_side", 0)
    if bits > 0:
        cell_side_px = approximate_cell_side(cells)
        heatmap_size = 0.65 * cell_side_px
        for cell in (*best, *worst):
            inset_heatmap(ax, cell, bits, heatmap_size)

    pts = chess_corner_points(diag)
    if pts.size:
        ax.scatter(pts[:, 0], pts[:, 1], s=2.0, c="#1f77b4", alpha=0.5, zorder=11)

    ax.set_axis_off()
    fig.suptitle(header_text(diag), fontsize=9, ha="left", x=0.01, y=0.995)
    fig.tight_layout(rect=(0, 0, 1, 0.87))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)


def approximate_cell_side(cells: list[dict[str, Any]]) -> float:
    if not cells:
        return 10.0
    sides = []
    for c in cells[: min(len(cells), 30)]:
        pts = np.array(c["corners_img"], dtype=float)
        d1 = np.linalg.norm(pts[1] - pts[0])
        d2 = np.linalg.norm(pts[2] - pts[1])
        sides.extend([d1, d2])
    if not sides:
        return 10.0
    return float(np.median(sides))


def batch_render(dir_: Path, out_dir: Path) -> list[Path]:
    written: list[Path] = []
    for diag in sorted(dir_.glob("*_diag.json")):
        stem = diag.name.removesuffix("_diag.json")
        snap = diag.parent / f"{stem}.png"
        if not snap.exists():
            print(f"skip {stem}: missing snap {snap}")
            continue
        out = out_dir / f"{stem}.png"
        render_one(diag, snap, out)
        written.append(out)
        print(f"wrote {out}")
    return written


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--dir", type=Path, help="dataset out dir (containing {stem}_diag.json + {stem}.png)")
    p.add_argument("--out", type=Path, required=True, help="output directory (batch) or file (single)")
    p.add_argument("--diag", type=Path, help="single-frame diag JSON")
    p.add_argument("--snap", type=Path, help="single-frame snap PNG")
    args = p.parse_args()

    if args.dir:
        args.out.mkdir(parents=True, exist_ok=True)
        batch_render(args.dir, args.out)
        return 0
    if args.diag and args.snap:
        render_one(args.diag, args.snap, args.out)
        return 0
    p.error("must supply either --dir or both --diag and --snap")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
