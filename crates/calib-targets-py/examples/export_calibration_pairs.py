"""Export PuzzleBoard detection results to a calibration-ready CSV.

Reads `PuzzleboardFrameReport` JSON produced by
`crates/calib-targets-puzzleboard/examples/run_dataset.rs` and emits one
CSV row per labeled corner:

    target_idx,snap_idx,corner_id,pixel_x,pixel_y,board_x,board_y,score,grid_i,grid_j,d4_transform,master_origin_row,master_origin_col

Pitfalls this script helps surface (the three most common reasons a
"successful" PuzzleBoard detection fails downstream calibration):

 1. **Pixel-coordinate unit mismatch.** The detector runs on the
    upscaled image (`upscale` factor in each report, typically 2 on
    this dataset). Pixel positions are therefore in *upscaled* units.
    Pass `--native-pixels` to divide them by the per-frame upscale so
    the CSV contains native-image pixels, which is what most
    calibration frameworks expect.

 2. **Board-frame semantics.** For FixedBoard decoding,
    `target_position` now lives in the physical printed-board frame.
    If exported rows land outside `[0, cols) × [0, rows)`, treat that
    as a real signal: either the run used `Full` search, the declared
    board spec does not match the printed target, or the decode is
    still wrong on that frame.

 3. **Cross-camera consistency.** Even after the decoder-frame fix,
    weak or narrow views can still fail to overlap their neighbours in
    board frame. We emit a per-target diagnostic showing which
    sequential pairs (0-1, 1-2, …, 5-0) don't overlap. Any row flagged
    `not_coherent_with_ring=True` comes from a snap whose decoded board
    placement disagrees with its ring neighbours.

Usage:
    uv run python crates/calib-targets-py/examples/export_calibration_pairs.py \\
        --reports bench_results/130x130_puzzle/phase4_fixed \\
        --out     bench_results/130x130_puzzle/calib_pairs.csv \\
        [--native-pixels] [--target-unit mm|m] [--rows 130 --cols 130]
"""

from __future__ import annotations

import argparse
import csv
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

import numpy as np


def parse_frame_name(name: str) -> tuple[int, int] | None:
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


def transform_signature(transform: dict[str, int]) -> tuple[int, int, int, int]:
    return (
        int(transform.get("a", 0)),
        int(transform.get("b", 0)),
        int(transform.get("c", 0)),
        int(transform.get("d", 0)),
    )


def transform_label(sig: tuple[int, int, int, int]) -> str:
    known = {
        (1, 0, 0, 1): "I",
        (-1, 0, 0, -1): "R180",
        (0, -1, 1, 0): "R90",
        (0, 1, -1, 0): "R270",
        (1, 0, 0, -1): "Mx",
        (-1, 0, 0, 1): "My",
        (0, 1, 1, 0): "Md+",
        (0, -1, -1, 0): "Md-",
    }
    return known.get(sig, f"({sig[0]},{sig[1]};{sig[2]},{sig[3]})")


