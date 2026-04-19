"""Batch overlay renderer for the 3536119669 dataset (or any target_*.png set).

Renders only what the detector actually delivers: labeled (i,j) corners
and their grid-neighbor edges. Unlabeled / disconnected ChESS responses
are deliberately omitted — they are not a detection output.

For each frame the overlay also runs a planarity correctness check: no
two grid-neighbor edges may cross except at a shared corner. Any
crossings are counted, highlighted, and reported in the frame title.

Detection recipe:

  - graph mode:             two_axis
  - orientation clustering: ON (two peaks, 30 deg outlier band, 5% min
                               peak weight). Without clustering the
                               marker-interior ChESS responses inject
                               false edges into the graph.
  - global-H prune:         OFF (it over-prunes under lens distortion).
  - local-H prune:          ON (window 2, threshold_rel 0.15, 2 px
                               floor, 16 iters).
  - p95 quality gate:       OFF — gating hides marginal frames instead
                               of showing their partial detection.
  - min corners:            20.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_chessboard_dataset.py \\
        --dataset testdata/3536119669 \\
        --out-dir bench_results/chessboard_3536119669/overlays_all
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path
from typing import Any

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image

import calib_targets as ct  # noqa: F401  -- ensures the bindings import cleanly
from calib_targets._core import detect_chessboard_debug


SNAP_WIDTH = 720
SNAP_HEIGHT = 540
SNAPS_PER_IMAGE = 6


def extract_snap(image: np.ndarray, snap_idx: int) -> np.ndarray:
    if image.shape[1] < SNAP_WIDTH * (snap_idx + 1):
        raise ValueError(
            f"image width {image.shape[1]} too small for snap {snap_idx} "
            f"(need {SNAP_WIDTH * (snap_idx + 1)})"
        )
    x0 = snap_idx * SNAP_WIDTH
    return image[:SNAP_HEIGHT, x0 : x0 + SNAP_WIDTH]


def build_params(
    *,
    chess_threshold: float,
    min_corners: int,
    expected_rows: int,
    expected_cols: int,
    use_clustering: bool,
    local_prune: bool,
    global_prune: bool,
    max_local_h_p95: float | None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    chess_payload = {
        "detector_mode": "canonical",
        "descriptor_mode": "follow_detector",
        "threshold_mode": "relative",
        "threshold_value": chess_threshold,
        "nms_radius": 2,
        "min_cluster_size": 2,
        "refiner": {"method": "none", "refinement_radius": 1},
        "pyramid_levels": 1,
        "pyramid_min_size": 64,
        "refinement_radius": 1,
        "merge_radius": 0.0,
        "upscale": {"mode": "disabled"},
    }
    params_payload = {
        "chess": chess_payload,
        "min_corner_strength": 0.0,
        "min_corners": min_corners,
        "expected_rows": expected_rows,
        "expected_cols": expected_cols,
        "completeness_threshold": 0.05,
        "use_orientation_clustering": use_clustering,
        "orientation_clustering_params": {
            "num_bins": 90,
            "max_iters": 10,
            "peak_min_separation_deg": 10.0,
            "outlier_threshold_deg": 30.0,
            "min_peak_weight_fraction": 0.05,
            "use_weights": True,
        },
        "graph": {
            "mode": "two_axis",
            "min_spacing_pix": 5.0,
            "max_spacing_pix": 50.0,
            "k_neighbors": 8,
            "orientation_tolerance_deg": 22.5,
            "min_step_rel": 0.6,
            "max_step_rel": 1.4,
            "angular_tol_deg": 12.0,
            "step_fallback_pix": 30.0,
        },
        "enable_global_homography_prune": global_prune,
        "local_homography": {
            "enable": local_prune,
            "window_half": 2,
            "min_neighbors": 5,
            "threshold_rel": 0.15,
            "threshold_px_floor": 2.0,
            "max_iters": 16,
        },
        # Phase 1: keep only components with >=8 corners. Smaller components
        # are noise — see docs/120issues.txt.
        "min_component_size": 8,
        # Phase 2: post-graph geometric sanity cleanups. Only active under
        # graph.mode == two_axis.
        "graph_cleanup": {
            "enforce_symmetry": True,
            "enforce_straightness": True,
            "enforce_planarity": True,
            "max_straightness_deg": 15.0,
        },
        # Phase 5: 4-point local-affine gap fill — recover missing corners
        # whose neighbors are already labeled.
        "gap_fill": {
            "enable": True,
            "window_half": 2,
            "min_neighbors": 4,
            "search_rel": 0.4,
            "max_iters": 3,
        },
    }
    if max_local_h_p95 is not None:
        params_payload["max_local_homography_p95_px"] = float(max_local_h_p95)
    return chess_payload, params_payload


def fmt(v: float | None) -> str:
    return "n/a" if v is None else f"{v:.2f}"


# --- Planarity correctness check --------------------------------------------

def _seg_cross_product(ax_: float, ay_: float, bx_: float, by_: float) -> float:
    return ax_ * by_ - ay_ * bx_


def _on_segment(px: float, py: float, ax_: float, ay_: float, bx_: float, by_: float,
                tol: float = 1e-9) -> bool:
    """Return True iff (px, py) lies inside the closed segment AB."""
    minx, maxx = (ax_, bx_) if ax_ <= bx_ else (bx_, ax_)
    miny, maxy = (ay_, by_) if ay_ <= by_ else (by_, ay_)
    return (minx - tol) <= px <= (maxx + tol) and (miny - tol) <= py <= (maxy + tol)


def segments_cross(
    p1: tuple[float, float],
    p2: tuple[float, float],
    p3: tuple[float, float],
    p4: tuple[float, float],
    *,
    endpoint_tol: float = 1e-6,
) -> bool:
    """Return True iff segments P1-P2 and P3-P4 properly cross in their
    interior. Shared endpoints (within endpoint_tol) do NOT count as a
    crossing — they are the legitimate meeting points of a planar graph.
    Collinear overlap is treated as a crossing (it is a real defect)."""
    def same_pt(a: tuple[float, float], b: tuple[float, float]) -> bool:
        return abs(a[0] - b[0]) <= endpoint_tol and abs(a[1] - b[1]) <= endpoint_tol
    if same_pt(p1, p3) or same_pt(p1, p4) or same_pt(p2, p3) or same_pt(p2, p4):
        return False

    d1 = _seg_cross_product(p4[0] - p3[0], p4[1] - p3[1], p1[0] - p3[0], p1[1] - p3[1])
    d2 = _seg_cross_product(p4[0] - p3[0], p4[1] - p3[1], p2[0] - p3[0], p2[1] - p3[1])
    d3 = _seg_cross_product(p2[0] - p1[0], p2[1] - p1[1], p3[0] - p1[0], p3[1] - p1[1])
    d4 = _seg_cross_product(p2[0] - p1[0], p2[1] - p1[1], p4[0] - p1[0], p4[1] - p1[1])

    if ((d1 > 0 and d2 < 0) or (d1 < 0 and d2 > 0)) and \
       ((d3 > 0 and d4 < 0) or (d3 < 0 and d4 > 0)):
        return True

    # Collinear overlap — treat as defect (a grid has no collinear coincident edges).
    if d1 == 0 and _on_segment(p1[0], p1[1], p3[0], p3[1], p4[0], p4[1]):
        return True
    if d2 == 0 and _on_segment(p2[0], p2[1], p3[0], p3[1], p4[0], p4[1]):
        return True
    if d3 == 0 and _on_segment(p3[0], p3[1], p1[0], p1[1], p2[0], p2[1]):
        return True
    if d4 == 0 and _on_segment(p4[0], p4[1], p1[0], p1[1], p2[0], p2[1]):
        return True
    return False


def count_edge_crossings(
    edges: list[tuple[tuple[float, float], tuple[float, float]]],
) -> list[tuple[int, int]]:
    """Return all edge-index pairs that improperly cross."""
    out: list[tuple[int, int]] = []
    for i in range(len(edges)):
        a1, a2 = edges[i]
        for j in range(i + 1, len(edges)):
            b1, b2 = edges[j]
            if segments_cross(a1, a2, b1, b2):
                out.append((i, j))
    return out


# --- Overlay ----------------------------------------------------------------

def _connected_components(
    nodes: list[int], adjacency: dict[int, list[int]]
) -> list[list[int]]:
    """Enumerate connected components over a labeled-corner adjacency dict."""
    seen: set[int] = set()
    comps: list[list[int]] = []
    for start in nodes:
        if start in seen:
            continue
        stack = [start]
        comp: list[int] = []
        while stack:
            node = stack.pop()
            if node in seen:
                continue
            seen.add(node)
            comp.append(node)
            for nb in adjacency.get(node, ()):
                if nb not in seen:
                    stack.append(nb)
        comps.append(comp)
    return comps


def draw_overlay(image: np.ndarray, frame: dict[str, Any], title: str, out_path: Path) -> dict[str, Any]:
    """Draw only labeled corners + grid-neighbor edges. Returns a small
    stats dict with planarity diagnostics for the caller to aggregate."""
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)

    result = frame.get("result")
    metrics = frame["metrics"]
    counts = frame["stage_counts"]

    # Build labeled-corner table keyed by (i, j).
    labeled: list[dict[str, Any]] = []
    if result is not None:
        for lc in result["detection"]["corners"]:
            grid = lc["grid"]
            if grid is None:
                continue
            labeled.append({
                "i": int(grid["i"]),
                "j": int(grid["j"]),
                "x": float(lc["position"][0]),
                "y": float(lc["position"][1]),
            })

    by_ij: dict[tuple[int, int], int] = {(c["i"], c["j"]): idx for idx, c in enumerate(labeled)}
    edges_ax0: list[tuple[int, int]] = []  # varies along i  (|Δi|=1)
    edges_ax1: list[tuple[int, int]] = []  # varies along j  (|Δj|=1)
    adjacency: dict[int, list[int]] = {idx: [] for idx in range(len(labeled))}
    for idx, c in enumerate(labeled):
        nb_i = by_ij.get((c["i"] + 1, c["j"]))
        if nb_i is not None:
            edges_ax0.append((idx, nb_i))
            adjacency[idx].append(nb_i)
            adjacency[nb_i].append(idx)
        nb_j = by_ij.get((c["i"], c["j"] + 1))
        if nb_j is not None:
            edges_ax1.append((idx, nb_j))
            adjacency[idx].append(nb_j)
            adjacency[nb_j].append(idx)

    edge_segments: list[tuple[tuple[float, float], tuple[float, float]]] = []
    for a_idx, b_idx in edges_ax0:
        a, b = labeled[a_idx], labeled[b_idx]
        edge_segments.append(((a["x"], a["y"]), (b["x"], b["y"])))
    for a_idx, b_idx in edges_ax1:
        a, b = labeled[a_idx], labeled[b_idx]
        edge_segments.append(((a["x"], a["y"]), (b["x"], b["y"])))
    n_ax0 = len(edges_ax0)

    crossings = count_edge_crossings(edge_segments)
    crossing_edge_idx: set[int] = set()
    for i, j in crossings:
        crossing_edge_idx.add(i)
        crossing_edge_idx.add(j)

    # Connected components on the accepted graph.
    comps = _connected_components(list(range(len(labeled))), adjacency)
    comps.sort(key=len, reverse=True)
    comp_sizes = [len(c) for c in comps]

    # Render edges.
    for edge_idx, (p1, p2) in enumerate(edge_segments):
        if edge_idx in crossing_edge_idx:
            color = "magenta"
            lw = 1.4
            alpha = 0.9
            zorder = 3
        else:
            color = "limegreen" if edge_idx < n_ax0 else "deepskyblue"
            lw = 0.9
            alpha = 0.8
            zorder = 2
        ax.plot([p1[0], p2[0]], [p1[1], p2[1]], color=color, lw=lw, alpha=alpha, zorder=zorder)

    # Render labeled corner dots + (i,j) labels.
    if labeled:
        xs = [c["x"] for c in labeled]
        ys = [c["y"] for c in labeled]
        ax.scatter(xs, ys, s=22, c="gold", edgecolors="black", linewidths=0.5, zorder=5)
        for c in labeled:
            ax.text(
                c["x"] + 3,
                c["y"] - 3,
                f"{c['i']},{c['j']}",
                fontsize=5,
                color="white",
                bbox=dict(boxstyle="square,pad=0.1", fc="black", ec="none", alpha=0.55),
                zorder=6,
            )

    cov = metrics.get("horizontal_coverage_frac") or 0.0
    local_med = metrics.get("local_homography_residual_median_px")
    local_p95 = metrics.get("local_homography_residual_p95_px")
    n_final = counts["final_labeled_corners"]
    comp_str = "/".join(str(s) for s in comp_sizes[:5]) if comp_sizes else "0"
    ax.set_title(
        f"{title}\n"
        f"labeled={len(labeled)}  edges={len(edge_segments)}  "
        f"components={len(comps)} (sizes={comp_str})  "
        f"crossings={len(crossings)}\n"
        f"cov={cov:.3f}   local-H: med={fmt(local_med)} p95={fmt(local_p95)}   "
        f"stage_final={n_final}",
        fontsize=9,
        color="red" if crossings else "black",
    )

    handles = [
        Line2D([0], [0], color="limegreen", lw=2, label="edge (Δi=1)"),
        Line2D([0], [0], color="deepskyblue", lw=2, label="edge (Δj=1)"),
        Line2D([0], [0], color="magenta", lw=2, label="CROSSING edge (defect)"),
        Line2D([0], [0], marker="o", color="gold", lw=0, markersize=6, label="labeled (i,j) corner"),
    ]
    ax.legend(handles=handles, loc="lower right", fontsize=7, framealpha=0.9)
    ax.set_xlim(0, image.shape[1])
    ax.set_ylim(image.shape[0], 0)
    ax.set_aspect("equal")
    ax.axis("off")
    fig.tight_layout()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)

    return {
        "labeled": len(labeled),
        "edges": len(edge_segments),
        "components": len(comps),
        "component_sizes": comp_sizes,
        "crossings": len(crossings),
    }


def parse_target_index(path: Path) -> int:
    stem = path.stem
    prefix = "target_"
    if not stem.startswith(prefix):
        raise ValueError(f"unexpected filename: {path}")
    return int(stem[len(prefix):])


def collect_targets(dataset: Path) -> list[Path]:
    out = []
    for p in dataset.iterdir():
        if not p.is_file() or p.suffix.lower() != ".png":
            continue
        stem = p.stem
        if not stem.startswith("target_") or " " in stem:
            continue
        try:
            parse_target_index(p)
        except ValueError:
            continue
        out.append(p)
    out.sort(key=parse_target_index)
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dataset", required=True, type=Path,
                        help="directory containing target_*.png files")
    parser.add_argument("--out-dir", required=True, type=Path,
                        help="where to write the overlay PNGs")
    parser.add_argument("--chess-threshold", type=float, default=0.12)
    parser.add_argument("--min-corners", type=int, default=20)
    parser.add_argument("--expected-rows", type=int, default=21)
    parser.add_argument("--expected-cols", type=int, default=21)
    parser.add_argument("--no-clustering", dest="use_clustering", action="store_false",
                        help="disable orientation clustering (default: enabled)")
    parser.add_argument("--no-local-prune", dest="local_prune", action="store_false")
    parser.add_argument("--global-prune", dest="global_prune", action="store_true",
                        help="enable global-H residual prune (default: off)")
    parser.add_argument("--max-local-h-p95", type=float, default=None,
                        help="post-prune p95 gate; default off for visualization")
    parser.add_argument("--tag", default="best",
                        help="tag appended to every output filename (default: best)")
    parser.set_defaults(use_clustering=True, local_prune=True, global_prune=False)
    args = parser.parse_args()

    targets = collect_targets(args.dataset)
    if not targets:
        raise SystemExit(f"no target_*.png files in {args.dataset}")

    _, params_payload = build_params(
        chess_threshold=args.chess_threshold,
        min_corners=args.min_corners,
        expected_rows=args.expected_rows,
        expected_cols=args.expected_cols,
        use_clustering=args.use_clustering,
        local_prune=args.local_prune,
        global_prune=args.global_prune,
        max_local_h_p95=args.max_local_h_p95,
    )
    chess_payload = params_payload["chess"]

    args.out_dir.mkdir(parents=True, exist_ok=True)
    print(
        f"dataset={args.dataset}  targets={len(targets)}  "
        f"snaps_per_image={SNAPS_PER_IMAGE}  total={len(targets) * SNAPS_PER_IMAGE}"
    )
    print(
        "config: clustering={}  local_prune={}  global_prune={}  p95_gate={}".format(
            args.use_clustering,
            args.local_prune,
            args.global_prune,
            args.max_local_h_p95,
        )
    )

    n_frames = 0
    n_detected = 0
    n_with_crossings = 0
    total_crossings = 0
    per_frame_rows: list[str] = []
    for path in targets:
        target_idx = parse_target_index(path)
        image = np.asarray(Image.open(path).convert("L"), dtype=np.uint8)
        for snap_idx in range(SNAPS_PER_IMAGE):
            snap = extract_snap(image, snap_idx)
            frame = detect_chessboard_debug(snap, chess_cfg=chess_payload, params=params_payload)
            title = f"t{target_idx}s{snap_idx}  {args.tag}"
            out_path = args.out_dir / f"t{target_idx}s{snap_idx}_{args.tag}.png"
            stats = draw_overlay(snap, frame, title, out_path)
            n_frames += 1
            if frame.get("result") is not None:
                n_detected += 1
            if stats["crossings"] > 0:
                n_with_crossings += 1
                total_crossings += stats["crossings"]
            per_frame_rows.append(
                "t{t}s{s}\tlabeled={lab}\tedges={e}\tcomp={c}\tsizes={sz}\tcrossings={x}".format(
                    t=target_idx,
                    s=snap_idx,
                    lab=stats["labeled"],
                    e=stats["edges"],
                    c=stats["components"],
                    sz=",".join(str(x) for x in stats["component_sizes"]) or "-",
                    x=stats["crossings"],
                )
            )

    pct = (100.0 * n_detected / n_frames) if n_frames else 0.0
    print(
        f"wrote {n_frames} overlays to {args.out_dir}"
        f"  (detected={n_detected}  rate={pct:.1f}%)"
    )
    print(
        f"planarity check: {n_with_crossings}/{n_frames} frames have edge crossings "
        f"(total defective-edge pairs: {total_crossings})"
    )

    summary_path = args.out_dir / f"_{args.tag}_summary.tsv"
    summary_path.write_text("\n".join(per_frame_rows) + "\n")
    print(f"per-frame stats: {summary_path}")


if __name__ == "__main__":
    main()
