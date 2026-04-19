"""Render overlays from chessboard-v2's per-snap DebugFrame JSON.

Input: a directory of `t{T}s{S}.json` files produced by
`cargo run -p chessboard-v2 --example run_dataset`. Each file wraps a
compact debug frame (labelled corners, cluster centers, blacklist,
etc.).

Output: one PNG overlay per snap at `<out>/t{T}s{S}_v2.png`.

The overlay renders:
- labelled corners in gold with their (i, j) annotation,
- grid-neighbor edges (|Δi|+|Δj| = 1) in blue/green by axis,
- blacklisted-at-labelled stage corners as red X with reason,
- all input corners as faint grey dots for context,
- cluster centers drawn as tangent lines in cyan and magenta.

Usage:
    uv run python crates/calib-targets-py/examples/overlay_chessboard_v2.py \\
        --dataset testdata/3536119669 \\
        --frames bench_results/chessboard_v2_overlays \\
        --out bench_results/chessboard_v2_overlays/png
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image


SNAP_WIDTH = 720
SNAP_HEIGHT = 540
SNAPS_PER_IMAGE = 6

# Keep in sync with `chessboard_v2::DEBUG_FRAME_SCHEMA`.
EXPECTED_DEBUG_FRAME_SCHEMA = 1
_warned_schemas: set[int] = set()


def extract_snap(image: np.ndarray, snap_idx: int) -> np.ndarray:
    x0 = snap_idx * SNAP_WIDTH
    return image[:SNAP_HEIGHT, x0 : x0 + SNAP_WIDTH]


def _check_schema(dbg: dict, tag: str) -> None:
    """Warn once per observed schema version when it differs from EXPECTED_DEBUG_FRAME_SCHEMA."""
    schema = dbg.get("schema")
    if schema == EXPECTED_DEBUG_FRAME_SCHEMA:
        return
    if schema in _warned_schemas:
        return
    _warned_schemas.add(schema)
    if schema is None:
        print(
            f"[warn] {tag}: DebugFrame missing 'schema' field "
            f"(expected v{EXPECTED_DEBUG_FRAME_SCHEMA}). Overlay may be inaccurate."
        )
    else:
        print(
            f"[warn] {tag}: DebugFrame schema v{schema} "
            f"(expected v{EXPECTED_DEBUG_FRAME_SCHEMA}). Overlay may be inaccurate."
        )


def render_overlay(
    snap: np.ndarray,
    frame: dict,
    tag: str,
    out_path: Path,
) -> dict:
    fig, ax = plt.subplots(figsize=(12, 9), dpi=110)
    ax.imshow(snap, cmap="gray", vmin=0, vmax=255)

    input_corners = frame["input_corners"]
    dbg = frame["frame"]
    _check_schema(dbg, tag)
    corners = dbg["corners"]
    detection = dbg.get("detection")

    # All input corners as faint grey dots.
    xs_all = [c["x"] for c in input_corners]
    ys_all = [c["y"] for c in input_corners]
    ax.scatter(xs_all, ys_all, s=6, c="#444444", alpha=0.45, zorder=1)

    # Labelled corners with (i, j).
    labelled: dict[tuple[int, int], tuple[float, float, int]] = {}
    blacklisted: list[tuple[float, float, tuple[int, int], str]] = []
    for ca in corners:
        stage = ca["stage"]
        if isinstance(stage, dict) and "Labeled" in stage:
            at = stage["Labeled"]["at"]
            labelled[(at[0], at[1])] = (ca["position"][0], ca["position"][1], ca["input_index"])
        elif isinstance(stage, dict) and "LabeledThenBlacklisted" in stage:
            b = stage["LabeledThenBlacklisted"]
            blacklisted.append(
                (
                    ca["position"][0],
                    ca["position"][1],
                    tuple(b["at"]),
                    b["reason"],
                )
            )

    # Grid edges: labelled[(i, j)] ↔ labelled[(i+1, j)] or (i, j+1).
    edges_ax0 = []
    edges_ax1 = []
    for (i, j), (x, y, _) in labelled.items():
        if (i + 1, j) in labelled:
            xn, yn, _ = labelled[(i + 1, j)]
            edges_ax0.append(((x, y), (xn, yn)))
        if (i, j + 1) in labelled:
            xn, yn, _ = labelled[(i, j + 1)]
            edges_ax1.append(((x, y), (xn, yn)))

    for (p0, p1) in edges_ax0:
        ax.plot([p0[0], p1[0]], [p0[1], p1[1]], color="limegreen", lw=1.0, alpha=0.9, zorder=2)
    for (p0, p1) in edges_ax1:
        ax.plot([p0[0], p1[0]], [p0[1], p1[1]], color="deepskyblue", lw=1.0, alpha=0.9, zorder=2)

    # Labelled dots + (i, j) annotations.
    if labelled:
        xs = [p[0] for p in labelled.values()]
        ys = [p[1] for p in labelled.values()]
        ax.scatter(xs, ys, s=22, c="gold", edgecolors="black", linewidths=0.5, zorder=5)
        for (i, j), (x, y, _) in labelled.items():
            ax.text(
                x + 3,
                y - 3,
                f"{i},{j}",
                fontsize=5,
                color="white",
                bbox=dict(boxstyle="square,pad=0.1", fc="black", ec="none", alpha=0.55),
                zorder=6,
            )

    # Blacklisted corners.
    for (x, y, at, reason) in blacklisted:
        ax.scatter([x], [y], s=60, c="red", marker="x", linewidths=2.0, zorder=6)
        ax.text(
            x + 5,
            y + 5,
            f"BL {at}",
            fontsize=5,
            color="red",
            zorder=7,
        )

    # Cluster direction lines in the image center.
    gd = dbg.get("grid_directions")
    if gd is not None:
        cx, cy = SNAP_WIDTH / 2.0, SNAP_HEIGHT / 2.0
        span = 0.4 * min(SNAP_WIDTH, SNAP_HEIGHT)
        for theta, color in zip(gd, ["cyan", "magenta"]):
            dx = math.cos(theta) * span
            dy = math.sin(theta) * span
            ax.plot(
                [cx - dx, cx + dx],
                [cy - dy, cy + dy],
                color=color,
                lw=1.5,
                alpha=0.5,
                zorder=0,
            )

    # Title with metrics.
    labelled_count = len(labelled)
    cell_size = dbg.get("cell_size")
    iters = dbg.get("iterations", [])
    iter_txt = " ".join(
        f"{it['iter']}→{it['labelled_count']}(+{len(it['new_blacklist'])}bl)"
        for it in iters
    )
    det_ok = detection is not None
    ax.set_title(
        f"{tag}  det={det_ok}  labelled={labelled_count}  "
        f"blacklisted={len(blacklisted)}  s={cell_size:.2f}"
        if cell_size is not None
        else f"{tag}  det={det_ok}  labelled={labelled_count}\n"
        f"iters: {iter_txt}",
        fontsize=9,
    )

    handles = [
        Line2D([0], [0], color="limegreen", lw=2, label="grid edge (Δi=1)"),
        Line2D([0], [0], color="deepskyblue", lw=2, label="grid edge (Δj=1)"),
        Line2D([0], [0], color="cyan", lw=2, label="cluster Θ₀"),
        Line2D([0], [0], color="magenta", lw=2, label="cluster Θ₁"),
        Line2D([0], [0], marker="o", color="gold", lw=0, markersize=6, label="labelled (i,j)"),
        Line2D([0], [0], marker="x", color="red", lw=0, markersize=8, label="blacklisted"),
        Line2D([0], [0], marker="o", color="#444444", lw=0, markersize=4, label="input corner"),
    ]
    ax.legend(handles=handles, loc="lower right", fontsize=7, framealpha=0.9)
    ax.set_xlim(0, SNAP_WIDTH)
    ax.set_ylim(SNAP_HEIGHT, 0)
    ax.set_aspect("equal")
    ax.axis("off")
    fig.tight_layout()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight")
    plt.close(fig)

    return {
        "labelled": labelled_count,
        "blacklisted": len(blacklisted),
        "detection": det_ok,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", required=True, type=Path,
                        help="directory of target_*.png files")
    parser.add_argument("--frames", required=True, type=Path,
                        help="directory of per-snap DebugFrame JSON files")
    parser.add_argument("--out", required=True, type=Path,
                        help="output directory for PNG overlays")
    parser.add_argument("--tag", default="v2",
                        help="suffix tag appended to every output filename")
    args = parser.parse_args()

    # Enumerate target PNGs.
    targets: dict[int, Path] = {}
    for p in args.dataset.iterdir():
        if not p.is_file() or p.suffix.lower() != ".png":
            continue
        stem = p.stem
        if not stem.startswith("target_") or " " in stem:
            continue
        try:
            idx = int(stem[len("target_"):])
        except ValueError:
            continue
        targets[idx] = p

    n_total = 0
    n_detected = 0
    sum_labelled = 0
    sum_blacklisted = 0
    per_frame_rows: list[str] = []

    for idx in sorted(targets):
        img = np.asarray(Image.open(targets[idx]).convert("L"), dtype=np.uint8)
        for snap_idx in range(SNAPS_PER_IMAGE):
            json_path = args.frames / f"t{idx}s{snap_idx}.json"
            if not json_path.exists():
                continue
            with open(json_path) as f:
                frame = json.load(f)
            snap = extract_snap(img, snap_idx)
            out_path = args.out / f"t{idx}s{snap_idx}_{args.tag}.png"
            stats = render_overlay(snap, frame, f"t{idx}s{snap_idx} {args.tag}", out_path)
            n_total += 1
            if stats["detection"]:
                n_detected += 1
                sum_labelled += stats["labelled"]
            sum_blacklisted += stats["blacklisted"]
            per_frame_rows.append(
                f"t{idx}s{snap_idx}\tdet={stats['detection']}\tlabelled={stats['labelled']}\tblacklisted={stats['blacklisted']}"
            )

    pct = (100.0 * n_detected / n_total) if n_total else 0.0
    avg = (sum_labelled / n_detected) if n_detected else 0.0
    print(
        f"wrote {n_total} overlays to {args.out}  "
        f"(detected={n_detected} / rate={pct:.1f}%, avg_labelled={avg:.1f}, "
        f"total_blacklisted={sum_blacklisted})"
    )
    summary = args.out / f"_{args.tag}_summary.tsv"
    summary.write_text("\n".join(per_frame_rows) + "\n")
    print(f"per-frame stats: {summary}")


if __name__ == "__main__":
    main()
