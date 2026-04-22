"""Interactive single-snap PuzzleBoard debug overlay.

Loads one snap from a `130x130_puzzle`-style dataset target PNG (6 × 720×540
snaps stacked horizontally), pairs it with its matching `run_dataset.rs`
JSON report, and opens an interactive matplotlib figure showing:

- the (upscaled) snap image
- every detected chessboard corner (black dots with master (i,j) labels)
- every observed edge-bit as a coloured circle at the edge midpoint:

    * **blue** circle for bit = 0 (expected dark dot)
    * **red** circle for bit = 1 (expected bright dot)
    * radius ∝ confidence (bigger = more confident)
    * faint grey circle for edges dropped by `min_bit_confidence`

Hover over any corner or edge to see its metadata in the toolbar. The
matplotlib pan/zoom buttons let you drill into noisy regions.

Usage:

    uv run python crates/calib-targets-py/examples/inspect_puzzleboard_snap.py \\
        --target privatedata/130x130_puzzle/target_3.png \\
        --snap 0 \\
        --json tmpdata/softll_130/t3s0.json \\
        [--upscale 2] [--min-conf 0.15]

The JSON is optional: if omitted the script runs detection itself using
the Rust/PyO3 bindings with default parameters for the declared board
geometry (`--rows`, `--cols`, `--cell-size-mm`).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Optional

import matplotlib.pyplot as plt
import numpy as np
from matplotlib.patches import Circle
from PIL import Image

SNAP_WIDTH = 720
SNAP_HEIGHT = 540
SNAPS_PER_IMAGE = 6
MASTER_SIZE = 501

# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(
        description="Interactive PuzzleBoard single-snap detection overlay.",
    )
    ap.add_argument(
        "--target", required=True, type=Path,
        help="Path to a target_*.png file (4320×540, 6 stacked snaps).",
    )
    ap.add_argument(
        "--snap", required=True, type=int, choices=range(SNAPS_PER_IMAGE),
        help="Snap index (0..5).",
    )
    ap.add_argument(
        "--json", type=Path,
        help=(
            "Path to t{T}s{S}.json from run_dataset.rs. If omitted, the "
            "script runs detection itself using the Rust/PyO3 bindings."
        ),
    )
    ap.add_argument(
        "--upscale", type=int, default=2,
        help="Upscaling factor applied before detection (default 2).",
    )
    ap.add_argument(
        "--min-conf", type=float, default=0.15,
        help=(
            "Confidence threshold used to highlight dropped edges. Must "
            "match the detector's `min_bit_confidence` to be meaningful."
        ),
    )
    ap.add_argument(
        "--rows", type=int, default=130,
        help="Puzzleboard rows (used only when --json is omitted).",
    )
    ap.add_argument(
        "--cols", type=int, default=130,
        help="Puzzleboard cols (used only when --json is omitted).",
    )
    ap.add_argument(
        "--cell-size-mm", type=float, default=1.014,
        help="Puzzleboard cell size (used only when --json is omitted).",
    )
    ap.add_argument(
        "--save", type=Path,
        help="Optional path to save the rendered PNG.",
    )
    return ap.parse_args()


# ---------------------------------------------------------------------------
# Image + detection loading
# ---------------------------------------------------------------------------


def load_snap(target_png: Path, snap_idx: int, upscale: int) -> np.ndarray:
    img = Image.open(target_png).convert("L")
    arr = np.asarray(img)
    if arr.shape[1] < (snap_idx + 1) * SNAP_WIDTH or arr.shape[0] < SNAP_HEIGHT:
        raise SystemExit(
            f"target PNG {target_png} is {arr.shape[1]}×{arr.shape[0]} — "
            f"expected at least {(snap_idx + 1) * SNAP_WIDTH}×{SNAP_HEIGHT}"
        )
    native = arr[0:SNAP_HEIGHT, snap_idx * SNAP_WIDTH : (snap_idx + 1) * SNAP_WIDTH]
    if upscale == 1:
        return native.astype(np.uint8)
    big = Image.fromarray(native).resize(
        (SNAP_WIDTH * upscale, SNAP_HEIGHT * upscale),
        resample=Image.BILINEAR,
    )
    return np.asarray(big).astype(np.uint8)


def detection_from_json(json_path: Path) -> dict[str, Any]:
    with json_path.open("r") as f:
        report = json.load(f)
    out = report.get("outcome", {})
    if out.get("kind") != "ok":
        raise SystemExit(
            f"{json_path}: detection did not succeed "
            f"(variant={out.get('variant')}, message={out.get('message')!r})"
        )
    return out


def detection_from_live_run(
    image: np.ndarray, rows: int, cols: int, cell_size_mm: float
) -> dict[str, Any]:
    """Fallback path: run detection via Python bindings."""
    try:
        from calib_targets import _core  # type: ignore
        from calib_targets import (
            PuzzleBoardParams,
            PuzzleBoardSpec,
        )
    except ImportError as e:
        raise SystemExit(
            "Cannot import calib_targets — build with `uv run maturin develop --release "
            "-m crates/calib-targets-py/Cargo.toml`."
        ) from e

    spec = PuzzleBoardSpec(
        rows=rows,
        cols=cols,
        cell_size=cell_size_mm,
        origin_row=0,
        origin_col=0,
    )
    params = PuzzleBoardParams.for_board(spec)
    raw = _core.detect_puzzleboard(
        image,
        chess_cfg=None,
        params=params.to_dict(),
    )
    # The raw dict is exactly PuzzleBoardDetectionResult as serialised by serde.
    return {
        "kind": "ok",
        "detection": raw["detection"],
        "alignment": raw["alignment"],
        "decode": raw["decode"],
        "observed_edges": raw["observed_edges"],
    }


# ---------------------------------------------------------------------------
# Local-grid → pixel-midpoint reconstruction
# ---------------------------------------------------------------------------


def build_master_to_pixel(detection: dict[str, Any]) -> dict[tuple[int, int], tuple[float, float]]:
    """Map master `(i, j)` → pixel `(x, y)` for every labelled corner."""
    m2p: dict[tuple[int, int], tuple[float, float]] = {}
    for c in detection.get("corners", []):
        grid = c.get("grid")
        pos = c.get("position")
        if grid is None or pos is None:
            continue
        m2p[(int(grid["i"]), int(grid["j"]))] = (float(pos[0]), float(pos[1]))
    return m2p


def local_to_master(
    local_i: int, local_j: int, alignment: dict[str, Any]
) -> tuple[int, int]:
    """Apply the alignment (D4 + translation), wrap into [0, 501)."""
    t = alignment["transform"]
    tx, ty = alignment["translation"]
    a, b = int(t["a"]), int(t["b"])
    c, d = int(t["c"]), int(t["d"])
    raw_i = a * local_i + b * local_j + int(tx)
    raw_j = c * local_i + d * local_j + int(ty)
    return (raw_i % MASTER_SIZE, raw_j % MASTER_SIZE)


def edge_endpoints(edge: dict[str, Any]) -> tuple[tuple[int, int], tuple[int, int]]:
    """Return the two chess-local `(col, row)` corner coordinates an edge spans."""
    local_row = int(edge["row"])
    local_col = int(edge["col"])
    orient_kind = edge["orientation"]["kind"] if isinstance(edge["orientation"], dict) else edge["orientation"]
    orient_kind = orient_kind.lower()
    a = (local_col, local_row)
    if orient_kind == "horizontal":
        b = (local_col + 1, local_row)
    elif orient_kind == "vertical":
        b = (local_col, local_row + 1)
    else:
        raise SystemExit(f"unknown edge orientation: {edge['orientation']!r}")
    return a, b


# ---------------------------------------------------------------------------
# Plot
# ---------------------------------------------------------------------------


def render_overlay(
    ax: plt.Axes,
    image: np.ndarray,
    detection: dict[str, Any],
    alignment: dict[str, Any],
    decode: dict[str, Any],
    observed_edges: list[dict[str, Any]],
    min_conf: float,
) -> None:
    ax.imshow(image, cmap="gray", origin="upper", interpolation="nearest")

    corners = detection.get("corners", [])
    if corners:
        xs = [float(c["position"][0]) for c in corners]
        ys = [float(c["position"][1]) for c in corners]
        ax.scatter(xs, ys, s=8, marker=".", color="#00cc66", alpha=0.9, label="labelled corner")

    m2p = build_master_to_pixel(detection)

    edges_drawn = 0
    edges_missing = 0
    radius_scale = 6.0
    for edge in observed_edges:
        (ca, cb) = edge_endpoints(edge)
        ma = local_to_master(*ca, alignment)
        mb = local_to_master(*cb, alignment)
        pa = m2p.get(ma)
        pb = m2p.get(mb)
        if pa is None or pb is None:
            edges_missing += 1
            continue
        mid = ((pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5)
        conf = float(edge["confidence"])
        bit = int(edge["bit"])
        color = "#e63946" if bit == 1 else "#1d4ed8"
        face = color if conf >= min_conf else "#bbbbbb"
        alpha = 0.85 if conf >= min_conf else 0.4
        radius = max(1.5, radius_scale * conf)
        circ = Circle(
            mid,
            radius=radius,
            facecolor=face,
            edgecolor="black",
            linewidth=0.4,
            alpha=alpha,
        )
        ax.add_patch(circ)
        edges_drawn += 1

    title_bits = [
        f"n_corners={len(corners)}",
        f"n_edges={len(observed_edges)} (plotted {edges_drawn}, missing-endpoint {edges_missing})",
        f"matched={decode.get('edges_matched')}/{decode.get('edges_observed')}",
        f"BER={decode.get('bit_error_rate'):.3f}",
        f"conf={decode.get('mean_confidence'):.3f}",
    ]
    scoring_mode = decode.get("scoring_mode")
    if isinstance(scoring_mode, dict):
        scoring_mode = scoring_mode.get("kind")
    if scoring_mode:
        title_bits.append(f"mode={scoring_mode}")
    if decode.get("score_margin") is not None:
        title_bits.append(f"margin={decode['score_margin']:.3f}")
    ax.set_title("  ".join(title_bits), fontsize=10)

    legend_handles = [
        plt.Line2D([0], [0], marker="o", color="w", markerfacecolor="#1d4ed8",
                   markersize=10, markeredgecolor="black", label="bit=0 (dark)"),
        plt.Line2D([0], [0], marker="o", color="w", markerfacecolor="#e63946",
                   markersize=10, markeredgecolor="black", label="bit=1 (bright)"),
        plt.Line2D([0], [0], marker="o", color="w", markerfacecolor="#bbbbbb",
                   markersize=8, markeredgecolor="black",
                   label=f"conf<{min_conf} (dropped)"),
        plt.Line2D([0], [0], marker=".", color="#00cc66", markersize=10,
                   linestyle="", label="labelled corner"),
    ]
    ax.legend(handles=legend_handles, loc="upper right", fontsize=8, framealpha=0.9)
    ax.set_xlabel("pixel x")
    ax.set_ylabel("pixel y")
    ax.set_aspect("equal")


# ---------------------------------------------------------------------------
# Hover / picker
# ---------------------------------------------------------------------------


class HoverAnnotator:
    """Attach per-artist metadata so matplotlib's `motion_notify_event` can
    show edge / corner details near the mouse cursor."""

    def __init__(self, fig: plt.Figure, ax: plt.Axes):
        self.fig = fig
        self.ax = ax
        self.annot = ax.annotate(
            "",
            xy=(0, 0),
            xytext=(12, 12),
            textcoords="offset points",
            fontsize=8,
            bbox=dict(boxstyle="round", facecolor="#fffbe6", alpha=0.95),
            visible=False,
        )
        self._artists: list[tuple[Any, str]] = []
        fig.canvas.mpl_connect("motion_notify_event", self._on_move)

    def register(self, artist: Any, text: str) -> None:
        self._artists.append((artist, text))

    def _on_move(self, event: Any) -> None:
        if event.inaxes is not self.ax:
            if self.annot.get_visible():
                self.annot.set_visible(False)
                self.fig.canvas.draw_idle()
            return
        for artist, text in self._artists:
            contains, _ = artist.contains(event)
            if contains:
                self.annot.xy = (event.xdata, event.ydata)
                self.annot.set_text(text)
                self.annot.set_visible(True)
                self.fig.canvas.draw_idle()
                return
        if self.annot.get_visible():
            self.annot.set_visible(False)
            self.fig.canvas.draw_idle()


def attach_hover(
    fig: plt.Figure,
    ax: plt.Axes,
    detection: dict[str, Any],
    alignment: dict[str, Any],
    observed_edges: list[dict[str, Any]],
    min_conf: float,
) -> HoverAnnotator:
    hover = HoverAnnotator(fig, ax)
    m2p = build_master_to_pixel(detection)

    for c in detection.get("corners", []):
        pos = c["position"]
        grid = c.get("grid") or {}
        tp = c.get("target_position") or [float("nan"), float("nan")]
        (circ,) = ax.plot(
            pos[0],
            pos[1],
            ".",
            color="#00cc66",
            markersize=6,
            picker=5,
            alpha=0.0,  # invisible; the scatter already drew it
        )
        txt = (
            f"corner master=({grid.get('i')},{grid.get('j')})\n"
            f"px=({pos[0]:.1f},{pos[1]:.1f})\n"
            f"target=({tp[0]:.2f},{tp[1]:.2f}) mm\n"
            f"id={c.get('id')}"
        )
        hover.register(circ, txt)

    for e in observed_edges:
        (ca, cb) = edge_endpoints(e)
        ma = local_to_master(*ca, alignment)
        mb = local_to_master(*cb, alignment)
        pa = m2p.get(ma)
        pb = m2p.get(mb)
        if pa is None or pb is None:
            continue
        mid = ((pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5)
        conf = float(e["confidence"])
        radius = max(1.5, 6.0 * conf)
        (circ,) = ax.plot(
            mid[0], mid[1], "o",
            markersize=radius,
            color="none",
            markeredgecolor="none",
            picker=max(radius, 3.0),
            alpha=0.0,
        )
        orient = e["orientation"]
        if isinstance(orient, dict):
            orient = orient.get("kind")
        dropped = "  (DROPPED)" if conf < min_conf else ""
        txt = (
            f"edge local=(r={e['row']},c={e['col']}) {orient}\n"
            f"bit={e['bit']}  conf={conf:.3f}{dropped}\n"
            f"master_endpoints={ma} → {mb}"
        )
        hover.register(circ, txt)

    return hover


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main() -> int:
    args = parse_args()
    image = load_snap(args.target, args.snap, args.upscale)

    if args.json is not None:
        outcome = detection_from_json(args.json)
    else:
        outcome = detection_from_live_run(image, args.rows, args.cols, args.cell_size_mm)

    detection = outcome["detection"]
    alignment = outcome["alignment"]
    decode = outcome["decode"]
    observed_edges = outcome.get("observed_edges") or []

    fig, ax = plt.subplots(figsize=(14, 10))
    render_overlay(ax, image, detection, alignment, decode, observed_edges, args.min_conf)
    fig.suptitle(
        f"{args.target.name}  snap={args.snap}  upscale={args.upscale}×",
        fontsize=11,
    )
    attach_hover(fig, ax, detection, alignment, observed_edges, args.min_conf)
    fig.tight_layout()

    if args.save is not None:
        args.save.parent.mkdir(parents=True, exist_ok=True)
        fig.savefig(args.save, dpi=150, bbox_inches="tight")
        print(f"saved {args.save}", file=sys.stderr)

    plt.show()
    return 0


if __name__ == "__main__":
    sys.exit(main())
