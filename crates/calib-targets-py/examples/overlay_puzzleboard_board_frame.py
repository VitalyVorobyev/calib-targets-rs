"""Board-frame overlay: 6-camera PuzzleBoard scatter in target coordinates.

Reads `PuzzleboardFrameReport` JSON produced by
`crates/calib-targets-puzzleboard/examples/run_dataset.rs` and produces
one matplotlib figure per *target* (20 in the 130x130 dataset) showing
every snap's labelled corners plotted in the PuzzleBoard board frame
(millimetres), coloured by snap index.

If the detector picked a consistent `(D4, origin)` across all 6
cameras, the scatter is a single roughly-hexagonal covered region
inside the physical board rectangle ``[0, cols × cell] × [0, rows × cell]``.
If different cameras decoded to different D4 transforms or origins
that wrap onto the 501 × 501 master (which happens in `Full` search
mode whenever the board pattern has a cyclically equivalent
alternative placement), the figure shows 2–4 disjoint patches — that
is the diagnostic evidence that per-camera `target_position` values
are *not* in a shared board frame and therefore cannot drive sensor
calibration as-is.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_puzzleboard_board_frame.py \\
        --reports bench_results/130x130_puzzle/phase3 \\
        --out     bench_results/130x130_puzzle/board_frame_overlays_phase3 \\
        [--rows 130 --cols 130 --cell-size 1.014]
"""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

import matplotlib

matplotlib.use("Agg")
import matplotlib.patches as mpatches
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D


SNAP_COLORS = plt.get_cmap("tab10")
MASTER_SIZE = 501


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


def snap_summary(frame: dict[str, Any]) -> dict[str, Any]:
    """Extract the fields we plot / report from one frame JSON."""
    outcome = frame.get("outcome", {})
    if outcome.get("kind") != "ok":
        return {
            "ok": False,
            "stage": outcome.get("stage"),
            "variant": outcome.get("variant"),
            "message": outcome.get("message", ""),
        }
    detection = outcome.get("detection", {})
    decode = outcome.get("decode", {})
    alignment = outcome.get("alignment", {})
    corners = detection.get("corners", [])
    xs: list[float] = []
    ys: list[float] = []
    scores: list[float] = []
    mis: list[int] = []
    mjs: list[int] = []
    for c in corners:
        tp = c.get("target_position")
        if tp is None:
            continue
        xs.append(float(tp[0]))
        ys.append(float(tp[1]))
        scores.append(float(c.get("score", 0.0)))
        g = c.get("grid")
        if g is not None:
            mis.append(int(g["i"]))
            mjs.append(int(g["j"]))
    return {
        "ok": True,
        "xs": np.asarray(xs, dtype=np.float64),
        "ys": np.asarray(ys, dtype=np.float64),
        "scores": np.asarray(scores, dtype=np.float64),
        "n": len(xs),
        "mi_min": min(mis) if mis else None,
        "mi_max": max(mis) if mis else None,
        "mj_min": min(mjs) if mjs else None,
        "mj_max": max(mjs) if mjs else None,
        "transform": transform_signature(alignment.get("transform", {})),
        "origin_row": int(decode.get("master_origin_row", 0)),
        "origin_col": int(decode.get("master_origin_col", 0)),
        "ber": float(decode.get("bit_error_rate", math.nan)),
        "conf": float(decode.get("mean_confidence", math.nan)),
        "edges_matched": int(decode.get("edges_matched", 0)),
        "edges_observed": int(decode.get("edges_observed", 0)),
    }


def marker_sizes(scores: np.ndarray) -> np.ndarray:
    if scores.size == 0:
        return scores
    lo, hi = np.percentile(scores, [5, 95])
    if hi <= lo:
        return np.full_like(scores, 10.0)
    clipped = np.clip(scores, lo, hi)
    return 4.0 + 14.0 * (clipped - lo) / (hi - lo)


