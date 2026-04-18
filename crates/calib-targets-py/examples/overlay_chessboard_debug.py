"""Render a detection-stage overlay for a single chessboard debug frame.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_chessboard_debug.py \\
        --image testdata/3536119669/target_0.png --snap 0 --out /tmp/overlay.png

Running without the bindings is a no-op; the script re-runs the instrumented
detector via calib_targets.detect_chessboard_debug, then draws the strong
corners with axis lines, the accepted graph edges, and the labelled grid
coordinates on top of the source image.

Positional arguments are kept minimal on purpose — the overlay is a visual
spot-check, not a batch tool. For batch / regression sweeps use
chessboard_sweep_v2.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image

import calib_targets as ct


def extract_snap(image: np.ndarray, snap_idx: int, snap_w: int, snap_h: int) -> np.ndarray:
    """Crop a horizontal sub-snap from a stacked test image."""
    if image.shape[1] < snap_w * (snap_idx + 1):
        raise ValueError(
            f"image width {image.shape[1]} too small for snap {snap_idx} (need {snap_w * (snap_idx + 1)})"
        )
    x0 = snap_idx * snap_w
    return image[:snap_h, x0 : x0 + snap_w]


def run_debug(
    image: np.ndarray,
    *,
    chess_threshold: float,
    mode: str,
    use_clustering: bool,
    local_prune: bool,
    max_local_h_p95: float | None,
    min_corners: int,
) -> dict[str, Any]:
    """Build the Rust-side payload dict directly — the typed dataclasses
    lag behind the Rust struct on new fields, and for a spot-check script
    the dict path is simpler and stays in sync with Rust automatically."""
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
        "expected_rows": 21,
        "expected_cols": 21,
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
            "mode": mode,
            "min_spacing_pix": 5.0,
            "max_spacing_pix": 50.0,
            "k_neighbors": 8,
            "orientation_tolerance_deg": 22.5,
            "min_step_rel": 0.6,
            "max_step_rel": 1.4,
            "angular_tol_deg": 12.0,
            "step_fallback_pix": 30.0,
        },
        "enable_global_homography_prune": False,
        "local_homography": {
            "enable": local_prune,
            "window_half": 2,
            "min_neighbors": 5,
            "threshold_rel": 0.15,
            "threshold_px_floor": 2.0,
            "max_iters": 16,
        },
    }
    if max_local_h_p95 is not None:
        params_payload["max_local_homography_p95_px"] = float(max_local_h_p95)

    from calib_targets._core import detect_chessboard_debug

    return detect_chessboard_debug(image, chess_cfg=chess_payload, params=params_payload)


def draw_overlay(image: np.ndarray, frame: dict[str, Any], title: str, out_path: Path) -> None:
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)

    strong = frame["strong_corners"]
    neighbors = frame["graph_neighbors"]
    result = frame.get("result")
    metrics = frame["metrics"]
    counts = frame["stage_counts"]

    # Graph edges — one line segment per undirected edge, colored by axis family.
    for src_idx, node_neighbors in enumerate(neighbors):
        for n in node_neighbors:
            dst_idx = n["dst"]
            if dst_idx <= src_idx:
                continue
            a, b = strong[src_idx], strong[dst_idx]
            dx, dy = b["x"] - a["x"], b["y"] - a["y"]
            angle = math.atan2(dy, dx)
            # Assign family by which endpoint axis is closer to the edge.
            dq = abs(((angle - a["axes"][0]["angle"] + math.pi / 2) % math.pi) - math.pi / 2)
            color = "limegreen" if dq < math.pi / 4 else "deepskyblue"
            ax.plot([a["x"], b["x"]], [a["y"], b["y"]], color=color, lw=0.8, alpha=0.6)

    # Strong-corner dots colored by orientation cluster.
    xs = [c["x"] for c in strong]
    ys = [c["y"] for c in strong]
    colors = []
    for c in strong:
        cl = c.get("orientation_cluster")
        if cl == 0:
            colors.append("gold")
        elif cl == 1:
            colors.append("orangered")
        else:
            colors.append("gray")
    ax.scatter(xs, ys, s=18, c=colors, edgecolors="black", linewidths=0.4, zorder=5)

    # Axes overlay: two line segments per corner, length ∝ 1/sigma (clipped).
    for c in strong:
        for axis_idx, axis in enumerate(c["axes"]):
            L = min(12.0, 3.0 / max(axis["sigma"], 0.1))
            dx = math.cos(axis["angle"]) * L
            dy = math.sin(axis["angle"]) * L
            col = "yellow" if axis_idx == 0 else "red"
            ax.plot(
                [c["x"] - dx, c["x"] + dx],
                [c["y"] - dy, c["y"] + dy],
                color=col,
                lw=0.6,
                alpha=0.7,
                zorder=4,
            )

    # Labelled corner grid indices (only the winning detection).
    if result is not None:
        for lc in result["detection"]["corners"]:
            grid = lc["grid"]
            if grid is None:
                continue
            pos = lc["position"]
            ax.text(
                pos[0] + 3,
                pos[1] - 3,
                f"{grid['i']},{grid['j']}",
                fontsize=5,
                color="white",
                bbox=dict(boxstyle="square,pad=0.1", fc="black", ec="none", alpha=0.55),
                zorder=6,
            )

    # Title + metrics panel.
    cov = metrics.get("horizontal_coverage_frac") or 0.0
    local_med = metrics.get("local_homography_residual_median_px")
    local_p95 = metrics.get("local_homography_residual_p95_px")
    res_med = metrics.get("residual_median_px")
    n_final = counts["final_labeled_corners"]
    n_graph = counts["graph_nodes"]
    ax.set_title(
        f"{title}\n"
        f"raw={counts['raw_corners']}  strong={counts['after_strength_filter']}  "
        f"graph={n_graph}  assigned={counts['assigned_grid_corners']}  "
        f"final={n_final}   cov={cov:.3f}\n"
        f"local-H: med={fmt(local_med)} p95={fmt(local_p95)}   global-H: med={fmt(res_med)}",
        fontsize=9,
    )

    # Legend.
    handles = [
        Line2D([0], [0], color="limegreen", lw=2, label="edge (axis-0 family)"),
        Line2D([0], [0], color="deepskyblue", lw=2, label="edge (axis-1 family)"),
        Line2D([0], [0], color="yellow", lw=1, label="axis[0] line"),
        Line2D([0], [0], color="red", lw=1, label="axis[1] line"),
        Line2D(
            [0], [0], marker="o", color="gold", lw=0, markersize=6, label="cluster 0"
        ),
        Line2D(
            [0], [0], marker="o", color="orangered", lw=0, markersize=6, label="cluster 1"
        ),
        Line2D([0], [0], marker="o", color="gray", lw=0, markersize=6, label="no cluster"),
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


def fmt(v: float | None) -> str:
    if v is None:
        return "n/a"
    return f"{v:.2f}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--snap", type=int, default=0)
    parser.add_argument("--snap-width", type=int, default=720)
    parser.add_argument("--snap-height", type=int, default=540)
    parser.add_argument("--chess-threshold", type=float, default=0.12)
    parser.add_argument("--mode", default="two_axis")
    parser.add_argument("--no-clustering", dest="use_clustering", action="store_false")
    parser.add_argument("--no-local-prune", dest="local_prune", action="store_false")
    parser.add_argument("--max-local-h-p95", type=float, default=None)
    parser.add_argument("--min-corners", type=int, default=20)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--title", default=None)
    parser.set_defaults(use_clustering=True, local_prune=True)
    args = parser.parse_args()

    image = np.asarray(Image.open(args.image).convert("L"), dtype=np.uint8)
    if image.shape[1] > args.snap_width:
        snap = extract_snap(image, args.snap, args.snap_width, args.snap_height)
    else:
        snap = image

    frame = run_debug(
        snap,
        chess_threshold=args.chess_threshold,
        mode=args.mode,
        use_clustering=args.use_clustering,
        local_prune=args.local_prune,
        max_local_h_p95=args.max_local_h_p95,
        min_corners=args.min_corners,
    )

    title = args.title or f"{args.image.name}#snap{args.snap}"
    draw_overlay(snap, frame, title, args.out)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
