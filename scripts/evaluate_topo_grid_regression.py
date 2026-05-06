#!/usr/bin/env python3
"""Evaluate the synthetic topological-grid regression set.

The report intentionally measures full chessboard detection through the public
Python API. ChESS extraction is included in the timings; grid-only scaling is
covered by the Rust benches.
"""

from __future__ import annotations

import argparse
import itertools
import json
import platform
import statistics
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
from PIL import Image

import calib_targets as ct


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPO_ROOT / "testdata/02-topo-grid/regression_manifest.json"
DEFAULT_OUTPUT_DIR = REPO_ROOT / "tools/out/topo-grid-regression"


@dataclass(frozen=True)
class Variant:
    name: str
    algorithm: str
    chess_cfg: ct.ChessConfig
    min_labeled_corners: int = 4
    upscale: float = 1.0
    axis_align_tol_deg: float | None = None
    diagonal_angle_tol_deg: float | None = None


def parse_float_list(values: list[str] | None) -> list[float | None]:
    if not values:
        return [None]
    parsed: list[float | None] = []
    for value in values:
        for item in value.split(","):
            item = item.strip()
            if item:
                parsed.append(float(item))
    return parsed or [None]


def run_text(cmd: list[str]) -> str | None:
    try:
        return subprocess.check_output(cmd, cwd=REPO_ROOT, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def cpu_name() -> str:
    mac_name = run_text(["sysctl", "-n", "machdep.cpu.brand_string"])
    if mac_name:
        return mac_name
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        for line in cpuinfo.read_text(errors="ignore").splitlines():
            if line.lower().startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor() or platform.machine()


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def selected_cases(manifest: dict[str, Any], image_filters: set[str]) -> list[dict[str, Any]]:
    cases = manifest.get("images", [])
    if not image_filters:
        return cases
    selected: list[dict[str, Any]] = []
    for case in cases:
        path = Path(case["path"])
        if str(path) in image_filters or path.name in image_filters:
            selected.append(case)
    missing = image_filters - {Path(case["path"]).name for case in selected} - {
        str(Path(case["path"])) for case in selected
    }
    if missing:
        raise SystemExit(f"unknown image filter(s): {', '.join(sorted(missing))}")
    return selected


def load_image(path: Path, upscale: float) -> np.ndarray:
    with Image.open(path) as image:
        gray = image.convert("L")
        if abs(upscale - 1.0) > 1e-6:
            width = max(1, round(gray.width * upscale))
            height = max(1, round(gray.height * upscale))
            gray = gray.resize((width, height), Image.Resampling.BICUBIC)
        return np.ascontiguousarray(np.array(gray, dtype=np.uint8))


def params_for(variant: Variant) -> ct.ChessboardParams:
    topological = ct.TopologicalParams()
    if variant.axis_align_tol_deg is not None:
        topological.axis_align_tol_rad = np.deg2rad(variant.axis_align_tol_deg).item()
    if variant.diagonal_angle_tol_deg is not None:
        topological.diagonal_angle_tol_rad = np.deg2rad(variant.diagonal_angle_tol_deg).item()
    return ct.ChessboardParams(
        graph_build_algorithm=variant.algorithm,
        min_labeled_corners=variant.min_labeled_corners,
        topological=topological,
    )


def detection_components(image: np.ndarray, variant: Variant) -> list[ct.ChessboardDetectionResult]:
    return ct.detect_chessboard_all(
        image,
        chess_cfg=variant.chess_cfg,
        params=params_for(variant),
    )


def labels_for(result: ct.ChessboardDetectionResult) -> list[tuple[int, int]]:
    return [
        (corner.grid.i, corner.grid.j)
        for corner in result.detection.corners
        if corner.grid is not None
    ]


def hole_count(labels: list[tuple[int, int]]) -> int:
    if not labels:
        return 0
    coords = set(labels)
    min_i = min(i for i, _ in labels)
    max_i = max(i for i, _ in labels)
    min_j = min(j for _, j in labels)
    max_j = max(j for _, j in labels)
    return sum(
        1
        for j in range(min_j, max_j + 1)
        for i in range(min_i, max_i + 1)
        if (i, j) not in coords
    )


def invariant_report(result: ct.ChessboardDetectionResult | None) -> dict[str, bool]:
    if result is None:
        return {
            "finite_positions": True,
            "no_duplicate_labels": True,
            "origin_rebased_to_zero": True,
            "visual_top_left_orientation": True,
        }
    labels = labels_for(result)
    corners = result.detection.corners
    finite_positions = all(
        np.isfinite(corner.position[0]) and np.isfinite(corner.position[1]) for corner in corners
    )
    no_duplicate_labels = len(labels) == len(set(labels))
    if labels:
        origin_rebased = min(i for i, _ in labels) == 0 and min(j for _, j in labels) == 0
    else:
        origin_rebased = True

    by_label = {
        (corner.grid.i, corner.grid.j): corner.position
        for corner in corners
        if corner.grid is not None
    }
    dx: list[float] = []
    dy: list[float] = []
    for (i, j), (x, y) in by_label.items():
        if (i + 1, j) in by_label:
            dx.append(by_label[(i + 1, j)][0] - x)
        if (i, j + 1) in by_label:
            dy.append(by_label[(i, j + 1)][1] - y)
    visual_ok = (not dx or statistics.fmean(dx) > 0.0) and (not dy or statistics.fmean(dy) > 0.0)
    return {
        "finite_positions": finite_positions,
        "no_duplicate_labels": no_duplicate_labels,
        "origin_rebased_to_zero": origin_rebased,
        "visual_top_left_orientation": visual_ok,
    }


def detection_summary(components: list[ct.ChessboardDetectionResult]) -> dict[str, Any]:
    component_counts = [len(component.detection.corners) for component in components]
    best = max(components, key=lambda item: len(item.detection.corners), default=None)
    best_labels = labels_for(best) if best else []
    return {
        "components": len(components),
        "component_labelled_counts": component_counts,
        "labelled_count": len(best_labels),
        "total_labelled_count": sum(component_counts),
        "holes": hole_count(best_labels),
        "invariants": invariant_report(best),
    }


def trace_summary(image: np.ndarray, variant: Variant) -> dict[str, Any]:
    trace_params = params_for(
        Variant(
            name=variant.name,
            algorithm="topological",
            chess_cfg=variant.chess_cfg,
            min_labeled_corners=variant.min_labeled_corners,
            axis_align_tol_deg=variant.axis_align_tol_deg,
            diagonal_angle_tol_deg=variant.diagonal_angle_tol_deg,
        )
    )
    payload = ct.trace_chessboard_topological(image, chess_cfg=variant.chess_cfg, params=trace_params)
    trace = payload.get("trace") or {}
    trace_corners = trace.get("corners") or []
    diagnostics = trace.get("diagnostics") or {}
    return {
        "raw_corners": len(payload.get("corners") or []),
        "usable_corners": sum(1 for corner in trace_corners if corner.get("usable")),
        "trace_components": len(trace.get("components") or []),
        "trace_diagnostics": diagnostics,
        "trace_error": payload.get("error"),
    }


def time_variant(image: np.ndarray, variant: Variant, repeats: int, warmup: int) -> tuple[list[float], list[ct.ChessboardDetectionResult]]:
    for _ in range(warmup):
        detection_components(image, variant)
    timings: list[float] = []
    components: list[ct.ChessboardDetectionResult] = []
    for _ in range(repeats):
        start = time.perf_counter()
        components = detection_components(image, variant)
        timings.append((time.perf_counter() - start) * 1000.0)
    return timings, components


def timing_summary(samples_ms: list[float]) -> dict[str, float]:
    if not samples_ms:
        return {"mean_ms": 0.0, "p50_ms": 0.0, "p95_ms": 0.0, "max_ms": 0.0}
    ordered = sorted(samples_ms)
    p95_index = min(len(ordered) - 1, int(np.ceil(0.95 * len(ordered))) - 1)
    return {
        "mean_ms": statistics.fmean(samples_ms),
        "p50_ms": statistics.median(samples_ms),
        "p95_ms": ordered[p95_index],
        "max_ms": max(samples_ms),
    }


def default_chess_cfg(blur_sigma: float | None, threshold: float | None) -> ct.ChessConfig:
    cfg = ct.ChessConfig()
    if blur_sigma is not None:
        cfg.pre_blur_sigma_px = blur_sigma
    if threshold is not None:
        cfg.threshold_value = threshold
    return cfg


def variants_for_case(
    case: dict[str, Any],
    algorithm: str,
    blur_sigma: float | None,
    threshold: float | None,
    axis_values: list[float | None],
    diagonal_values: list[float | None],
) -> list[Variant]:
    algorithms = ["topological", "chessboard_v2"] if algorithm == "all" else [algorithm]
    variants: list[Variant] = []
    for algo in algorithms:
        if algo == "low_res":
            continue
        for axis, diagonal in itertools.product(axis_values, diagonal_values):
            suffix = ""
            if axis is not None or diagonal is not None:
                suffix = f"/axis={axis or 'default'}/diag={diagonal or 'default'}"
            variants.append(
                Variant(
                    name=f"{algo}{suffix}",
                    algorithm=algo,
                    chess_cfg=default_chess_cfg(blur_sigma, threshold),
                    axis_align_tol_deg=axis,
                    diagonal_angle_tol_deg=diagonal,
                )
            )

    if algorithm in {"all", "low_res"} and case.get("low_res"):
        low = case["low_res"]
        chess = low.get("chess", {})
        cfg = ct.ChessConfig(
            threshold_value=threshold
            if threshold is not None
            else float(chess.get("threshold_value", ct.ChessConfig().threshold_value)),
            pre_blur_sigma_px=blur_sigma
            if blur_sigma is not None
            else float(chess.get("pre_blur_sigma_px", 0.0)),
        )
        variants.append(
            Variant(
                name="low_res",
                algorithm=low["algorithm"],
                chess_cfg=cfg,
                min_labeled_corners=4,
                upscale=float(low.get("upscale", 1.0)),
            )
        )
    return variants


def evaluate(args: argparse.Namespace) -> dict[str, Any]:
    manifest = load_manifest(args.manifest)
    cases = selected_cases(manifest, set(args.image or []))
    axis_values = parse_float_list(args.axis_align_tol_deg)
    diagonal_values = parse_float_list(args.diagonal_angle_tol_deg)

    runs: list[dict[str, Any]] = []
    for case in cases:
        image_path = REPO_ROOT / case["path"]
        for variant in variants_for_case(
            case,
            args.algorithm,
            args.blur_sigma,
            args.threshold,
            axis_values,
            diagonal_values,
        ):
            image = load_image(image_path, variant.upscale)
            samples_ms, components = time_variant(image, variant, args.repeats, args.warmup)
            run = {
                "image": case["path"],
                "variant": variant.name,
                "algorithm": variant.algorithm,
                "resolution": {"width": int(image.shape[1]), "height": int(image.shape[0])},
                "upscale": variant.upscale,
                "chess_config": variant.chess_cfg.to_dict(),
                "topological_overrides": {
                    "axis_align_tol_deg": variant.axis_align_tol_deg,
                    "diagonal_angle_tol_deg": variant.diagonal_angle_tol_deg,
                },
                **trace_summary(image, variant),
                **detection_summary(components),
                "timing": timing_summary(samples_ms),
                "samples_ms": samples_ms,
            }
            runs.append(run)
            print(
                f"{Path(case['path']).name:18} {variant.name:32} "
                f"{run['resolution']['width']}x{run['resolution']['height']} "
                f"raw={run['raw_corners']:3} usable={run['usable_corners']:3} "
                f"labelled={run['labelled_count']:3} comps={run['components']:2} "
                f"mean={run['timing']['mean_ms']:.2f}ms p95={run['timing']['p95_ms']:.2f}ms"
            )

    return {
        "schema": 1,
        "metadata": {
            "git_sha": run_text(["git", "rev-parse", "--short", "HEAD"]),
            "rust_version": run_text(["rustc", "-Vv"]),
            "cpu": cpu_name(),
            "python": platform.python_version(),
            "profile": "Python bindings full detection",
            "feature_flags": "workspace defaults",
            "repeats": args.repeats,
            "warmup": args.warmup,
        },
        "manifest": str(args.manifest),
        "runs": runs,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--image", action="append", help="Image basename or manifest path to evaluate")
    parser.add_argument(
        "--algorithm",
        choices=["all", "topological", "chessboard_v2", "low_res"],
        default="all",
    )
    parser.add_argument("--repeats", type=int, default=20)
    parser.add_argument("--warmup", type=int, default=3)
    parser.add_argument("--blur-sigma", type=float)
    parser.add_argument("--threshold", type=float)
    parser.add_argument(
        "--axis-align-tol-deg",
        action="append",
        help="Topological tolerance sweep value(s), comma-separated or repeated",
    )
    parser.add_argument(
        "--diagonal-angle-tol-deg",
        action="append",
        help="Topological tolerance sweep value(s), comma-separated or repeated",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.repeats <= 0:
        raise SystemExit("--repeats must be positive")
    if args.warmup < 0:
        raise SystemExit("--warmup must be non-negative")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report = evaluate(args)
    output = args.output_dir / "report.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"wrote {output}")


if __name__ == "__main__":
    main()
