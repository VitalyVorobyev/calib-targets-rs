#!/usr/bin/env python3
"""
Thin wrapper around the Rust `charuco_investigate` example.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT_ROOT = REPO_ROOT / "tmpdata" / "charuco_investigate"


def default_out_dir(mode: str, image: str | None) -> Path:
    if mode == "dataset":
        return DEFAULT_OUT_ROOT / "dataset"
    if image is None:
        return DEFAULT_OUT_ROOT
    stem = Path(image).stem
    if mode == "perf":
        return DEFAULT_OUT_ROOT / f"{stem}_perf"
    return DEFAULT_OUT_ROOT / stem


def cargo_command(args: argparse.Namespace, out_dir: Path) -> list[str]:
    cmd = [
        "cargo",
        "run",
        "-p",
        "calib-targets-charuco",
        "--example",
        "charuco_investigate",
        "--",
        args.mode,
    ]
    if args.input_dir:
        cmd.extend(["--input-dir", str(args.input_dir)])
    cmd.extend(["--out-dir", str(out_dir)])
    if args.image:
        cmd.extend(["--image", args.image])
    if args.strip is not None:
        cmd.extend(["--strip", str(args.strip)])
    if args.repeat is not None:
        cmd.extend(["--repeat", str(args.repeat)])
    return cmd


def load_summary(summary_path: Path) -> dict:
    return json.loads(summary_path.read_text())


def iter_reports(args: argparse.Namespace, out_dir: Path) -> list[Path]:
    if args.mode == "perf":
        if args.strip is None:
            raise SystemExit("--strip is required for perf overlays")
        report = out_dir / f"strip_{args.strip}" / "report.json"
        return [report] if report.exists() else []

    summary = load_summary(out_dir / "summary.json")
    strips = summary.get("strips") or []
    reports = []
    for strip in strips:
        report_path = Path(strip["report_path"])
        if args.overlay_failures and strip.get("passes_all", False):
            continue
        if args.overlay_one and args.strip is not None and strip.get("strip_index") != args.strip:
            continue
        reports.append(report_path)
    if args.overlay_one and args.strip is None and reports:
        return [reports[0]]
    return reports


def render_overlays(args: argparse.Namespace, out_dir: Path) -> None:
    if not (args.overlay_one or args.overlay_all or args.overlay_failures):
        return
    if importlib.util.find_spec("matplotlib") is None:
        print(
            "skipping overlay rendering: matplotlib is not installed",
            file=sys.stderr,
        )
        return
    reports = iter_reports(args, out_dir)
    if not reports:
        print("no reports selected for overlay rendering")
        return
    script = REPO_ROOT / "tools" / "plot_charuco_overlay.py"
    for report in reports:
        try:
            subprocess.run(
                [sys.executable, str(script), str(report)],
                cwd=REPO_ROOT,
                check=True,
            )
        except subprocess.CalledProcessError as err:
            print(
                f"warning: overlay rendering failed for {report}: {err}",
                file=sys.stderr,
            )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["single", "perf", "dataset"])
    parser.add_argument("--input-dir", type=Path)
    parser.add_argument("--out-dir", type=Path)
    parser.add_argument("--image")
    parser.add_argument("--strip", type=int)
    parser.add_argument("--repeat", type=int)
    parser.add_argument("--overlay-one", action="store_true")
    parser.add_argument("--overlay-all", action="store_true")
    parser.add_argument("--overlay-failures", action="store_true")
    args = parser.parse_args()

    overlay_flags = sum(
        int(flag)
        for flag in (args.overlay_one, args.overlay_all, args.overlay_failures)
    )
    if overlay_flags > 1:
        raise SystemExit("choose only one overlay mode")

    out_dir = args.out_dir or default_out_dir(args.mode, args.image)
    cmd = cargo_command(args, out_dir)
    subprocess.run(cmd, cwd=REPO_ROOT, check=True)
    render_overlays(args, out_dir)


if __name__ == "__main__":
    main()
