#!/usr/bin/env python3
"""Render standard blog overlays for the projective-grid topological path.

The script uses ``calib_targets.trace_chessboard_topological`` so every
intermediate stage comes from the Rust implementation rather than a parallel
Python copy of the algorithm.

Output layout:

    docs/img/02-topo-grid/<image-stem>/00-input.png
    docs/img/02-topo-grid/<image-stem>/01-corners-axes.png
    ...
    docs/img/02-topo-grid/<image-stem>/09-final-grid.png
    docs/img/02-topo-grid/manifest.json
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any, Callable

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image

import calib_targets as ct


STAGES = [
    "00-input.png",
    "01-corners-axes.png",
    "02-usable-corners.png",
    "03-delaunay-edge-kinds.png",
    "04-mergeable-triangles.png",
    "05-raw-quads.png",
    "06-topology-filter.png",
    "07-geometry-filter.png",
    "08-walk-labels.png",
    "09-final-grid.png",
]

EDGE_COLORS = {
    "grid": "#1b9e77",
    "diagonal": "#377eb8",
    "spurious": "#d95f02",
}

TRI_COLORS = {
    "mergeable": "#2ca25f",
    "all_grid": "#756bb1",
    "multi_diagonal": "#3182bd",
    "has_spurious": "#de2d26",
}


def load_gray(path: Path, upscale: float = 1.0) -> np.ndarray:
    image = Image.open(path).convert("L")
    if upscale != 1.0:
        if upscale <= 0.0:
            raise ValueError("--upscale must be positive")
        width, height = image.size
        image = image.resize(
            (max(1, round(width * upscale)), max(1, round(height * upscale))),
            Image.Resampling.BICUBIC,
        )
    return np.asarray(image, dtype=np.uint8)


def save_input(image: np.ndarray, out_path: Path) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(image).save(out_path)


def make_figure(image: np.ndarray) -> tuple[plt.Figure, plt.Axes]:
    h, w = image.shape
    dpi = 140
    fig_w = min(12.0, max(6.0, w / dpi))
    fig_h = fig_w * h / w
    fig, ax = plt.subplots(figsize=(fig_w, fig_h), dpi=dpi)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)
    ax.set_xlim(0, w)
    ax.set_ylim(h, 0)
    ax.set_aspect("equal")
    ax.axis("off")
    return fig, ax


def save_overlay(
    image: np.ndarray,
    out_path: Path,
    title: str,
    draw: Callable[[plt.Axes], None],
    legend: list[Line2D] | None = None,
) -> None:
    fig, ax = make_figure(image)
    draw(ax)
    # ax.set_title(title, fontsize=9)
    if legend:
        ax.legend(handles=legend, loc="lower right", fontsize=6, framealpha=0.9)
    fig.tight_layout(pad=0.05)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight", pad_inches=0.02)
    plt.close(fig)


def corner_positions(payload: dict[str, Any]) -> dict[int, tuple[float, float]]:
    return {
        int(c["index"]): (float(c["position"][0]), float(c["position"][1]))
        for c in payload["corners"]
    }


def draw_corner_axes(ax: plt.Axes, payload: dict[str, Any]) -> None:
    corners = payload["corners"]
    if not corners:
        return
    xs = [c["position"][0] for c in corners]
    ys = [c["position"][1] for c in corners]
    ax.scatter(xs, ys, s=12, c="#fdd835", edgecolors="black", linewidths=0.25, zorder=4)
    for c in corners:
        x, y = c["position"]
        for axis, color in zip(c["axes"], ("#00e5ff", "#ff4081")):
            sigma = max(float(axis["sigma"]), 0.05)
            length = min(14.0, 5.0 / sigma)
            theta = float(axis["angle"])
            dx = math.cos(theta) * length
            dy = math.sin(theta) * length
            ax.plot([x - dx, x + dx], [y - dy, y + dy], color=color, lw=0.55, alpha=0.75)


def usable_indices(payload: dict[str, Any]) -> set[int]:
    trace = payload.get("trace")
    if trace is None:
        return set()
    return {int(c["index"]) for c in trace["corners"] if c.get("usable")}


def draw_usable(
    ax: plt.Axes,
    payload: dict[str, Any],
    *,
    only_usable: bool = False,
    show_unusable: bool = True,
) -> None:
    """Plot corner markers.

    `only_usable`: draw only the usable subset (post-Stage-2 plots).
    `show_unusable`: when both flags are False, render red unusable
        corners alongside green usable ones (Stage 2 itself).
    """
    trace = payload.get("trace")
    pos = corner_positions(payload)
    if trace is None:
        draw_corner_axes(ax, payload)
        return
    usable_set = usable_indices(payload)
    for idx, (x, y) in pos.items():
        is_usable = idx in usable_set
        if only_usable and not is_usable:
            continue
        if not is_usable and not show_unusable:
            continue
        color = "#2ca25f" if is_usable else "#de2d26"
        ax.scatter([x], [y], s=15, c=color, edgecolors="black", linewidths=0.25, zorder=4)


def unique_edges(trace: dict[str, Any]) -> list[tuple[int, int, str]]:
    seen: dict[tuple[int, int], str] = {}
    for tri in trace["triangles"]:
        vertices = tri["vertices"]
        for k, kind in enumerate(tri["edge_kinds"]):
            a = vertices[k]
            b = vertices[(k + 1) % 3]
            key = (a, b) if a < b else (b, a)
            seen.setdefault(key, kind)
    return [(a, b, kind) for (a, b), kind in seen.items()]


def draw_delaunay(ax: plt.Axes, payload: dict[str, Any]) -> None:
    trace = payload.get("trace")
    if trace is None:
        draw_usable(ax, payload)
        return
    pos = corner_positions(payload)
    usable_set = usable_indices(payload)
    for a, b, kind in unique_edges(trace):
        if a not in pos or b not in pos:
            continue
        # Skip Delaunay edges to / from unusable corners — they would only
        # ever classify as Spurious by the per-endpoint rule and clutter
        # the plot with background noise.
        if a not in usable_set or b not in usable_set:
            continue
        x0, y0 = pos[a]
        x1, y1 = pos[b]
        ax.plot([x0, x1], [y0, y1], color=EDGE_COLORS[kind], lw=0.6, alpha=0.75)
    draw_usable(ax, payload, only_usable=True)


def draw_triangles(ax: plt.Axes, payload: dict[str, Any]) -> None:
    trace = payload.get("trace")
    if trace is None:
        draw_usable(ax, payload)
        return
    pos = corner_positions(payload)
    usable_set = usable_indices(payload)
    for tri in trace["triangles"]:
        if not all(v in usable_set for v in tri["vertices"]):
            continue
        pts = [pos[v] for v in tri["vertices"] if v in pos]
        if len(pts) != 3:
            continue
        color = TRI_COLORS[tri["class"]]
        xs, ys = zip(*(pts + [pts[0]]))
        alpha = 0.18 if tri["class"] == "mergeable" else 0.07
        ax.fill(xs, ys, color=color, alpha=alpha)
        ax.plot(xs, ys, color=color, lw=0.45, alpha=0.45)
    draw_usable(ax, payload, only_usable=True)


def draw_quads(ax: plt.Axes, payload: dict[str, Any], mode: str) -> None:
    trace = payload.get("trace")
    if trace is None:
        draw_usable(ax, payload)
        return
    pos = corner_positions(payload)
    for quad in trace["quads"]:
        pts = [pos[v] for v in quad["vertices"] if v in pos]
        if len(pts) != 4:
            continue
        xs, ys = zip(*(pts + [pts[0]]))
        if mode == "raw":
            color, alpha, lw = "#2b8cbe", 0.95, 0.8
        elif mode == "topology":
            color = "#2ca25f" if quad["topology_pass"] else "#de2d26"
            alpha, lw = 0.9, 0.85
        else:
            if quad["kept"]:
                color = "#2ca25f"
            elif quad["topology_pass"]:
                color = "#fdae6b"
            else:
                color = "#de2d26"
            alpha, lw = 0.9, 0.85
        ax.plot(xs, ys, color=color, lw=lw, alpha=alpha)
    draw_usable(ax, payload, only_usable=True)


def component_labels_from_trace(payload: dict[str, Any]) -> list[dict[str, Any]]:
    trace = payload.get("trace")
    if trace is None:
        return []
    labels: list[dict[str, Any]] = []
    for comp in trace["components"]:
        labels.extend(comp["labels"])
    return labels


def draw_grid_labels(
    ax: plt.Axes,
    payload: dict[str, Any],
    labels: list[dict[str, Any]],
    color_i: str = "#1b9e77",
    color_j: str = "#377eb8",
) -> None:
    pos = corner_positions(payload)
    by_grid = {(int(l["i"]), int(l["j"])): int(l["corner_idx"]) for l in labels}
    for (i, j), idx in by_grid.items():
        if idx not in pos:
            continue
        x, y = pos[idx]
        for nb, color in [((i + 1, j), color_i), ((i, j + 1), color_j)]:
            nb_idx = by_grid.get(nb)
            if nb_idx is None or nb_idx not in pos:
                continue
            xn, yn = pos[nb_idx]
            ax.plot([x, xn], [y, yn], color=color, lw=0.9, alpha=0.9)
    for (i, j), idx in by_grid.items():
        if idx not in pos:
            continue
        x, y = pos[idx]
        ax.scatter([x], [y], s=18, c="#fdd835", edgecolors="black", linewidths=0.35, zorder=4)
        ax.text(
            x + 2,
            y - 2,
            f"{i},{j}",
            fontsize=4.5,
            color="white",
            bbox=dict(boxstyle="square,pad=0.08", fc="black", ec="none", alpha=0.55),
            zorder=5,
        )


def draw_walk(ax: plt.Axes, payload: dict[str, Any]) -> None:
    """Stage 8: per-component projective-grid walk labels.

    Renders every labelled component produced by `build_grid_topological`
    directly from the projective-grid trace. Each component carries an
    independent `(i, j)` origin, so components with overlapping labels
    can co-exist in the plot without a merge step. Stage 9 below shows
    what's left after the chessboard adapter's mandatory final geometry
    gate runs over these.
    """
    draw_grid_labels(ax, payload, component_labels_from_trace(payload))


def detection_grid_points(payload: dict[str, Any]) -> dict[tuple[int, int], tuple[float, float]]:
    """Largest detection from the trace payload, after geometry check.

    The Rust trace endpoint runs the full chessboard detector with
    `GraphBuildAlgorithm::Topological` alongside the per-stage trace and
    pickles the resulting `Detection`s into the payload. We pick the
    first (largest by labelled-corner count) — same selection
    `Detector::detect()` makes — so Stage 9 reflects the precision-gated
    output a calibration consumer would see, *not* the raw topological
    walk.
    """
    detections = payload.get("detections") or []
    if not detections:
        return {}
    by_grid: dict[tuple[int, int], tuple[float, float]] = {}
    for corner in detections[0]["target"]["corners"]:
        grid = corner.get("grid")
        pos = corner.get("position")
        if grid is None or pos is None:
            continue
        by_grid[(int(grid["i"]), int(grid["j"]))] = (float(pos[0]), float(pos[1]))
    return by_grid


def draw_final(ax: plt.Axes, payload: dict[str, Any]) -> None:
    """Stage 9: final detection emitted by the chessboard adapter.

    The chessboard adapter consumes the topological walk labels (Stage
    8 above), runs `merge_components_local` on the per-component
    output, then runs the same `run_geometry_check` precision gate that
    chessboard-v2 uses (line collinearity / local-H residual / largest
    cardinally-connected component). Anything that survives is what the
    public `Detection` carries; that's what's drawn here.
    """
    by_grid = detection_grid_points(payload)
    if not by_grid:
        # Fallback to the largest topological component when the
        # adapter refused to ship a detection (mostly diagnostic
        # frames with too few labelled corners).
        labels = component_labels_from_trace(payload)
        pos = corner_positions(payload)
        for entry in labels:
            idx = int(entry["corner_idx"])
            if idx not in pos:
                continue
            by_grid[(int(entry["i"]), int(entry["j"]))] = pos[idx]
    for (i, j), (x, y) in by_grid.items():
        for nb, color in [((i + 1, j), "#1b9e77"), ((i, j + 1), "#377eb8")]:
            p = by_grid.get(nb)
            if p is None:
                continue
            ax.plot([x, p[0]], [y, p[1]], color=color, lw=1.05, alpha=0.95)
    for (i, j), (x, y) in by_grid.items():
        ax.scatter([x], [y], s=20, c="#fdd835", edgecolors="black", linewidths=0.4, zorder=4)
        ax.text(
            x + 2,
            y - 2,
            f"{i},{j}",
            fontsize=4.5,
            color="white",
            bbox=dict(boxstyle="square,pad=0.08", fc="black", ec="none", alpha=0.55),
            zorder=5,
        )


def render_image(path: Path, out_dir: Path, args: argparse.Namespace) -> dict[str, Any]:
    image = load_gray(path, args.upscale)
    topo = ct.TopologicalParams(
        axis_align_tol_rad=math.radians(args.axis_align_tol_deg),
        diagonal_angle_tol_rad=math.radians(args.diagonal_angle_tol_deg),
        max_axis_sigma_rad=math.radians(args.max_axis_sigma_deg),
        edge_ratio_max=args.edge_ratio_max,
        min_quads_per_component=args.min_quads_per_component,
        cluster_axis_tol_rad=math.radians(args.cluster_axis_tol_deg),
        quad_edge_min_rel=args.quad_edge_min_rel,
        quad_edge_max_rel=args.quad_edge_max_rel,
    )
    trace_params = ct.ChessboardParams(graph_build_algorithm="topological", topological=topo)
    chess_cfg = ct.ChessConfig(threshold_value=args.chess_threshold)
    payload = ct.trace_chessboard_topological(image, chess_cfg=chess_cfg, params=trace_params)
    if args.final_algorithm != "topological":
        final_params = ct.ChessboardParams(graph_build_algorithm=args.final_algorithm)
        payload["detections"] = [
            detection.to_dict()
            for detection in ct.detect_chessboard_all(
                image,
                chess_cfg=chess_cfg,
                params=final_params,
            )
        ]
        payload["final_graph_build_algorithm"] = args.final_algorithm

    stem = f"{path.stem}-{args.variant_name}" if args.variant_name else path.stem
    stem_dir = out_dir / stem
    save_input(image, stem_dir / STAGES[0])
    save_overlay(
        image,
        stem_dir / STAGES[1],
        f"{path.name}: ChESS corners + local axes",
        lambda ax: draw_corner_axes(ax, payload),
        [
            Line2D([0], [0], color="#00e5ff", lw=1, label="axis 0"),
            Line2D([0], [0], color="#ff4081", lw=1, label="axis 1"),
        ],
    )
    save_overlay(
        image,
        stem_dir / STAGES[2],
        f"{path.name}: axis-sigma usable filter",
        lambda ax: draw_usable(ax, payload),
        [
            Line2D([0], [0], marker="o", color="#2ca25f", lw=0, label="usable"),
            Line2D([0], [0], marker="o", color="#de2d26", lw=0, label="not used"),
        ],
    )
    save_overlay(
        image,
        stem_dir / STAGES[3],
        f"{path.name}: Delaunay edge classification",
        lambda ax: draw_delaunay(ax, payload),
        [Line2D([0], [0], color=c, lw=2, label=k) for k, c in EDGE_COLORS.items()],
    )
    save_overlay(
        image,
        stem_dir / STAGES[4],
        f"{path.name}: triangle composition",
        lambda ax: draw_triangles(ax, payload),
        [Line2D([0], [0], color=c, lw=2, label=k) for k, c in TRI_COLORS.items()],
    )
    save_overlay(image, stem_dir / STAGES[5], f"{path.name}: raw merged quads", lambda ax: draw_quads(ax, payload, "raw"))
    save_overlay(image, stem_dir / STAGES[6], f"{path.name}: topological quad filter", lambda ax: draw_quads(ax, payload, "topology"))
    save_overlay(image, stem_dir / STAGES[7], f"{path.name}: geometry quad filter", lambda ax: draw_quads(ax, payload, "geometry"))
    save_overlay(image, stem_dir / STAGES[8], f"{path.name}: topological walk labels", lambda ax: draw_walk(ax, payload))
    save_overlay(image, stem_dir / STAGES[9], f"{path.name}: final merged detection", lambda ax: draw_final(ax, payload))

    trace = payload.get("trace")
    diagnostics = trace.get("diagnostics") if trace else {}
    detections = payload.get("detections") or []
    return {
        "image": str(path),
        "variant": args.variant_name,
        "final_algorithm": args.final_algorithm,
        "output_dir": str(stem_dir),
        "stages": STAGES,
        "width": int(image.shape[1]),
        "height": int(image.shape[0]),
        "corner_count": len(payload["corners"]),
        "labelled_count": len(detections[0]["target"]["corners"]) if detections else 0,
        "error": payload.get("error"),
        "diagnostics": diagnostics,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image-dir", type=Path, default=Path("testdata/02-topo-grid"))
    parser.add_argument("--out-dir", type=Path, default=Path("docs/img/02-topo-grid"))
    parser.add_argument("--manifest-name", default="manifest.json")
    parser.add_argument("--only", nargs="*", default=None, help="Optional image stems or filenames to render.")
    parser.add_argument("--variant-name", default=None, help="Optional suffix for output image directories.")
    parser.add_argument("--final-algorithm", choices=["topological", "chessboard_v2"], default="topological")
    parser.add_argument("--chess-threshold", type=float, default=100.0)
    parser.add_argument("--pre-blur-sigma", type=float, default=0.0)
    parser.add_argument("--upscale", type=float, default=1.0)
    parser.add_argument("--axis-align-tol-deg", type=float, default=15.0)
    parser.add_argument("--diagonal-angle-tol-deg", type=float, default=15.0)
    parser.add_argument("--max-axis-sigma-deg", type=float, default=math.degrees(0.6))
    parser.add_argument("--edge-ratio-max", type=float, default=10.0)
    parser.add_argument("--min-quads-per-component", type=int, default=1)
    parser.add_argument("--cluster-axis-tol-deg", type=float, default=16.0)
    parser.add_argument("--quad-edge-min-rel", type=float, default=0.0)
    parser.add_argument("--quad-edge-max-rel", type=float, default=1.8)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    wanted = set(args.only or [])
    images = sorted(p for p in args.image_dir.iterdir() if p.suffix.lower() in {".png", ".jpg", ".jpeg"})
    if wanted:
        images = [p for p in images if p.name in wanted or p.stem in wanted]
    if not images:
        raise SystemExit(f"no images found in {args.image_dir}")
    rows = [render_image(path, args.out_dir, args) for path in images]
    args.out_dir.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema": 1,
        "image_dir": str(args.image_dir),
        "out_dir": str(args.out_dir),
        "params": {
            "chess_threshold": args.chess_threshold,
            "pre_blur_sigma": args.pre_blur_sigma,
            "upscale": args.upscale,
            "final_algorithm": args.final_algorithm,
            "axis_align_tol_deg": args.axis_align_tol_deg,
            "diagonal_angle_tol_deg": args.diagonal_angle_tol_deg,
            "max_axis_sigma_deg": args.max_axis_sigma_deg,
            "edge_ratio_max": args.edge_ratio_max,
            "min_quads_per_component": args.min_quads_per_component,
            "cluster_axis_tol_deg": args.cluster_axis_tol_deg,
            "quad_edge_min_rel": args.quad_edge_min_rel,
            "quad_edge_max_rel": args.quad_edge_max_rel,
        },
        "images": rows,
    }
    (args.out_dir / args.manifest_name).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"rendered {len(rows)} image(s) to {args.out_dir}")


if __name__ == "__main__":
    main()
