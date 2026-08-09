#!/usr/bin/env python3
"""Run reproducible topological-grid overlays, quality, timing, and HTML."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any

from topological_report import write_report


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = ROOT / "scripts/topological_campaign.toml"
VENV_PYTHON = ROOT / ".venv/bin/python"
CAMPAIGN_PYTHON = VENV_PYTHON if VENV_PYTHON.is_file() else Path(sys.executable)

OPTION_SPECS: tuple[tuple[str, tuple[str, ...], Any], ...] = (
    ("chess_threshold", ("corner", "threshold"), 100.0),
    ("pre_blur_sigma", ("corner", "pre_blur_sigma"), 0.0),
    ("upscale", ("corner", "upscale"), 1.0),
    ("min_corner_strength", ("chessboard", "min_corner_strength"), 0.0),
    ("min_labeled_corners", ("chessboard", "min_labeled_corners"), 8),
    ("max_components", ("chessboard", "max_components"), 3),
    ("axis_align_tol_deg", ("expert", "topological", "axis_align_tol_deg"), 15.0),
    ("max_axis_sigma_deg", ("expert", "topological", "max_axis_sigma_deg"), 34.37747),
    ("opposing_edge_ratio_max", ("expert", "topological", "opposing_edge_ratio_max"), 10.0),
    ("min_quads_per_component", ("expert", "topological", "min_quads_per_component"), 1),
    ("cluster_axis_tol_deg", ("expert", "topological", "cluster_axis_tol_deg"), 16.0),
    ("edge_length_min_rel", ("expert", "topological", "edge_length_min_rel"), 0.0),
    ("edge_length_max_rel", ("expert", "topological", "edge_length_max_rel"), 1.8),
    ("geometry_recovery_tol_rel", ("expert", "recovery", "geometry_recovery_tol_rel"), 0.15),
)


def run(command: list[str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def reject_unknown(table: dict[str, Any], allowed: set[str], context: str) -> None:
    unknown = sorted(set(table) - allowed)
    if unknown:
        raise ValueError(f"unknown {context} key(s): {', '.join(unknown)}")


def load_config(path: Path) -> dict[str, Any]:
    with path.open("rb") as stream:
        config = tomllib.load(stream)
    reject_unknown(
        config,
        {"schema", "images", "output", "corner", "chessboard", "expert", "performance", "quality", "variants"},
        "campaign",
    )
    if config.get("schema") != 2:
        raise ValueError("campaign config must have schema = 2")
    images = config.get("images")
    if not isinstance(images, list) or not images or not all(isinstance(value, str) for value in images):
        raise ValueError("campaign config must list at least one image path")
    for image in images:
        if not (ROOT / image).is_file():
            raise FileNotFoundError(ROOT / image)

    output = config.get("output", {})
    reject_unknown(output, {"overlays", "runs", "determinism", "report"}, "output")
    for key in ("overlays", "runs", "determinism", "report"):
        if not isinstance(output.get(key), str):
            raise ValueError(f"output.{key} is required")
    reject_unknown(config.get("corner", {}), {"threshold", "pre_blur_sigma", "upscale"}, "corner")
    reject_unknown(
        config.get("chessboard", {}),
        {"min_corner_strength", "min_labeled_corners", "max_components"},
        "chessboard",
    )
    expert = config.get("expert", {})
    reject_unknown(expert, {"topological", "recovery"}, "expert")
    reject_unknown(
        expert.get("topological", {}),
        {
            "axis_align_tol_deg",
            "max_axis_sigma_deg",
            "opposing_edge_ratio_max",
            "min_quads_per_component",
            "cluster_axis_tol_deg",
            "edge_length_min_rel",
            "edge_length_max_rel",
        },
        "expert.topological",
    )
    reject_unknown(
        expert.get("recovery", {}),
        {"geometry_recovery_tol_rel"},
        "expert.recovery",
    )
    reject_unknown(config.get("performance", {}), {"warmups", "repeats"}, "performance")
    if "quality" in config:
        reject_unknown(
            config["quality"],
            {"image_root", "ground_truth", "matching_tolerance_px"},
            "quality",
        )

    variants = config.get("variants")
    if not isinstance(variants, list) or not variants:
        raise ValueError("campaign config must define at least one [[variants]] entry")
    ids: set[str] = set()
    primary = 0
    for variant in variants:
        reject_unknown(
            variant,
            {"id", "primary", "orientation_method", "enable_geometry_only_recovery"},
            "variant",
        )
        variant_id = variant.get("id")
        if not isinstance(variant_id, str) or not variant_id or variant_id in ids:
            raise ValueError("variant ids must be non-empty and unique")
        ids.add(variant_id)
        if variant.get("orientation_method") not in {"ring_fit", "disk_fit"}:
            raise ValueError(f"variant {variant_id}: orientation_method must be ring_fit or disk_fit")
        if not isinstance(variant.get("enable_geometry_only_recovery"), bool):
            raise ValueError(f"variant {variant_id}: enable_geometry_only_recovery must be boolean")
        primary += bool(variant.get("primary", False))
    if primary != 1:
        raise ValueError("exactly one variant must set primary = true")
    return config


def nested(config: dict[str, Any], path: tuple[str, ...], default: Any) -> Any:
    value: Any = config
    for key in path:
        if not isinstance(value, dict) or key not in value:
            return default
        value = value[key]
    return value


def effective_options(config: dict[str, Any], variant: dict[str, Any]) -> dict[str, Any]:
    values = {name: nested(config, path, default) for name, path, default in OPTION_SPECS}
    values["orientation_method"] = variant["orientation_method"]
    values["enable_geometry_only_recovery"] = variant["enable_geometry_only_recovery"]
    return values


def append_options(command: list[str], values: dict[str, Any]) -> None:
    for name, value in values.items():
        rendered = str(value).lower() if isinstance(value, bool) else str(value)
        command.extend((f"--{name.replace('_', '-')}", rendered))


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


def variant_overlay_dir(config: dict[str, Any], variant: dict[str, Any]) -> Path:
    return ROOT / config["output"]["overlays"] / variant["id"]


def variant_run_dir(config: dict[str, Any], variant: dict[str, Any]) -> Path:
    return ROOT / config["output"]["runs"] / variant["id"]


def render_overlays(config: dict[str, Any], variant: dict[str, Any]) -> None:
    command = [str(CAMPAIGN_PYTHON), "scripts/render_topological_blog_overlays.py"]
    command.extend(("--out-dir", str(variant_overlay_dir(config, variant))))
    command.extend(("--variant-name", variant["id"]))
    append_options(command, effective_options(config, variant))
    command.append("--images")
    command.extend(config["images"])
    run(command)


def evaluate_quality(config: dict[str, Any], variant: dict[str, Any]) -> None:
    quality = config.get("quality")
    if not quality:
        return
    ground_truth = ROOT / quality["ground_truth"]
    if not ground_truth.is_file():
        print(f"quality: {ground_truth.relative_to(ROOT)} is absent; images are unscored")
        return
    out = variant_run_dir(config, variant) / "quality.json"
    command = [
        str(CAMPAIGN_PYTHON),
        "scripts/evaluate_topological_quality.py",
        "--trace-dir",
        str(variant_overlay_dir(config, variant)),
        "--image-root",
        quality["image_root"],
        "--ground-truth",
        str(ground_truth),
        "--out",
        str(out),
        "--tolerance-px",
        str(quality.get("matching_tolerance_px", 3.0)),
        "--detection-scale",
        str(nested(config, ("corner", "upscale"), 1.0)),
        "--images",
        *config["images"],
    ]
    run(command)


def measure_performance(config: dict[str, Any], variant: dict[str, Any]) -> None:
    performance = config.get("performance", {})
    command = [
        "cargo",
        "run",
        "--release",
        "-p",
        "calib-targets-bench",
        "--bin",
        "topo_stage_timing",
        "--",
        "--out",
        str(variant_run_dir(config, variant) / "timing.json"),
        "--warmup",
        str(performance.get("warmups", 5)),
        "--repeats",
        str(performance.get("repeats", 50)),
    ]
    append_options(command, effective_options(config, variant))
    command.append("--images")
    command.extend(config["images"])
    run(command)


def artifact_hashes(config: dict[str, Any], variant: dict[str, Any]) -> dict[str, str]:
    root = variant_overlay_dir(config, variant)
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    hashes: dict[str, str] = {}
    for image in manifest["images"]:
        directory = ROOT / image["output_dir"]
        for name in ["trace.json", *image["stages"]]:
            path = directory / name
            hashes[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def write_determinism(config: dict[str, Any], snapshots: dict[str, dict[str, str]]) -> None:
    variants = []
    mismatch_count = 0
    for variant in config["variants"]:
        second = artifact_hashes(config, variant)
        first = snapshots[variant["id"]]
        mismatches = sorted(path for path, digest in first.items() if second.get(path) != digest)
        mismatch_count += len(mismatches)
        variants.append(
            {
                "id": variant["id"],
                "artifacts_checked": len(first),
                "mismatches": mismatches,
                "passed": not mismatches,
            }
        )
    report = {
        "schema": 2,
        "scope": "exact Rust trace JSON and every rendered blog overlay",
        "variants": variants,
        "passed": mismatch_count == 0,
    }
    path = ROOT / config["output"]["determinism"]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    if mismatch_count:
        raise SystemExit(f"determinism gate failed for {mismatch_count} artifact(s)")
    print(f"wrote {path.relative_to(ROOT)}")


def generate_report(config: dict[str, Any], config_path: Path) -> None:
    write_report(ROOT, config, config_path, ROOT / config["output"]["report"])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=("overlays", "quality", "determinism", "performance", "report", "all"),
        nargs="?",
        default="all",
    )
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--skip-build", action="store_true", help="Reuse the installed Python extension.")
    args = parser.parse_args()
    config_path = args.config.resolve()
    config = load_config(config_path)
    variants = config["variants"]

    if args.action in {"overlays", "determinism", "all"} and not args.skip_build:
        build_python_extension()

    if args.action in {"overlays", "determinism", "all"}:
        for variant in variants:
            render_overlays(config, variant)

    if args.action in {"quality", "all"}:
        for variant in variants:
            evaluate_quality(config, variant)

    if args.action in {"determinism", "all"}:
        snapshots = {variant["id"]: artifact_hashes(config, variant) for variant in variants}
        for variant in variants:
            render_overlays(config, variant)
        write_determinism(config, snapshots)

    if args.action in {"performance", "all"}:
        for variant in variants:
            measure_performance(config, variant)

    if args.action in {"report", "all"}:
        generate_report(config, config_path)


if __name__ == "__main__":
    main()