def scatter_overlap(
    xa: np.ndarray, ya: np.ndarray, xb: np.ndarray, yb: np.ndarray,
    threshold_mm: float = 3.0, min_near: int = 5,
) -> bool:
    if xa.size == 0 or xb.size == 0:
        return False
    step_a = max(1, xa.size // 200)
    step_b = max(1, xb.size // 200)
    xa_d = xa[::step_a]
    ya_d = ya[::step_a]
    xb_d = xb[::step_b]
    yb_d = yb[::step_b]
    thr2 = threshold_mm * threshold_mm
    near = 0
    for x0, y0 in zip(xa_d, ya_d):
        dx = xb_d - x0
        dy = yb_d - y0
        d2 = dx * dx + dy * dy
        near += int(np.count_nonzero(d2 <= thr2))
        if near >= min_near:
            return True
    return False


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--reports", required=True, type=Path,
                        help="directory of per-snap PuzzleboardFrameReport JSONs")
    parser.add_argument("--out", required=True, type=Path,
                        help="output CSV path")
    parser.add_argument("--native-pixels", action="store_true",
                        help="divide pixel_x / pixel_y by each frame's upscale factor")
    parser.add_argument("--target-unit", choices=("mm", "m"), default="mm",
                        help="unit for board_x / board_y (default mm)")
    parser.add_argument("--rows", type=int, default=130,
                        help="printed board rows (for out-of-board counting)")
    parser.add_argument("--cols", type=int, default=130,
                        help="printed board cols (for out-of-board counting)")
    parser.add_argument("--cell-size", type=float, default=1.014,
                        help="printed board cell size in mm (default 1.014)")
    parser.add_argument("--overlap-threshold-mm", type=float, default=3.0,
                        help="distance threshold for sequential ring overlap test")
    args = parser.parse_args()

    frame_paths = sorted(
        (p for p in args.reports.iterdir()
         if p.suffix == ".json" and parse_frame_name(p.name) is not None),
        key=lambda p: parse_frame_name(p.name) or (0, 0),
    )
    if not frame_paths:
        raise SystemExit(f"no t{{T}}s{{S}}.json files in {args.reports}")

    # Load all per-snap summaries once; we need them for ring-overlap
    # analysis before writing rows.
    frames: dict[tuple[int, int], dict[str, Any]] = {}
    for fp in frame_paths:
        ts = parse_frame_name(fp.name)
        if ts is None:
            continue
        frames[ts] = load_frame(fp)

    # Pre-compute per-(target, snap) scatter arrays (for ring overlap).
    scatters: dict[tuple[int, int], tuple[np.ndarray, np.ndarray]] = {}
    for (target_idx, snap_idx), frame in frames.items():
        outcome = frame.get("outcome", {})
        if outcome.get("kind") != "ok":
            continue
        corners = outcome.get("detection", {}).get("corners", [])
        xs = [float(c["target_position"][0])
              for c in corners if c.get("target_position") is not None]
        ys = [float(c["target_position"][1])
              for c in corners if c.get("target_position") is not None]
        if xs:
            scatters[(target_idx, snap_idx)] = (
                np.asarray(xs, dtype=np.float64),
                np.asarray(ys, dtype=np.float64),
            )

    # Ring-overlap matrix per target: snap_idx → set of ring neighbours with
    # non-overlapping scatters.
    per_target_broken: dict[int, set[tuple[int, int]]] = defaultdict(set)
    per_target_pair_status: dict[int, dict[tuple[int, int], bool]] = defaultdict(dict)
    targets = sorted({t for (t, _) in frames.keys()})
    for target_idx in targets:
        for a, b in ((0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)):
            sa = scatters.get((target_idx, a))
            sb = scatters.get((target_idx, b))
            if sa is None or sb is None:
                per_target_pair_status[target_idx][(a, b)] = False
                per_target_broken[target_idx].add((a, b))
                continue
            ok = scatter_overlap(
                sa[0], sa[1], sb[0], sb[1],
                threshold_mm=args.overlap_threshold_mm,
            )
            per_target_pair_status[target_idx][(a, b)] = ok
            if not ok:
                per_target_broken[target_idx].add((a, b))

    # ---------------- CSV emission ----------------

    unit_scale = 1.0 if args.target_unit == "mm" else 1e-3
    args.out.parent.mkdir(parents=True, exist_ok=True)
    rows_written = 0
    rows_out_of_board = 0
    pixel_cell_medians: dict[int, list[float]] = defaultdict(list)
    with args.out.open("w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow([
            "target_idx", "snap_idx", "corner_id",
            "pixel_x", "pixel_y",
            "board_x", "board_y",
            "score",
            "grid_i", "grid_j",
            "d4_transform",
            "master_origin_row", "master_origin_col",
            "ring_neighbour_broken",
        ])
        for (target_idx, snap_idx), frame in sorted(frames.items()):
            outcome = frame.get("outcome", {})
            if outcome.get("kind") != "ok":
                continue
            detection = outcome.get("detection", {})
            decode = outcome.get("decode", {})
            alignment = outcome.get("alignment", {})
            t_sig = transform_signature(alignment.get("transform", {}))
            t_label = transform_label(t_sig)
            origin_row = int(decode.get("master_origin_row", 0))
            origin_col = int(decode.get("master_origin_col", 0))
            upscale = int(frame.get("upscale", 1))
            px_divisor = upscale if args.native_pixels else 1

            # Is this snap a ring-neighbour of a broken pair?
            neighbours_broken = any(
                (snap_idx == a or snap_idx == b)
                for (a, b) in per_target_broken.get(target_idx, set())
            )

            # Collect pixel positions for per-snap cell-size consistency check.
            positions_px: list[tuple[float, float, int, int]] = []

            for c in detection.get("corners", []):
                grid = c.get("grid")
                tp = c.get("target_position")
                corner_id = c.get("id")
                if tp is None or grid is None or corner_id is None:
                    continue
                px = float(c["position"][0]) / px_divisor
                py = float(c["position"][1]) / px_divisor
                bx = float(tp[0]) * unit_scale
                by = float(tp[1]) * unit_scale
                gi = int(grid["i"])
                gj = int(grid["j"])
                mi = int(gi)
                mj = int(gj)
                writer.writerow([
                    target_idx, snap_idx, int(corner_id),
                    f"{px:.4f}", f"{py:.4f}",
                    f"{bx:.4f}", f"{by:.4f}",
                    f"{float(c.get('score', 0.0)):.3f}",
                    gi, gj, t_label,
                    origin_row, origin_col,
                    int(neighbours_broken),
                ])
                rows_written += 1
                if not (0 <= mi < args.cols and 0 <= mj < args.rows):
                    rows_out_of_board += 1
                positions_px.append((px, py, gi, gj))

            # Per-snap median adjacent-corner pixel spacing.
            if len(positions_px) >= 2:
                by_grid = {(g[2], g[3]): (g[0], g[1]) for g in positions_px}
                dists: list[float] = []
                for (gi, gj), (px, py) in by_grid.items():
                    for (di, dj) in ((1, 0), (0, 1)):
                        neigh = by_grid.get((gi + di, gj + dj))
                        if neigh is None:
                            continue
                        dists.append(
                            float(np.hypot(neigh[0] - px, neigh[1] - py))
                        )
                if dists:
                    pixel_cell_medians[snap_idx].append(float(np.median(dists)))

    # ---------------- Diagnostic report ----------------

    print(f"wrote {rows_written} rows to {args.out}")
    print(f"upscale handling: {'native (divided by per-frame upscale)' if args.native_pixels else 'raw (kept upscaled)'}")
    print(f"board coords unit: {args.target_unit}")
    print()

    # Per-target ring overlap summary.
    print("per-target sequential ring overlap:")
    n_consistent = 0
    for target_idx in targets:
        pairs = per_target_pair_status.get(target_idx, {})
        broken = [f"{a}-{b}" for (a, b), ok in pairs.items() if not ok]
        tag = "OK" if not broken else f"BROKEN: {', '.join(broken)}"
        if not broken:
            n_consistent += 1
        print(f"  t{target_idx:02d}  {tag}")
    print(f"\n  {n_consistent}/{len(targets)} targets fully consistent (all 6 ring pairs overlap)")
    print()

    # Out-of-board count.
    print(
        f"rows with master-(i, j) outside [0, {args.cols}) × [0, {args.rows}): "
        f"{rows_out_of_board}/{rows_written}  "
        f"({100.0 * rows_out_of_board / max(rows_written, 1):.1f}%)"
    )
    print(
        "  (Under FixedBoard decoding this should usually stay near zero. "
        "Non-zero rows here mean the export is no longer in the declared "
        "physical board frame: check the chosen search mode, board spec, "
        "or weak-view decode quality.)"
    )
    print()

    # Per-snap pixel cell-size medians (detects upscale mistakes).
    if pixel_cell_medians:
        print("per-snap pixel cell-size (median adjacent-corner pixel spacing):")
        for snap_idx, medians in sorted(pixel_cell_medians.items()):
            arr = np.asarray(medians, dtype=np.float64)
            expected = args.cell_size  # 1 cell in mm, before pixel scaling
            print(
                f"  s{snap_idx}: "
                f"{len(medians)} frames, "
                f"median={np.median(arr):.2f} px, "
                f"p10={np.percentile(arr, 10):.2f} px, "
                f"p90={np.percentile(arr, 90):.2f} px"
            )
        print(
            f"  (If one snap's median is ≈ 2× the others', the upscale "
            f"correction wasn't applied to its pixels. Expected baseline "
            f"per 1 mm cell on the 720×540 native frame ≈ 6-12 px depending "
            f"on magnification; native pixels after --native-pixels.)"
        )


if __name__ == "__main__":
    main()
