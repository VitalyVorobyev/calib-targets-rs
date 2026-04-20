"""Render ChArUco detection + matcher diagnostics produced by
`calib-targets-charuco/examples/run_dataset.rs --emit-diag --save-snaps`.

For each `t{T}s{S}_diag.json` it reads the companion `t{T}s{S}.png` (the
pre-upscaled snap) and overlays:

  * input ChESS corners (small blue dots).
  * candidate marker-cell quads filled by match status:
      green  = expected_id set & score ≥ SCORE_OK_THRESHOLD
      orange = expected_id set & score <  threshold  (weak match)
      gray   = mapped to a black square (no marker expected)
      red    = not sampled (too small / off-image)
  * ChArUco grid edges connecting adjacent (i,j) labelled corners.
  * labelled ChArUco corners (yellow dots with tiny id labels).
  * decoded marker quads drawn as cyan outlines with their id at the
    centroid.
  * header text: chosen / runner-up hypothesis, margin, rejection reason,
    final detection stats.
  * per-bit log-likelihood mini-heatmap for the TOP_N best + TOP_N worst
    cells (identified by expected_score).

Usage:
    uv run python crates/calib-targets-py/examples/overlay_charuco.py \\
        --dir bench_results/charuco/target_0_final \\
        --out bench_results/charuco/target_0_final/overlay
"""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.collections import LineCollection, PatchCollection
from matplotlib.patches import Polygon
from PIL import Image


SCORE_OK_THRESHOLD = -8.0  # per-cell LL ≥ this counts as "strong match" (κ=36 regime)
TOP_N_HEATMAPS = 10


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


def classify_cell(cell: dict[str, Any]) -> str:
    if not cell.get("sampled", False):
        return "#e03b3b"
    expected_id = cell.get("expected_id")
    expected_score = cell.get("expected_score")
    if (
        expected_id is None
        or expected_score is None
        or not math.isfinite(expected_score)
    ):
        return "#8a8a8a"
    if expected_score >= SCORE_OK_THRESHOLD:
        return "#2ca02c"
    return "#ff7f0e"


def draw_cells(ax: plt.Axes, diag: dict[str, Any]) -> None:
    patches = []
    colors = []
    for cell in iter_cells(diag):
        patches.append(Polygon(cell["corners_img"], closed=True))
        colors.append(classify_cell(cell))
    if not patches:
        return
    pc = PatchCollection(patches, alpha=0.28)
    pc.set_facecolor(colors)
    pc.set_edgecolor("black")
    pc.set_linewidth(0.5)
    ax.add_collection(pc)


def draw_markers(ax: plt.Axes, diag: dict[str, Any]) -> None:
    markers = None
    for comp in diag["detect"]["components"]:
        outcome = comp.get("outcome", {})
        if outcome.get("status") != "ok":
            continue
        # pull markers from the top-level result block we added
        break
    result = diag.get("result")
    if not result:
        return
    markers = result.get("markers", [])
    if not markers:
        return
    patches = []
    for m in markers:
        quad = m.get("corners_img")
        if quad:
            patches.append(Polygon(quad, closed=True))
    if patches:
        pc = PatchCollection(patches, alpha=1.0)
        pc.set_facecolor("none")
        pc.set_edgecolor("#00bcd4")
        pc.set_linewidth(1.3)
        ax.add_collection(pc)
    for m in markers:
        quad = m.get("corners_img")
        if not quad:
            continue
        centroid = np.mean(quad, axis=0)
        ax.text(
            centroid[0],
            centroid[1],
            str(m["id"]),
            fontsize=5,
            ha="center",
            va="center",
            color="white",
            bbox=dict(
                boxstyle="round,pad=0.1",
                facecolor="#0097a7",
                edgecolor="none",
                alpha=0.85,
            ),
            zorder=15,
        )


def draw_grid_edges(ax: plt.Axes, diag: dict[str, Any]) -> None:
    result = diag.get("result")
    if not result:
        return
    corners = result.get("corners") or []
    if not corners:
        return
    by_grid: dict[tuple[int, int], tuple[float, float]] = {}
    for c in corners:
        grid = c.get("grid")
        if grid is None:
            continue
        by_grid[(int(grid[0]), int(grid[1]))] = (
            float(c["position"][0]),
            float(c["position"][1]),
        )
    if not by_grid:
        return
    segments: list[list[tuple[float, float]]] = []
    for (i, j), p in by_grid.items():
        for di, dj in ((1, 0), (0, 1)):
            q = by_grid.get((i + di, j + dj))
            if q is None:
                continue
            segments.append([p, q])
    if not segments:
        return
    lc = LineCollection(segments, colors="#ffd54f", linewidths=0.7, alpha=0.9, zorder=12)
    ax.add_collection(lc)


