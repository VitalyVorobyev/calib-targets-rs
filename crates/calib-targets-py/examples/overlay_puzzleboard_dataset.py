"""Phase-3 overlay: PuzzleBoard decode diagnostics.

Reads `PuzzleboardFrameReport` JSON produced by
`crates/calib-targets-puzzleboard/examples/run_dataset.rs` and renders
one overlay PNG per snap.

Successful detections get:
  - labelled corners coloured by the master (row, col) gradient
    (red channel = master_j / 501, green channel = master_i / 501).
    Wrong labels show up instantly as a hue jump inside an otherwise
    smoothly-varying cloud.
  - grid-neighbour edges in the master frame drawn in a thin grey.
  - title line with matched/observed edges, BER, mean confidence, and
    master origin.

Failed detections fall back to Phase-2 style:
  - labelled chessboard corners (from `chessboard_frame.detection`)
    in gold, edges in green/blue; no master colouring.
  - red banner: `FAIL stage={stage} variant={variant}`.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_puzzleboard_dataset.py \\
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
MASTER_ROWS = 501
MASTER_COLS = 501


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


def master_colour(master_i: int, master_j: int) -> tuple[float, float, float]:
    """RGB colour keyed by master (row=j, col=i). Smooth over the 501×501
    master so wrong labels flicker as a sharp hue change."""
    r = (master_j % MASTER_ROWS) / (MASTER_ROWS - 1)
    g = (master_i % MASTER_COLS) / (MASTER_COLS - 1)
    b = 0.3
    return (r, g, b)


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


# --- Draw paths -------------------------------------------------------------

def draw_success(
    ax: plt.Axes,
    frame: dict[str, Any],
    puzzle_result: dict[str, Any],
) -> dict[str, Any]:
    corners = puzzle_result["detection"]["corners"]
    labelled: list[dict[str, Any]] = []
    for c in corners:
        grid = c.get("grid")
        if grid is None:
            continue
        labelled.append({
            "mi": int(grid["i"]),
            "mj": int(grid["j"]),
            "x": float(c["position"][0]),
            "y": float(c["position"][1]),
            "id": c.get("id"),
        })

    # Adjacency by master (i, j) — master coords are unique after decode.
    by_ij = {(c["mi"], c["mj"]): idx for idx, c in enumerate(labelled)}
    edges: list[tuple[int, int]] = []
    adjacency: dict[int, list[int]] = {idx: [] for idx in range(len(labelled))}
    for idx, c in enumerate(labelled):
        for di, dj in ((1, 0), (0, 1)):
            nb = by_ij.get((c["mi"] + di, c["mj"] + dj))
            if nb is not None:
                edges.append((idx, nb))
                adjacency[idx].append(nb)
                adjacency[nb].append(idx)
    for a, b in edges:
        pa, pb = labelled[a], labelled[b]
        ax.plot(
            [pa["x"], pb["x"]],
            [pa["y"], pb["y"]],
            color="white",
            lw=0.4,
            alpha=0.4,
            zorder=2,
        )

    if labelled:
        xs = [c["x"] for c in labelled]
        ys = [c["y"] for c in labelled]
        colors = [master_colour(c["mi"], c["mj"]) for c in labelled]
        ax.scatter(xs, ys, s=18, c=colors, edgecolors="black", linewidths=0.3, zorder=5)

    comps = _connected_components(list(range(len(labelled))), adjacency)
    comp_sizes = sorted((len(c) for c in comps), reverse=True)

    return {
        "labelled": len(labelled),
        "components": len(comps),
        "component_sizes": comp_sizes,
    }


def draw_failure(
    ax: plt.Axes,
    frame: dict[str, Any],
) -> dict[str, Any]:
    """Fallback: Phase-2 style grid overlay from the chessboard DebugFrame."""
    detection = frame["chessboard_frame"].get("detection")
    labelled: list[dict[str, Any]] = []
    if detection is not None:
        for c in detection["target"]["corners"]:
            grid = c.get("grid")
            if grid is None:
                continue
            labelled.append({
                "i": int(grid["i"]),
                "j": int(grid["j"]),
                "x": float(c["position"][0]),
                "y": float(c["position"][1]),
            })

    by_ij = {(c["i"], c["j"]): idx for idx, c in enumerate(labelled)}
    edges_ax0: list[tuple[int, int]] = []
    edges_ax1: list[tuple[int, int]] = []
    for idx, c in enumerate(labelled):
        nb_i = by_ij.get((c["i"] + 1, c["j"]))
        if nb_i is not None:
            edges_ax0.append((idx, nb_i))
        nb_j = by_ij.get((c["i"], c["j"] + 1))
        if nb_j is not None:
            edges_ax1.append((idx, nb_j))
    for a, b in edges_ax0:
        pa, pb = labelled[a], labelled[b]
        ax.plot([pa["x"], pb["x"]], [pa["y"], pb["y"]], color="limegreen", lw=0.7, alpha=0.7, zorder=2)
    for a, b in edges_ax1:
        pa, pb = labelled[a], labelled[b]
        ax.plot([pa["x"], pb["x"]], [pa["y"], pb["y"]], color="deepskyblue", lw=0.7, alpha=0.7, zorder=2)
    if labelled:
        xs = [c["x"] for c in labelled]
        ys = [c["y"] for c in labelled]
        ax.scatter(xs, ys, s=14, c="gold", edgecolors="black", linewidths=0.3, zorder=5)
    return {"labelled": len(labelled)}


def draw_overlay(
    image: np.ndarray,
    frame: dict[str, Any],
    title: str,
    out_path: Path,
) -> dict[str, Any]:
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)

    outcome = frame["outcome"]
    width = frame["width"]
    height = frame["height"]
    upscale = frame["upscale"]
    kind = outcome.get("kind")

    if kind == "ok":
        # Outcome body nested under `content` or flattened depending on serde:
        # our wrapper serializes Ok(Box<PuzzleBoardDetectionResult>) as
        # `{ "kind": "ok", <PuzzleBoardDetectionResult fields...> }` because
        # we used `#[serde(tag = "kind")]` on a tuple-struct variant — the
        # wrapped object fields are hoisted into the enum body. Handle both
        # shapes just in case.
        result = outcome if "detection" in outcome else outcome.get("content", {})
        stats = draw_success(ax, frame, result)
        decode = result.get("decode", {})
        matched = decode.get("edges_matched", 0)
        observed = decode.get("edges_observed", 0)
        ber = decode.get("bit_error_rate", float("nan"))
        conf = decode.get("mean_confidence", float("nan"))
        origin_row = decode.get("master_origin_row", 0)
        origin_col = decode.get("master_origin_col", 0)
        cfg_idx = frame.get("best_config_index")
        ax.set_title(
            f"{title}\n"
            f"labelled={stats['labelled']}  components={stats['components']}  "
            f"matched={matched}/{observed}  BER={ber:.3f}  conf={conf:.2f}\n"
            f"master_origin=({origin_row},{origin_col})  "
            f"best_cfg_idx={cfg_idx}  upscale={upscale}",
            fontsize=9,
        )
        status = "ok"
        handles = [
            Line2D([0], [0], marker="o", color=(0.9, 0.1, 0.3), lw=0, markersize=6,
                   label="master (row≈hi, col≈lo)"),
            Line2D([0], [0], marker="o", color=(0.1, 0.9, 0.3), lw=0, markersize=6,
                   label="master (row≈lo, col≈hi)"),
        ]
    else:
        stats = draw_failure(ax, frame)
        stage = outcome.get("stage", "?")
        variant = outcome.get("variant", "?")
        message = outcome.get("message", "")
        ax.set_title(
            f"{title}  [FAIL]\n"
            f"stage={stage}  variant={variant}\n"
            f"{message}   "
            f"chess_labelled={stats['labelled']}  upscale={upscale}",
            fontsize=9,
            color="red",
        )
        ax.text(
            0.5,
            0.98,
            f"FAIL: {stage}/{variant}",
            transform=ax.transAxes,
            fontsize=11,
            color="white",
            ha="center",
            va="top",
            bbox=dict(boxstyle="round,pad=0.4", fc="red", ec="black", alpha=0.85),
            zorder=10,
        )
        status = f"fail:{stage}/{variant}"
        handles = [
            Line2D([0], [0], color="limegreen", lw=2, label="chess edge (Δi=1)"),
            Line2D([0], [0], color="deepskyblue", lw=2, label="chess edge (Δj=1)"),
            Line2D([0], [0], marker="o", color="gold", lw=0, markersize=6, label="chess corner"),
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

    stats["status"] = status
    return stats


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--dataset", required=True, type=Path,
                        help="directory containing target_*.png files")
    parser.add_argument("--frames", required=True, type=Path,
                        help="directory of per-snap PuzzleboardFrameReport JSONs")
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
    n_ok = 0
    failure_counts: dict[str, int] = {}
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
        stats = draw_overlay(up, frame, f"t{target_idx}s{snap_idx}", out_path)
        n_frames += 1
        if stats["status"] == "ok":
            n_ok += 1
        else:
            failure_counts[stats["status"]] = failure_counts.get(stats["status"], 0) + 1
        rows.append(
            f"t{target_idx}s{snap_idx}\tstatus={stats['status']}\t"
            f"labelled={stats.get('labelled', 0)}\t"
            f"components={stats.get('components', 0)}"
        )

    rate = 100.0 * n_ok / n_frames if n_frames else 0.0
    print(f"wrote {n_frames} overlays to {args.out}  (detected={n_ok}  rate={rate:.1f}%)")
    if failure_counts:
        print("failures:")
        for key, cnt in sorted(failure_counts.items()):
            print(f"  {key}: {cnt}")
    summary_path = args.out / "_puzzleboard_summary.tsv"
    summary_path.write_text("\n".join(rows) + "\n")
    print(f"per-frame stats: {summary_path}")


if __name__ == "__main__":
    main()
