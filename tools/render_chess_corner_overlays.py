#!/usr/bin/env python3
"""Render ChESS corner and local-axis overlays for inspection.

This is a focused visual-debug companion to
``scripts/render_topological_blog_overlays.py``. It only renders the raw
corner cloud and the two local orientation axes returned by chess-corners,
plus summary counts from the Rust-backed topological trace.

Examples:

    .venv/bin/python tools/render_chess_corner_overlays.py \
        testdata/02-topo-grid/GeminiChess1.png \
        testdata/02-topo-grid/GeminiChess2.png \
        testdata/puzzleboard_reference/example2.png \
        --threshold 100 \
        --orientation-method both

    .venv/bin/python tools/render_chess_corner_overlays.py \
        testdata/02-topo-grid/GeminiChess2.png \
        --threshold-kind relative \
        --threshold 0.18 \
        --variant-name rel018
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.lines import Line2D
from PIL import Image

import calib_targets as ct


METHODS = ("ring_fit", "disk_fit")


def load_gray(path: Path, upscale: float) -> np.ndarray:
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


def threshold_config(kind: str, value: float) -> ct.Threshold:
    if kind == "absolute":
        return ct.Threshold.absolute(value)
    if kind == "relative":
        return ct.Threshold.relative(value)
    raise ValueError(f"unsupported threshold kind: {kind}")


def method_list(value: str) -> list[str]:
    if value == "both":
        return list(METHODS)
    if value in METHODS:
        return [value]
    raise ValueError(f"unsupported orientation method: {value}")


def trace_payload(
    image: np.ndarray,
    *,
    threshold: ct.Threshold,
    orientation_method: str,
    pre_blur_sigma_px: float,
) -> dict[str, Any]:
    chess_cfg = ct.ChessConfig(
        threshold=threshold,
        orientation_method=orientation_method,
    )
    params = ct.ChessboardParams()
    return ct.trace_chessboard_topological(
        image,
        chess_cfg=chess_cfg,
        params=params,
        pre_blur_sigma_px=pre_blur_sigma_px,
    )


def corner_stats(corners: list[dict[str, Any]]) -> dict[str, Any]:
    strengths = np.array([float(c["strength"]) for c in corners], dtype=float)
    sigmas: list[float] = []
    for c in corners:
        for axis in c["axes"]:
            sigma = float(axis["sigma"])
            if math.isfinite(sigma) and sigma < math.pi - 1e-3:
                sigmas.append(math.degrees(sigma))
    return {
        "corner_count": len(corners),
        "strength_min": float(strengths.min()) if strengths.size else 0.0,
        "strength_median": float(np.median(strengths)) if strengths.size else 0.0,
        "strength_max": float(strengths.max()) if strengths.size else 0.0,
        "axis_sigma_deg_median": float(np.median(sigmas)) if sigmas else None,
        "axis_sigma_deg_p90": float(np.percentile(sigmas, 90)) if sigmas else None,
    }


def final_labelled_count(payload: dict[str, Any]) -> int:
    detections = payload.get("detections") or []
    if not detections:
        return 0
    return len(detections[0].get("target", {}).get("corners", []))


def draw_corner_axes(
    ax: plt.Axes,
    image: np.ndarray,
    payload: dict[str, Any],
    title: str,
) -> None:
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)
    corners = payload.get("corners") or []
    if corners:
        xs = np.array([float(c["position"][0]) for c in corners])
        ys = np.array([float(c["position"][1]) for c in corners])
        strengths = np.array([float(c["strength"]) for c in corners])
        lo = np.percentile(strengths, 5) if len(strengths) > 1 else strengths.min()
        hi = np.percentile(strengths, 95) if len(strengths) > 1 else strengths.max()
        if hi <= lo:
            colors: Any = "#fdd835"
        else:
            colors = np.clip((strengths - lo) / (hi - lo), 0.0, 1.0)
        ax.scatter(
            xs,
            ys,
            s=12,
            c=colors,
            cmap="plasma",
            edgecolors="black",
            linewidths=0.25,
            zorder=5,
        )

        base_len = max(5.0, min(16.0, min(image.shape) * 0.018))
        for c in corners:
            x = float(c["position"][0])
            y = float(c["position"][1])
            for axis, color in zip(c["axes"], ("#00d5ff", "#ff3d71")):
                theta = float(axis["angle"])
                sigma = float(axis["sigma"])
                if not math.isfinite(theta) or not math.isfinite(sigma):
                    continue
                if sigma >= math.pi - 1e-3:
                    continue
                length = min(base_len, 3.8 / max(sigma, 0.08))
                dx = math.cos(theta) * length
                dy = math.sin(theta) * length
                ax.plot(
                    [x - dx, x + dx],
                    [y - dy, y + dy],
                    color=color,
                    lw=0.55,
                    alpha=0.72,
                    zorder=4,
                )

    trace = payload.get("trace") or {}
    diagnostics = trace.get("diagnostics") or {}
    ax.set_title(
        f"{title}\n"
        f"raw={len(corners)} usable={diagnostics.get('corners_used', 'n/a')} "
        f"final={final_labelled_count(payload)}",
        fontsize=9,
    )
    ax.set_xlim(0, image.shape[1])
    ax.set_ylim(image.shape[0], 0)
    ax.set_aspect("equal")
    ax.axis("off")


def legend_handles() -> list[Line2D]:
    return [
        Line2D(
            [0],
            [0],
            marker="o",
            color="black",
            markerfacecolor="#fdd835",
            lw=0,
            markersize=5,
            label="corner response",
        ),
        Line2D([0], [0], color="#00d5ff", lw=1.5, label="axis[0]"),
        Line2D([0], [0], color="#ff3d71", lw=1.5, label="axis[1]"),
    ]


def save_single(
    image: np.ndarray,
    payload: dict[str, Any],
    out_path: Path,
    title: str,
) -> None:
    height, width = image.shape
    dpi = 150
    fig_width = min(14.0, max(7.0, width / dpi))
    fig_height = fig_width * height / width
    fig, ax = plt.subplots(figsize=(fig_width, fig_height), dpi=dpi)
    draw_corner_axes(ax, image, payload, title)
    ax.legend(handles=legend_handles(), loc="lower right", fontsize=7, framealpha=0.85)
    fig.tight_layout(pad=0.05)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight", pad_inches=0.02)
    plt.close(fig)


def save_side_by_side(
    image: np.ndarray,
    payloads: dict[str, dict[str, Any]],
    methods: list[str],
    out_path: Path,
    image_name: str,
) -> None:
    height, width = image.shape
    dpi = 145
    fig_width = min(18.0, max(10.0, len(methods) * width / dpi))
    fig_height = fig_width * height / (len(methods) * width)
    fig, axes = plt.subplots(1, len(methods), figsize=(fig_width, fig_height), dpi=dpi)
    axes_arr = np.atleast_1d(axes)
    for ax, method in zip(axes_arr, methods):
        draw_corner_axes(ax, image, payloads[method], f"{image_name} - {method}")
    axes_arr[-1].legend(handles=legend_handles(), loc="lower right", fontsize=7, framealpha=0.85)
    fig.tight_layout(pad=0.05)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out_path, bbox_inches="tight", pad_inches=0.02)
    plt.close(fig)


def config_slug(args: argparse.Namespace) -> str:
    threshold = f"{args.threshold:g}".replace(".", "p")
    parts = [f"{args.threshold_kind}{threshold}"]
    if args.pre_blur_sigma > 0.0:
        parts.append(f"blur{args.pre_blur_sigma:g}".replace(".", "p"))
    if args.upscale != 1.0:
        parts.append(f"up{args.upscale:g}".replace(".", "p"))
    if args.variant_name:
        parts.append(args.variant_name)
    return "-".join(parts)


def render(args: argparse.Namespace) -> dict[str, Any]:
    threshold = threshold_config(args.threshold_kind, args.threshold)
    methods = method_list(args.orientation_method)
    rows: list[dict[str, Any]] = []
    slug = config_slug(args)

    for image_path in args.images:
        image = load_gray(image_path, args.upscale)
        stem_dir = args.out_dir / image_path.stem / slug
        payloads: dict[str, dict[str, Any]] = {}
        for method in methods:
            payload = trace_payload(
                image,
                threshold=threshold,
                orientation_method=method,
                pre_blur_sigma_px=args.pre_blur_sigma,
            )
            payloads[method] = payload
            trace = payload.get("trace") or {}
            diagnostics = trace.get("diagnostics") or {}
            out_path = stem_dir / f"corners-{method}.png"
            save_single(
                image,
                payload,
                out_path,
                f"{image_path.name} - {method}",
            )
            row = {
                "image": str(image_path),
                "method": method,
                "width": int(image.shape[1]),
                "height": int(image.shape[0]),
                **corner_stats(payload.get("corners") or []),
                "usable_corners": diagnostics.get("corners_used"),
                "quads_kept": diagnostics.get("quads_kept"),
                "final_labelled": final_labelled_count(payload),
                "overlay": str(out_path),
            }
            rows.append(row)
            print(
                f"{image_path} {method}: "
                f"raw={row['corner_count']} usable={row['usable_corners']} "
                f"final={row['final_labelled']} -> {out_path}"
            )
        if args.side_by_side and len(methods) > 1:
            save_side_by_side(
                image,
                payloads,
                methods,
                stem_dir / "corners-side-by-side.png",
                image_path.name,
            )

    manifest = {
        "schema": 1,
        "description": "Local ChESS corner and local-axis inspection overlays.",
        "params": {
            "threshold_kind": args.threshold_kind,
            "threshold": args.threshold,
            "orientation_method": args.orientation_method,
            "pre_blur_sigma": args.pre_blur_sigma,
            "upscale": args.upscale,
            "variant_name": args.variant_name,
        },
        "rows": rows,
    }
    args.out_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = args.out_dir / args.manifest_name
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"wrote manifest -> {manifest_path}")
    return manifest


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("images", nargs="+", type=Path, help="Input image paths.")
    parser.add_argument("--out-dir", type=Path, default=Path("preview/topo-corner-inspection"))
    parser.add_argument("--manifest-name", default="manifest.json")
    parser.add_argument("--variant-name", default=None)
    parser.add_argument("--threshold", type=float, default=100.0)
    parser.add_argument("--threshold-kind", choices=["absolute", "relative"], default="absolute")
    parser.add_argument(
        "--orientation-method",
        choices=["ring_fit", "disk_fit", "both"],
        default="both",
    )
    parser.add_argument("--pre-blur-sigma", type=float, default=0.0)
    parser.add_argument("--upscale", type=float, default=1.0)
    parser.add_argument("--no-side-by-side", dest="side_by_side", action="store_false")
    parser.set_defaults(side_by_side=True)
    return parser.parse_args()


def main() -> None:
    render(parse_args())


if __name__ == "__main__":
    main()