def draw_charuco_corners(ax: plt.Axes, diag: dict[str, Any]) -> None:
    result = diag.get("result")
    if not result:
        return
    corners = result.get("corners") or []
    if not corners:
        return
    xs = [c["position"][0] for c in corners]
    ys = [c["position"][1] for c in corners]
    ax.scatter(
        xs, ys, s=18, c="#ffd54f", edgecolors="#333333", linewidths=0.5, zorder=13
    )
    # Label every corner; if too many, fall back to every 4th.
    stride = 1 if len(corners) <= 40 else 4
    for idx, c in enumerate(corners):
        if idx % stride != 0:
            continue
        cid = c.get("id")
        if cid is None:
            continue
        ax.text(
            c["position"][0] + 3.0,
            c["position"][1] - 3.0,
            str(cid),
            fontsize=5,
            color="#ffd54f",
            zorder=14,
            bbox=dict(
                boxstyle="round,pad=0.05",
                facecolor="black",
                edgecolor="none",
                alpha=0.55,
            ),
        )


def draw_input_corners(ax: plt.Axes, diag: dict[str, Any]) -> None:
    pts = diag.get("input_corners") or []
    if not pts:
        return
    arr = np.asarray(pts, dtype=float)
    ax.scatter(arr[:, 0], arr[:, 1], s=1.5, c="#1f77b4", alpha=0.45, zorder=11)


def inset_heatmap(ax: plt.Axes, cell: dict[str, Any], bits: int, size_px: float) -> None:
    ll = cell.get("expected_bit_ll") or []
    if len(ll) != bits * bits:
        return
    arr = np.array(ll, dtype=float).reshape(bits, bits)
    vmin = min(-6.0, float(arr.min()))
    vmax = 0.0
    centroid = np.mean(cell["corners_img"], axis=0)
    half = size_px * 0.5
    extent = [
        centroid[0] - half,
        centroid[0] + half,
        centroid[1] + half,
        centroid[1] - half,
    ]
    ax.imshow(
        arr,
        cmap="RdYlGn",
        vmin=vmin,
        vmax=vmax,
        extent=extent,
        interpolation="nearest",
        alpha=0.8,
        zorder=10,
    )


def approximate_cell_side(cells: list[dict[str, Any]]) -> float:
    if not cells:
        return 10.0
    sides: list[float] = []
    for c in cells[: min(len(cells), 30)]:
        pts = np.array(c["corners_img"], dtype=float)
        sides.append(float(np.linalg.norm(pts[1] - pts[0])))
        sides.append(float(np.linalg.norm(pts[2] - pts[1])))
    return float(np.median(sides)) if sides else 10.0


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
        f"matcher={matcher}  status={status}  cells={comp.get('candidate_cell_count')}  "
        f"chess_corners={comp.get('chess_corner_count')}",
    ]
    if chosen:
        lines.append(
            f"chosen: rot={chosen['rotation']}  t={tuple(chosen['translation'])}  "
            f"score={chosen['score']:.2f}  contrib={chosen['contributing_cells']}"
        )
    if runner:
        lines.append(
            f"runner: rot={runner['rotation']}  t={tuple(runner['translation'])}  "
            f"score={runner['score']:.2f}  contrib={runner['contributing_cells']}"
        )
    if margin is not None:
        lines.append(f"margin={margin:.4f}")
    if reject:
        lines.append(f"rejection: {reject}")
    if status == "ok":
        lines.append(
            f"detected: markers={outcome.get('markers')}  "
            f"charuco_corners={outcome.get('charuco_corners')}"
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

    # Order matters: cells under grid under markers under labels.
    draw_input_corners(ax, diag)
    draw_cells(ax, diag)

    cells = list(iter_cells(diag))
    scored = [
        c
        for c in cells
        if c.get("expected_score") is not None
        and math.isfinite(c["expected_score"])
    ]
    scored.sort(key=lambda c: c["expected_score"])
    worst = scored[:TOP_N_HEATMAPS]
    best = scored[-TOP_N_HEATMAPS:]
    bits = (
        diag["detect"]["components"][0].get("board", {}).get("bits_per_side", 0)
        if diag["detect"]["components"]
        else 0
    )
    if bits > 0:
        size = 0.5 * approximate_cell_side(cells)
        for cell in (*best, *worst):
            inset_heatmap(ax, cell, bits, size)

    draw_grid_edges(ax, diag)
    draw_markers(ax, diag)
    draw_charuco_corners(ax, diag)

    ax.set_axis_off()
    fig.suptitle(header_text(diag), fontsize=9, ha="left", x=0.01, y=0.995)
    fig.tight_layout(rect=(0, 0, 1, 0.85))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)


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
    p.add_argument(
        "--dir",
        type=Path,
        help="dataset out dir (containing {stem}_diag.json + {stem}.png)",
    )
    p.add_argument(
        "--out",
        type=Path,
        required=True,
        help="output directory (batch) or file (single)",
    )
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
