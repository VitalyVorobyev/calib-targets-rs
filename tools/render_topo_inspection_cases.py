#!/usr/bin/env python3
"""Render topological-grid stage overlays from a case manifest.

The manifest stores per-image ChESS and topological parameters so visual
inspection runs are reproducible. By default it uses
``tools/topo_inspection_cases.json``:

    .venv/bin/python tools/render_topo_inspection_cases.py

To run one case:

    .venv/bin/python tools/render_topo_inspection_cases.py --only GeminiChess2

The renderer delegates each image to
``scripts/render_topological_blog_overlays.py`` so stage images remain
consistent with the older blog/debug overlays.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG = REPO_ROOT / "tools/topo_inspection_cases.json"
DEFAULT_OUT_DIR = REPO_ROOT / "preview/topo-grid-inspection"
OVERLAY_SCRIPT = REPO_ROOT / "scripts/render_topological_blog_overlays.py"


RENDER_FIELDS = {
    "final_algorithm",
    "chess_threshold",
    "chess_threshold_kind",
    "orientation_method",
    "pre_blur_sigma",
    "upscale",
    "axis_align_tol_deg",
    "diagonal_angle_tol_deg",
    "max_axis_sigma_deg",
    "edge_ratio_max",
    "min_quads_per_component",
    "cluster_axis_tol_deg",
    "quad_edge_min_rel",
    "quad_edge_max_rel",
}


def load_renderer():
    spec = importlib.util.spec_from_file_location("topological_overlay_renderer", OVERLAY_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot import renderer from {OVERLAY_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_config(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def selected_cases(config: dict[str, Any], only: set[str]) -> list[dict[str, Any]]:
    cases = config.get("cases") or []
    if not only:
        return cases
    selected = [
        case
        for case in cases
        if case.get("name") in only
        or Path(case.get("path", "")).name in only
        or Path(case.get("path", "")).stem in only
    ]
    matched = {
        case.get("name")
        for case in selected
    } | {
        Path(case.get("path", "")).name
        for case in selected
    } | {
        Path(case.get("path", "")).stem
        for case in selected
    }
    missing = only - matched
    if missing:
        raise SystemExit(f"unknown --only case(s): {', '.join(sorted(missing))}")
    return selected


def render_args(defaults: dict[str, Any], case: dict[str, Any], out_dir: Path) -> SimpleNamespace:
    merged = {**defaults, **{k: v for k, v in case.items() if k in RENDER_FIELDS}}
    missing = sorted(RENDER_FIELDS - merged.keys())
    if missing:
        raise SystemExit(f"case {case.get('name', case.get('path'))}: missing fields {missing}")
    return SimpleNamespace(
        **merged,
        variant_name=None,
        out_dir=out_dir,
        manifest_name="manifest.json",
    )


def render(config_path: Path, out_dir: Path, only: set[str]) -> dict[str, Any]:
    config = load_config(config_path)
    defaults = config.get("defaults") or {}
    cases = selected_cases(config, only)
    renderer = load_renderer()
    out_dir.mkdir(parents=True, exist_ok=True)

    rows: list[dict[str, Any]] = []
    for case in cases:
        path = REPO_ROOT / case["path"]
        if not path.exists():
            raise SystemExit(f"input image does not exist: {path}")
        args = render_args(defaults, case, out_dir)
        row = renderer.render_image(path, out_dir, args)
        row["case_name"] = case.get("name", path.stem)
        row["note"] = case.get("note")
        row["case_params"] = {field: getattr(args, field) for field in sorted(RENDER_FIELDS)}
        rows.append(row)
        print(
            f"{row['case_name']}: corners={row['corner_count']} "
            f"labelled={row['labelled_count']} -> {row['output_dir']}"
        )

    manifest = {
        "schema": 1,
        "config": str(config_path),
        "out_dir": str(out_dir),
        "description": config.get("description"),
        "images": rows,
    }
    manifest_path = out_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"wrote manifest -> {manifest_path}")
    return manifest


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument(
        "--only",
        nargs="*",
        default=None,
        help="Optional case names, image stems, or filenames from the manifest.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    render(args.config, args.out_dir, set(args.only or []))


if __name__ == "__main__":
    main()
