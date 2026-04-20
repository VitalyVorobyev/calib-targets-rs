"""Phase-2 overlay: chessboard-grid labels + edges from CompactFrame JSON.

Consumes the per-snap JSON emitted by
`crates/calib-targets-chessboard/examples/run_dataset.rs` and renders
the labelled (i, j) corners plus their grid-neighbor edges on the
upscaled snap. Edge crossings are a planarity defect and are
highlighted in magenta. Multi-component detections are noted in the
title — for PuzzleBoard this is legitimate (the decoder fuses
components), but we want the signal visible.

Unlike `overlay_chessboard_dataset.py`, this overlay does NOT re-run
detection; it consumes the already-emitted JSON so bench numbers and
overlays stay consistent.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_chessboard_grid.py \\
        --dataset <dir-of-target-pngs> \\
        --frames  <run-dataset-json-dir> \\
        --out     <overlay-output-dir>
"""

from __future__ import annotations

import argparse
import json
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


# --- IO ---------------------------------------------------------------------

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


def extract_snap(full: np.ndarray, snap_idx: int) -> np.ndarray:
    x0 = snap_idx * NATIVE_SNAP_WIDTH
    return full[:NATIVE_SNAP_HEIGHT, x0 : x0 + NATIVE_SNAP_WIDTH]


def upscale_snap(snap: np.ndarray, factor: int) -> np.ndarray:
    if factor == 1:
        return snap
    h, w = snap.shape
    pil = Image.fromarray(snap, mode="L").resize(
        (w * factor, h * factor), resample=Image.BILINEAR
    )
    return np.asarray(pil, dtype=np.uint8)


# --- Planarity check (ported from overlay_chessboard_dataset.py) -----------

def _seg_cross_product(ax_: float, ay_: float, bx_: float, by_: float) -> float:
    return ax_ * by_ - ay_ * bx_


def _on_segment(px: float, py: float, ax_: float, ay_: float, bx_: float, by_: float,
                tol: float = 1e-9) -> bool:
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
    """True iff segments p1-p2 and p3-p4 cross in their interior.

    Shared endpoints (within endpoint_tol) are legal meeting points of
    a planar graph and do NOT count as a crossing. Collinear overlap
    is treated as a crossing (it is a real defect).
    """
    def same_pt(a, b):
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
    out: list[tuple[int, int]] = []
    for i in range(len(edges)):
        a1, a2 = edges[i]
        for j in range(i + 1, len(edges)):
            b1, b2 = edges[j]
            if segments_cross(a1, a2, b1, b2):
                out.append((i, j))
    return out


def _connected_components(
    nodes: list[int], adjacency: dict[int, list[int]]
) -> list[list[int]]:
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


# --- Overlay ----------------------------------------------------------------

