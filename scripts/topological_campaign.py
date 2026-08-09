#!/usr/bin/env python3
"""Run reproducible blog overlays and production timing from one TOML config."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = ROOT / "scripts/topological_campaign.toml"
VENV_PYTHON = ROOT / ".venv/bin/python"
CAMPAIGN_PYTHON = VENV_PYTHON if VENV_PYTHON.is_file() else Path(sys.executable)
OVERLAY_ARTIFACTS = (
    "trace.json",
    "00-input.png",
    "01-corners-axes.png",
    "02-strength-sigma-admission.png",
    "03-cluster-assignments.png",
    "04-projective-grid-usable.png",
    "05-delaunay-edge-kinds.png",
    "06-mergeable-triangles.png",
    "07-raw-quads.png",
    "08-topology-filter.png",
    "09-geometry-filter.png",
    "10-scale-filter.png",
    "11-walk-components.png",
    "12-generic-merge-fit.png",
    "13-chessboard-recovery.png",
    "14-final-grid.png",
)


def run(command: list[str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def load_config(path: Path) -> dict[str, Any]:
    with path.open("rb") as stream:
        config = tomllib.load(stream)
    if config.get("schema") != 1:
        raise ValueError("campaign config must have schema = 1")
    images = config.get("images")
    if not isinstance(images, list) or not images:
        raise ValueError("campaign config must list at least one image")
    for image in images:
        if not (ROOT / image).is_file():
            raise FileNotFoundError(ROOT / image)
    return config


def option(command: list[str], name: str, value: Any) -> None:
    command.extend((f"--{name.replace('_', '-')}", str(value)))


def common_options(config: dict[str, Any]) -> list[str]:
    corner = config.get("corner", {})
    chessboard = config.get("chessboard", {})
    topological = config.get("expert", {}).get("topological", {})
    values = {
        "chess_threshold": corner.get("threshold", 100.0),
        "orientation_method": corner.get("orientation_method", "ring_fit").replace("_", "-"),
        "min_corner_strength": chessboard.get("min_corner_strength", 0.0),
        "min_labeled_corners": chessboard.get("min_labeled_corners", 8),
        "max_components": chessboard.get("max_components", 3),
        "pre_blur_sigma": corner.get("pre_blur_sigma", 0.0),
        "upscale": corner.get("upscale", 1.0),
        "axis_align_tol_deg": topological.get("axis_align_tol_deg", 15.0),
        "max_axis_sigma_deg": topological.get("max_axis_sigma_deg", 34.37747),
        "opposing_edge_ratio_max": topological.get("opposing_edge_ratio_max", 10.0),
        "min_quads_per_component": topological.get("min_quads_per_component", 1),
        "edge_length_min_rel": topological.get("edge_length_min_rel", 0.0),
        "edge_length_max_rel": topological.get("edge_length_max_rel", 1.8),
    }
    command: list[str] = []
    for name, value in values.items():
        option(command, name, value)
    return command


def build_python_extension() -> None:
    run([
        str(CAMPAIGN_PYTHON),
        "-m",
        "maturin",
        "develop",
        "--release",
        "-m",
        "crates/calib-targets-py/Cargo.toml",
    ])


def render_overlays(config: dict[str, Any]) -> None:
    corner = config.get("corner", {})
    topological = config.get("expert", {}).get("topological", {})
    command = [str(CAMPAIGN_PYTHON), "scripts/render_topological_blog_overlays.py"]
    option(command, "out_dir", config["output"]["overlays"])
    option(command, "chess_threshold", corner.get("threshold", 100.0))
    option(command, "orientation_method", corner.get("orientation_method", "ring_fit"))
    option(command, "min_corner_strength", config.get("chessboard", {}).get("min_corner_strength", 0.0))
    option(command, "min_labeled_corners", config.get("chessboard", {}).get("min_labeled_corners", 8))
    option(command, "max_components", config.get("chessboard", {}).get("max_components", 3))
    option(command, "pre_blur_sigma", corner.get("pre_blur_sigma", 0.0))
    option(command, "upscale", corner.get("upscale", 1.0))
    for name, default in (
        ("axis_align_tol_deg", 15.0),
        ("max_axis_sigma_deg", 34.37747),
        ("opposing_edge_ratio_max", 10.0),
        ("min_quads_per_component", 1),
        ("cluster_axis_tol_deg", 16.0),
        ("edge_length_min_rel", 0.0),
        ("edge_length_max_rel", 1.8),
    ):
        option(command, name, topological.get(name, default))
    command.append("--images")
    command.extend(config["images"])
    run(command)


def measure_performance(config: dict[str, Any]) -> None:
    perf = config.get("performance", {})
    command = [
        "cargo",
        "run",
        "--release",
        "-p",
        "calib-targets-bench",
        "--bin",
        "topo_stage_timing",
        "--",
    ]
    option(command, "out", config["output"]["performance"])
    option(command, "warmup", perf.get("warmups", 5))
    option(command, "repeats", perf.get("repeats", 50))
    command.extend(common_options(config))
    command.append("--images")
    command.extend(config["images"])
    run(command)


def evaluate_quality(config: dict[str, Any]) -> None:
    quality = config.get("quality", {})
    command = [str(CAMPAIGN_PYTHON), "scripts/evaluate_topological_quality.py"]
    option(command, "trace_dir", config["output"]["overlays"])
    option(command, "image_root", quality["image_root"])
    option(command, "ground_truth", quality["ground_truth"])
    option(command, "out", quality["report"])
    option(command, "tolerance_px", quality.get("matching_tolerance_px", 3.0))
    option(command, "detection_scale", config.get("corner", {}).get("upscale", 1.0))
    run(command)


def artifact_hashes(config: dict[str, Any]) -> dict[str, str]:
    root = ROOT / config["output"]["overlays"]
    hashes: dict[str, str] = {}
    for image in config["images"]:
        stem = Path(image).stem
        for name in OVERLAY_ARTIFACTS:
            path = root / stem / name
            hashes[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def verify_determinism(config: dict[str, Any], first: dict[str, str]) -> None:
    render_overlays(config)
    second = artifact_hashes(config)
    mismatches = sorted(path for path in first if first[path] != second.get(path))
    report = {
        "schema": 1,
        "scope": "exact Rust trace JSON and every rendered blog overlay",
        "artifacts_checked": len(first),
        "mismatches": mismatches,
        "passed": not mismatches,
    }
    path = ROOT / config["output"]["determinism"]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    if mismatches:
        raise SystemExit(f"determinism gate failed for {len(mismatches)} artifact(s)")
    print(f"wrote {path.relative_to(ROOT)}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=("overlays", "quality", "determinism", "performance", "all"),
        nargs="?",
        default="all",
    )
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--skip-build", action="store_true", help="Reuse the installed Python extension.")
    args = parser.parse_args()
    config = load_config(args.config.resolve())
    if args.action in ("overlays", "determinism", "all"):
        if not args.skip_build:
            build_python_extension()
        render_overlays(config)
        evaluate_quality(config)
        if args.action in ("determinism", "all"):
            verify_determinism(config, artifact_hashes(config))
    elif args.action == "quality":
        evaluate_quality(config)
    if args.action in ("performance", "all"):
        measure_performance(config)


if __name__ == "__main__":
    main()