def check_consistency(
    summaries: dict[int, dict[str, Any]],
    rows: int,
    cols: int,
) -> tuple[bool, list[str], dict[str, Any]]:
    """Test that every decoded snap's scatter overlaps the others' scatters.

    Calibration needs the 6 cameras of one target to live in a *shared*
    coordinate frame. Under the fixed decoder, FixedBoard outputs are in
    the physical printed-board frame, so overlap now has the direct
    interpretation we actually want: neighbouring snaps should cover
    intersecting board regions. We still use a translation-tolerant
    point-cloud overlap test because the visible footprint is a thin
    strip under oblique view, and axis-aligned bbox overlap is too brittle.
    """
    reasons: list[str] = []
    ok_snaps = {idx: s for idx, s in summaries.items() if s.get("ok")}
    if not ok_snaps:
        return False, ["no snap decoded"], {}

    transforms = {s["transform"] for s in ok_snaps.values()}

    # Union of per-snap master bboxes (the hexagon footprint).
    union_i_lo = min(s["mi_min"] for s in ok_snaps.values() if s["mi_min"] is not None)
    union_i_hi = max(s["mi_max"] for s in ok_snaps.values() if s["mi_max"] is not None)
    union_j_lo = min(s["mj_min"] for s in ok_snaps.values() if s["mj_min"] is not None)
    union_j_hi = max(s["mj_max"] for s in ok_snaps.values() if s["mj_max"] is not None)

    # The 6 cameras sit in a symmetric hexagonal ring around the target,
    # in snap-index order (0 adjacent to 1 and 5, 1 adjacent to 0 and 2,
    # …, 5 adjacent to 4 and 0). So every *sequential* pair
    # (0-1, 1-2, 2-3, 3-4, 4-5, 5-0) must have overlapping scatters in
    # a shared master frame; if any pair is disjoint, the decoder
    # broke cross-camera consistency for that pair.
    #
    # We use a distance-based test rather than axis-aligned bbox overlap
    # because per-camera scatters are elongated strips under oblique
    # projection — two adjacent thin strips can easily have disjoint
    # AABBs while their point clouds cross.
    overlap_threshold_mm = 3.0  # ≈ 3 cells at 1 mm pitch
    min_near_pairs = 5

    def scatter_overlap(a: dict[str, Any], b: dict[str, Any]) -> bool:
        if not (a.get("ok") and b.get("ok")):
            return False
        xa = a["xs"]
        ya = a["ys"]
        xb = b["xs"]
        yb = b["ys"]
        if xa.size == 0 or xb.size == 0:
            return False
        step_a = max(1, xa.size // 200)
        step_b = max(1, xb.size // 200)
        xa_d = xa[::step_a]
        ya_d = ya[::step_a]
        xb_d = xb[::step_b]
        yb_d = yb[::step_b]
        near = 0
        thr2 = overlap_threshold_mm * overlap_threshold_mm
        for x_val, y_val in zip(xa_d, ya_d):
            dx = xb_d - x_val
            dy = yb_d - y_val
            d2 = dx * dx + dy * dy
            near += int(np.count_nonzero(d2 <= thr2))
            if near >= min_near_pairs:
                return True
        return False

    seq_pairs = ((0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0))
    pair_overlaps: list[tuple[int, int, bool]] = []
    for a, b in seq_pairs:
        sa, sb = summaries.get(a), summaries.get(b)
        if sa is None or sb is None or not sa.get("ok") or not sb.get("ok"):
            pair_overlaps.append((a, b, False))
            continue
        pair_overlaps.append((a, b, scatter_overlap(sa, sb)))

    # Also compute the full 6×6 overlap matrix (for diagnostic output).
    all_pair_overlaps: list[tuple[int, int, bool]] = []
    for a in range(6):
        for b in range(a + 1, 6):
            sa, sb = summaries.get(a), summaries.get(b)
            if sa is None or sb is None or not sa.get("ok") or not sb.get("ok"):
                all_pair_overlaps.append((a, b, False))
                continue
            all_pair_overlaps.append((a, b, scatter_overlap(sa, sb)))

    non_overlap = [(a, b) for a, b, ok in pair_overlaps if not ok]
    if non_overlap:
        reasons.append(
            "sequential ring overlap FAILS on pairs: "
            + ", ".join(f"{a}-{b}" for a, b in non_overlap)
        )

    if len(transforms) > 1:
        # Per the PuzzleBoard convention, each camera's local grid (0,0) is
        # tied to its own image orientation — so the D4 transform absorbs
        # the local→master rotation and will usually differ between the
        # 6 cameras. This is info, not a failure.
        reasons.append(
            f"D4 per snap: {sorted(transform_label(t) for t in transforms)} "
            "(expected; each camera has its own local grid convention)"
        )

    if len(ok_snaps) < 6:
        missing = sorted(set(range(6)) - set(ok_snaps.keys()))
        reasons.append(f"only {len(ok_snaps)}/6 snaps decoded (missing: {missing})")

    consistent = not non_overlap and len(ok_snaps) == 6

    stats = {
        "union_i": (union_i_lo, union_i_hi),
        "union_j": (union_j_lo, union_j_hi),
        "pair_overlaps": pair_overlaps,
        "all_pair_overlaps": all_pair_overlaps,
        "n_transforms": len(transforms),
        "n_ok_snaps": len(ok_snaps),
    }
    return consistent, reasons, stats


def plot_target(
    target_idx: int,
    summaries: dict[int, dict[str, Any]],
    rows: int,
    cols: int,
    cell_size: float,
    out_path: Path,
) -> dict[str, Any]:
    fig, ax = plt.subplots(figsize=(9, 9), dpi=110)

    board_w = cols * cell_size
    board_h = rows * cell_size

    # Reference rectangles: the physical board and the 501x501 master.
    master_mm = MASTER_SIZE * cell_size
    ax.add_patch(
        mpatches.Rectangle(
            (0.0, 0.0), master_mm, master_mm,
            fill=False, edgecolor="#888888", lw=0.5, linestyle=":",
            zorder=1, label=None,
        )
    )
    ax.add_patch(
        mpatches.Rectangle(
            (0.0, 0.0), board_w, board_h,
            fill=False, edgecolor="#006600", lw=1.2, linestyle="--",
            zorder=2, label=None,
        )
    )

    total = 0
    n_ok = 0
    handles: list[Line2D] = []
    for snap_idx in range(6):
        summary = summaries.get(snap_idx)
        color = SNAP_COLORS(snap_idx)
        if summary is None:
            handles.append(
                Line2D(
                    [0], [0], marker="o", color=color, lw=0, markersize=6,
                    label=f"s{snap_idx}: missing",
                )
            )
            continue
        if not summary.get("ok"):
            handles.append(
                Line2D(
                    [0], [0], marker="o", color=color, lw=0, markersize=6,
                    label=(
                        f"s{snap_idx}: FAIL {summary.get('stage')}/"
                        f"{summary.get('variant')}"
                    ),
                )
            )
            continue

        xs = summary["xs"]
        ys = summary["ys"]
        total += summary["n"]
        n_ok += 1
        sizes = marker_sizes(summary["scores"])
        ax.scatter(
            xs, ys,
            s=sizes,
            c=[color],
            edgecolors="black",
            linewidths=0.2,
            alpha=0.8,
            zorder=5,
        )
        t_label = transform_label(summary["transform"])
        mi_lo, mi_hi = summary["mi_min"], summary["mi_max"]
        mj_lo, mj_hi = summary["mj_min"], summary["mj_max"]
        handles.append(
            Line2D(
                [0], [0], marker="o", markerfacecolor=color,
                markeredgecolor="black", color=color, lw=0, markersize=7,
                label=(
                    f"s{snap_idx}: n={summary['n']}  D4={t_label}  "
                    f"i∈[{mi_lo},{mi_hi}] j∈[{mj_lo},{mj_hi}]  "
                    f"BER={summary['ber']:.3f}"
                ),
            )
        )

    consistent, reasons, stats = check_consistency(summaries, rows, cols)
    status = "consistent (hexagonal ring overlap)" if consistent else "inconsistent"
    color_status = "#006600" if consistent else "#b00000"

    # Observed footprint: the union bbox of all 6 scatters. When ring-overlap
    # holds this rectangle closely brackets the coloured scatters. It is
    # only the jointly observed region, not the whole physical board
    # (green dashed).
    if consistent and "union_i" in stats:
        u_i_lo, u_i_hi = stats["union_i"]
        u_j_lo, u_j_hi = stats["union_j"]
        ax.add_patch(
            mpatches.Rectangle(
                (u_i_lo * cell_size, u_j_lo * cell_size),
                (u_i_hi - u_i_lo) * cell_size,
                (u_j_hi - u_j_lo) * cell_size,
                fill=False, edgecolor="#0050b0", lw=0.8, linestyle="-",
                zorder=3,
            )
        )
        reasons.append(
            f"union observed bbox i∈[{u_i_lo},{u_i_hi}] j∈[{u_j_lo},{u_j_hi}]"
        )

    # Draw sequential ring edges 0-1, 1-2, 2-3, 3-4, 4-5, 5-0: green when
    # the pair's scatters overlap, red when disjoint. Centroid markers
    # label each snap's mean (i, j) position.
    centroids: dict[int, tuple[float, float]] = {}
    for idx, s in summaries.items():
        if not s.get("ok") or s["xs"].size == 0:
            continue
        centroids[idx] = (float(s["xs"].mean()), float(s["ys"].mean()))
    for a, b, ok in stats.get("pair_overlaps", []):
        if a not in centroids or b not in centroids:
            continue
        xa, ya = centroids[a]
        xb, yb = centroids[b]
        ax.plot(
            [xa, xb], [ya, yb],
            color=("#007a00" if ok else "#c00000"),
            lw=1.2 if ok else 2.0,
            alpha=0.8,
            zorder=4,
        )
    for idx, (cx, cy) in centroids.items():
        ax.plot(cx, cy, marker="o", markersize=4, color="black", zorder=6)
        ax.annotate(
            f"s{idx}", (cx, cy), xytext=(4, -4), textcoords="offset points",
            fontsize=8, color="black", zorder=7,
        )

    ax.set_title(
        f"target {target_idx:02d} — {total} corners across {n_ok}/6 snaps — {status}",
        color=color_status,
        fontsize=11,
    )
    if reasons:
        ax.text(
            0.5, 0.995,
            "\n".join(reasons[:3]),
            transform=ax.transAxes,
            fontsize=8,
            color="white",
            ha="center",
            va="top",
            bbox=dict(
                boxstyle="round,pad=0.3",
                fc=color_status, ec="black", alpha=0.9,
            ),
            zorder=20,
        )

    # Axes extents: zoom to the observed scatter, but keep the printed-board
    # rectangle visible in the same frame for context.
    all_xs = [
        x
        for s in summaries.values()
        if s.get("ok")
        for x in s["xs"].tolist()
    ]
    all_ys = [
        y
        for s in summaries.values()
        if s.get("ok")
        for y in s["ys"].tolist()
    ]
    if all_xs and all_ys:
        sx_lo, sx_hi = min(all_xs), max(all_xs)
        sy_lo, sy_hi = min(all_ys), max(all_ys)
        scatter_span = max(sx_hi - sx_lo, sy_hi - sy_lo, 1.0)
        pad = max(0.10 * scatter_span, 5.0 * cell_size)
        x_lo = sx_lo - pad
        x_hi = sx_hi + pad
        y_lo = sy_lo - pad
        y_hi = sy_hi + pad
        # If the printed-board rectangle is within 1.5× the scatter span,
        # widen to include it as a context anchor. Otherwise, a separate
        # inset is clearer than squishing.
        if max(abs(x_lo), abs(x_hi - board_w)) < 1.5 * scatter_span:
            x_lo = min(x_lo, -pad)
            x_hi = max(x_hi, board_w + pad)
        if max(abs(y_lo), abs(y_hi - board_h)) < 1.5 * scatter_span:
            y_lo = min(y_lo, -pad)
            y_hi = max(y_hi, board_h + pad)
    else:
        pad = pad_default(cell_size)
        x_lo, x_hi = -pad, board_w + pad
        y_lo, y_hi = -pad, board_h + pad

    ax.set_xlim(x_lo, x_hi)
    ax.set_ylim(y_hi, y_lo)  # image convention: y increases downward
    ax.set_aspect("equal")
    ax.set_xlabel("target_position x  [mm]")
    ax.set_ylabel("target_position y  [mm]")
    ax.grid(True, lw=0.3, color="#dddddd", zorder=0)

    handles.append(
        Line2D(
            [0], [0], color="#006600", lw=1.2, linestyle="--",
            label=f"printed board [0,{board_w:.1f}] x [0,{board_h:.1f}] mm",
        )
    )
    handles.append(
        Line2D(
            [0], [0], color="#0050b0", lw=0.8, linestyle="-",
            label="union of 6-camera scatter (hexagon footprint)",
        )
    )
    handles.append(
        Line2D(
            [0], [0], color="#888888", lw=0.5, linestyle=":",
            label=f"master 501x501 [{master_mm:.0f} mm]",
        )
    )
    ax.legend(handles=handles, loc="lower right", fontsize=7, framealpha=0.9)

    fig.tight_layout()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)

    return {
        "consistent": consistent,
        "reasons": reasons,
        "total": total,
        "n_ok": n_ok,
    }


def pad_default(cell_size: float) -> float:
    return 10.0 * cell_size


def plot_overview(
    per_target: dict[int, dict[int, dict[str, Any]]],
    rows: int,
    cols: int,
    cell_size: float,
    out_path: Path,
) -> None:
    indices = sorted(per_target.keys())
    if not indices:
        return
    n_rows = 4
    n_cols = 5
    fig, axes = plt.subplots(n_rows, n_cols, figsize=(16, 13), dpi=110)
    axes = np.asarray(axes).flatten()
    master_mm = MASTER_SIZE * cell_size
    board_w = cols * cell_size
    board_h = rows * cell_size

    for slot, target_idx in enumerate(indices[: n_rows * n_cols]):
        ax = axes[slot]
        summaries = per_target[target_idx]
        ax.add_patch(
            mpatches.Rectangle(
                (0.0, 0.0), master_mm, master_mm,
                fill=False, edgecolor="#888888", lw=0.3, linestyle=":",
            )
        )
        ax.add_patch(
            mpatches.Rectangle(
                (0.0, 0.0), board_w, board_h,
                fill=False, edgecolor="#006600", lw=0.8, linestyle="--",
            )
        )
        for snap_idx, s in summaries.items():
            if not s.get("ok"):
                continue
            ax.scatter(
                s["xs"], s["ys"], s=2.0, c=[SNAP_COLORS(snap_idx)], alpha=0.7,
            )
        consistent, _, _ = check_consistency(summaries, rows, cols)
        ax.set_title(
            f"t{target_idx:02d}",
            color="#006600" if consistent else "#b00000",
            fontsize=9,
        )
        ax.set_aspect("equal")
        ax.set_xlim(-10, master_mm + 10)
        ax.set_ylim(master_mm + 10, -10)
        ax.set_xticks([])
        ax.set_yticks([])

    for slot in range(len(indices), n_rows * n_cols):
        axes[slot].axis("off")

    fig.suptitle(
        f"board-frame overlay index — {len(indices)} targets, printed "
        f"{cols}x{rows} @ {cell_size} mm",
        fontsize=12,
    )
    fig.tight_layout(rect=(0, 0, 1, 0.97))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--reports", required=True, type=Path,
                        help="directory of per-snap PuzzleboardFrameReport JSONs")
    parser.add_argument("--out", required=True, type=Path,
                        help="directory to write per-target PNGs")
    parser.add_argument("--rows", type=int, default=130,
                        help="printed board rows (default 130)")
    parser.add_argument("--cols", type=int, default=130,
                        help="printed board cols (default 130)")
    parser.add_argument("--cell-size", type=float, default=1.014,
                        help="printed board cell size in mm (default 1.014)")
    args = parser.parse_args()

    frame_paths = sorted(
        (p for p in args.reports.iterdir()
         if p.suffix == ".json" and parse_frame_name(p.name) is not None),
        key=lambda p: parse_frame_name(p.name) or (0, 0),
    )
    if not frame_paths:
        raise SystemExit(f"no t{{T}}s{{S}}.json files in {args.reports}")

    per_target: dict[int, dict[int, dict[str, Any]]] = defaultdict(dict)
    for fp in frame_paths:
        ts = parse_frame_name(fp.name)
        if ts is None:
            continue
        target_idx, snap_idx = ts
        per_target[target_idx][snap_idx] = snap_summary(load_frame(fp))

    args.out.mkdir(parents=True, exist_ok=True)
    consistency_lines: list[str] = []
    n_consistent = 0
    n_total = 0
    for target_idx in sorted(per_target.keys()):
        summaries = per_target[target_idx]
        out_path = args.out / f"target_{target_idx:02d}.png"
        info = plot_target(
            target_idx, summaries,
            rows=args.rows, cols=args.cols, cell_size=args.cell_size,
            out_path=out_path,
        )
        n_total += 1
        if info["consistent"]:
            n_consistent += 1
            consistency_lines.append(f"t{target_idx:02d}  OK  n={info['total']}")
        else:
            consistency_lines.append(
                f"t{target_idx:02d}  FAIL  "
                + "; ".join(info["reasons"])
            )

    plot_overview(
        per_target,
        rows=args.rows, cols=args.cols, cell_size=args.cell_size,
        out_path=args.out / "targets_overview.png",
    )

    summary_text = [
        f"reports   : {args.reports}",
        f"board     : {args.cols} x {args.rows} cells @ {args.cell_size} mm",
        f"consistent: {n_consistent}/{n_total} targets",
        "",
        *consistency_lines,
    ]
    (args.out / "per_target_consistency.txt").write_text("\n".join(summary_text) + "\n")
    print("\n".join(summary_text))
    print(f"\nwrote {n_total} per-target figures to {args.out}")


if __name__ == "__main__":
    main()