def draw_grid_overlay(
    image: np.ndarray,
    frame: dict[str, Any],
    title: str,
    out_path: Path,
) -> dict[str, Any]:
    """Draw labelled corners + edges. Returns per-frame stats."""
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)

    detection = frame["frame"].get("detection")
    labelled: list[dict[str, Any]] = []
    if detection is not None:
        for lc in detection["target"]["corners"]:
            grid = lc.get("grid")
            if grid is None:
                continue
            labelled.append({
                "i": int(grid["i"]),
                "j": int(grid["j"]),
                "x": float(lc["position"][0]),
                "y": float(lc["position"][1]),
            })

    by_ij: dict[tuple[int, int], int] = {(c["i"], c["j"]): idx for idx, c in enumerate(labelled)}
    edges_ax0: list[tuple[int, int]] = []  # Δi=1
    edges_ax1: list[tuple[int, int]] = []  # Δj=1
    adjacency: dict[int, list[int]] = {idx: [] for idx in range(len(labelled))}
    for idx, c in enumerate(labelled):
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
    for a, b in edges_ax0:
        pa, pb = labelled[a], labelled[b]
        edge_segments.append(((pa["x"], pa["y"]), (pb["x"], pb["y"])))
    for a, b in edges_ax1:
        pa, pb = labelled[a], labelled[b]
        edge_segments.append(((pa["x"], pa["y"]), (pb["x"], pb["y"])))
    n_ax0 = len(edges_ax0)

    crossings = count_edge_crossings(edge_segments)
    crossing_edge_idx: set[int] = set()
    for i, j in crossings:
        crossing_edge_idx.add(i)
        crossing_edge_idx.add(j)

    comps = _connected_components(list(range(len(labelled))), adjacency)
    comps.sort(key=len, reverse=True)
    comp_sizes = [len(c) for c in comps]

    for edge_idx, (p1, p2) in enumerate(edge_segments):
        if edge_idx in crossing_edge_idx:
            color, lw, alpha, zorder = "magenta", 1.4, 0.9, 3
        else:
            color = "limegreen" if edge_idx < n_ax0 else "deepskyblue"
            lw, alpha, zorder = 0.9, 0.8, 2
        ax.plot([p1[0], p2[0]], [p1[1], p2[1]], color=color, lw=lw, alpha=alpha, zorder=zorder)

    if labelled:
        xs = [c["x"] for c in labelled]
        ys = [c["y"] for c in labelled]
        ax.scatter(xs, ys, s=18, c="gold", edgecolors="black", linewidths=0.4, zorder=5)

    width = frame["width"]
    height = frame["height"]
    upscale = frame["upscale"]
    comp_str = "/".join(str(s) for s in comp_sizes[:5]) if comp_sizes else "0"
    title_color = "red" if crossings else "black"
    ax.set_title(
        f"{title}\n"
        f"labelled={len(labelled)}  edges={len(edge_segments)}  "
        f"components={len(comps)} (sizes={comp_str})  "
        f"crossings={len(crossings)}  upscale={upscale}",
        fontsize=9,
        color=title_color,
    )
    handles = [
        Line2D([0], [0], color="limegreen", lw=2, label="edge (Δi=1)"),
        Line2D([0], [0], color="deepskyblue", lw=2, label="edge (Δj=1)"),
        Line2D([0], [0], color="magenta", lw=2, label="CROSSING edge (defect)"),
        Line2D([0], [0], marker="o", color="gold", lw=0, markersize=6, label="labelled (i,j)"),
    ]
    ax.legend(handles=handles, loc="lower right", fontsize=7, framealpha=0.9)
    ax.set_xlim(0, width)
    ax.set_ylim(height, 0)
    ax.set_aspect("equal")
    ax.axis("off")
    fig.tight_layout()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)

    return {
        "labelled": len(labelled),
        "edges": len(edge_segments),
        "components": len(comps),
        "component_sizes": comp_sizes,
        "crossings": len(crossings),
        "largest_component": comp_sizes[0] if comp_sizes else 0,
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

    frame_paths = sorted(
        (p for p in args.frames.iterdir()
         if p.suffix == ".json" and parse_frame_name(p.name) is not None),
        key=lambda p: parse_frame_name(p.name) or (0, 0),
    )
    if not frame_paths:
        raise SystemExit(f"no t{{T}}s{{S}}.json files in {args.frames}")

    args.out.mkdir(parents=True, exist_ok=True)

    image_cache: dict[int, np.ndarray] = {}
    rows: list[str] = []
    n_frames = 0
    n_detected = 0
    n_multi_component = 0
    n_with_crossings = 0
    total_crossings = 0

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
        stats = draw_grid_overlay(up, frame, f"t{target_idx}s{snap_idx}", out_path)

        n_frames += 1
        if stats["labelled"] > 0:
            n_detected += 1
        if stats["components"] > 1:
            n_multi_component += 1
        if stats["crossings"] > 0:
            n_with_crossings += 1
            total_crossings += stats["crossings"]

        rows.append(
            f"t{target_idx}s{snap_idx}\tlabelled={stats['labelled']}\t"
            f"edges={stats['edges']}\tcomp={stats['components']}\t"
            f"sizes={','.join(str(s) for s in stats['component_sizes']) or '-'}\t"
            f"largest={stats['largest_component']}\tcrossings={stats['crossings']}"
        )

    pct = 100.0 * n_detected / n_frames if n_frames else 0.0
    print(
        f"wrote {n_frames} overlays to {args.out}"
        f"  (detected={n_detected}  rate={pct:.1f}%)"
    )
    print(
        f"multi-component: {n_multi_component}/{n_frames}"
        f"   crossings: {n_with_crossings}/{n_frames} frames "
        f"(total defective-edge pairs: {total_crossings})"
    )
    summary_path = args.out / "_grid_summary.tsv"
    summary_path.write_text("\n".join(rows) + "\n")
    print(f"per-frame stats: {summary_path}")


if __name__ == "__main__":
    main()
