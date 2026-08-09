#!/usr/bin/env python3
"""Evaluate exact generic/final checkpoints against manually reviewed grid truth."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any, Callable

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from PIL import Image


D4: tuple[Callable[[int, int], tuple[int, int]], ...] = (
    lambda u, v: (u, v),
    lambda u, v: (-u, v),
    lambda u, v: (u, -v),
    lambda u, v: (-u, -v),
    lambda u, v: (v, u),
    lambda u, v: (-v, u),
    lambda u, v: (v, -u),
    lambda u, v: (-v, -u),
)


def pixel_matches(entries: list[dict[str, Any]], truth: list[dict[str, Any]], tolerance: float) -> list[tuple[int, int, float]]:
    candidates: list[tuple[float, int, int]] = []
    for entry_index, entry in enumerate(entries):
        x, y = entry["position"]
        for truth_index, point in enumerate(truth):
            distance = math.hypot(x - point["position"][0], y - point["position"][1])
            if distance <= tolerance:
                candidates.append((distance, entry_index, truth_index))
    used_entries: set[int] = set()
    used_truth: set[int] = set()
    matches: list[tuple[int, int, float]] = []
    for distance, entry_index, truth_index in sorted(candidates):
        if entry_index not in used_entries and truth_index not in used_truth:
            used_entries.add(entry_index)
            used_truth.add(truth_index)
            matches.append((entry_index, truth_index, distance))
    return matches


def best_alignment(entries: list[dict[str, Any]], truth: list[dict[str, Any]], matches: list[tuple[int, int, float]]) -> dict[str, Any]:
    best: tuple[tuple[int, int, int, int, int], int, int, int] | None = None
    for transform_index, transform in enumerate(D4):
        translations = {
            (
                truth[truth_index]["u"] - transform(entries[entry_index]["u"], entries[entry_index]["v"])[0],
                truth[truth_index]["v"] - transform(entries[entry_index]["u"], entries[entry_index]["v"])[1],
            )
            for entry_index, truth_index, _ in matches
        }
        for du, dv in translations:
            correct = sum(
                tuple(a + b for a, b in zip(transform(entries[entry_index]["u"], entries[entry_index]["v"]), (du, dv)))
                == (truth[truth_index]["u"], truth[truth_index]["v"])
                for entry_index, truth_index, _ in matches
            )
            rank = (correct, -transform_index, -abs(du) - abs(dv), -du, -dv)
            if best is None or rank > best[0]:
                best = (rank, transform_index, du, dv)
    if best is None:
        return {"d4": 0, "translation": [0, 0], "correct": 0, "wrong": 0}
    rank, transform_index, du, dv = best
    return {
        "d4": transform_index,
        "translation": [du, dv],
        "correct": rank[0],
        "wrong": len(matches) - rank[0],
    }


def evaluate(components: list[list[dict[str, Any]]], truth: dict[str, Any], tolerance: float) -> dict[str, Any]:
    visible = [point for point in truth["points"] if point["status"] == "visible"]
    labelled = correct = wrong = false_labelled = holes = 0
    matched_truth: set[int] = set()
    max_distance = 0.0
    alignments = []
    for index, entries in enumerate(components):
        labelled += len(entries)
        if entries:
            coordinates = {(entry["u"], entry["v"]) for entry in entries}
            us, vs = zip(*coordinates)
            holes += (max(us) - min(us) + 1) * (max(vs) - min(vs) + 1) - len(coordinates)
        matches = pixel_matches(entries, visible, tolerance)
        alignment = best_alignment(entries, visible, matches)
        alignments.append({"component": index, **alignment})
        correct += alignment["correct"]
        wrong += alignment["wrong"]
        false_labelled += len(entries) - len(matches)
        for _, truth_index, distance in matches:
            matched_truth.add(truth_index)
            max_distance = max(max_distance, distance)
    primary_alignment = alignments[0] if alignments else None
    return {
        "labelled": labelled,
        "correct_labels": correct,
        "wrong_labels": wrong,
        "false_labelled_features": false_labelled,
        "missed_visible_corners": len(visible) - len(matched_truth),
        "visible_ground_truth": len(visible),
        "precision": correct / labelled if labelled else 0.0,
        "recall": correct / len(visible) if visible else 1.0,
        "holes_within_component_bboxes": holes,
        "components": len(components),
        "max_pixel_match_distance": max_distance,
        "component_alignments": alignments,
        "canonical_primary": bool(
            primary_alignment
            and primary_alignment["d4"] == 0
            and primary_alignment["translation"] == [0, 0]
        ),
    }


def native_position(position: list[float], detection_scale: float) -> list[float]:
    return [position[0] / detection_scale, position[1] / detection_scale]


def generic_components(payload: dict[str, Any], detection_scale: float) -> list[list[dict[str, Any]]]:
    positions = {
        int(corner["index"]): native_position(corner["position"], detection_scale)
        for corner in payload["corners"]
    }
    return [
        [
            {
                "u": label["u"],
                "v": label["v"],
                "source_index": label["feature_index"],
                "position": positions[int(label["feature_index"])],
            }
            for label in component["labels"]
        ]
        for component in payload["trace"]["final_components"]
    ]


def recovered_components(payload: dict[str, Any], detection_scale: float) -> list[list[dict[str, Any]]]:
    positions = {
        int(corner["index"]): native_position(corner["position"], detection_scale)
        for corner in payload["corners"]
    }
    return [
        [
            {
                "u": label["u"],
                "v": label["v"],
                "source_index": label["corner_index"],
                "position": positions[int(label["corner_index"])],
            }
            for label in component
        ]
        for component in payload["chessboard_stages"]["recovered_components"]
    ]


def final_components(payload: dict[str, Any], detection_scale: float) -> list[list[dict[str, Any]]]:
    return [
        [
            {
                "u": corner["grid"]["u"],
                "v": corner["grid"]["v"],
                "source_index": corner["input_index"],
                "position": native_position(corner["position"], detection_scale),
            }
            for corner in detection["corners"]
        ]
        for detection in payload.get("detections", [])
    ]


def source_set(components: list[list[dict[str, Any]]]) -> set[int]:
    return {entry["source_index"] for component in components for entry in component}


def draw_ground_truth(image_path: Path, truth: dict[str, Any], out_path: Path) -> None:
    image = Image.open(image_path).convert("L")
    figure, axis = plt.subplots(figsize=(12, 12 * image.height / image.width), dpi=140)
    axis.imshow(image, cmap="gray", vmin=0, vmax=255)
    for point in truth["points"]:
        x, y = point["position"]
        visible = point["status"] == "visible"
        color = "#00d084" if visible else "#ff3b30"
        axis.scatter([x], [y], s=20, facecolors=color if visible else "none", edgecolors=color, linewidths=0.8)
        axis.text(x + 2, y - 2, f'{point["u"]},{point["v"]}', color="white", fontsize=4)
    axis.axis("off")
    figure.tight_layout(pad=0.02)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(out_path, bbox_inches="tight", pad_inches=0.01)
    plt.close(figure)


def draw_contact_sheet(image_root: Path, truths: list[dict[str, Any]], out_path: Path) -> None:
    figure, axes = plt.subplots(2, 2, figsize=(16, 10), dpi=140)
    for axis, truth in zip(axes.flat, truths):
        image = Image.open(image_root / truth["image"]).convert("L")
        axis.imshow(image, cmap="gray", vmin=0, vmax=255)
        for point in truth["points"]:
            color = "#00d084" if point["status"] == "visible" else "#ff3b30"
            axis.scatter([point["position"][0]], [point["position"][1]], s=9, facecolors=color, edgecolors=color, linewidths=0.6)
        visible = sum(point["status"] == "visible" for point in truth["points"])
        axis.set_title(f'{truth["image"]} — visible {visible}, excluded {len(truth["points"]) - visible}', fontsize=9)
        axis.axis("off")
    figure.tight_layout(pad=0.6)
    figure.savefig(out_path, bbox_inches="tight", pad_inches=0.05)
    plt.close(figure)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace-dir", type=Path, required=True)
    parser.add_argument("--image-root", type=Path, required=True)
    parser.add_argument("--ground-truth", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--tolerance-px", type=float, default=3.0)
    parser.add_argument(
        "--detection-scale",
        type=float,
        default=1.0,
        help="Scale applied before detection; coordinates are divided by it for native-resolution scoring.",
    )
    args = parser.parse_args()
    if not math.isfinite(args.detection_scale) or args.detection_scale <= 0.0:
        parser.error("--detection-scale must be finite and positive")
    ground_truth = json.loads(args.ground_truth.read_text(encoding="utf-8"))
    image_root = args.image_root
    reports = []
    for truth in ground_truth["images"]:
        image_path = image_root / truth["image"]
        image_bytes = image_path.read_bytes()
        actual_sha256 = hashlib.sha256(image_bytes).hexdigest()
        if actual_sha256 != truth["image_sha256"]:
            raise SystemExit(
                f'image hash mismatch for {truth["image"]}: '
                f'expected {truth["image_sha256"]}, got {actual_sha256}'
            )
        with Image.open(image_path) as image:
            actual_size = [image.width, image.height]
        if actual_size != truth["native_size"]:
            raise SystemExit(
                f'image size mismatch for {truth["image"]}: '
                f'expected {truth["native_size"]}, got {actual_size}'
            )
        payload = json.loads((args.trace_dir / Path(truth["image"]).stem / "trace.json").read_text(encoding="utf-8"))
        generic = generic_components(payload, args.detection_scale)
        recovered = recovered_components(payload, args.detection_scale)
        final = final_components(payload, args.detection_scale)
        generic_sources, recovered_sources, final_sources = map(source_set, (generic, recovered, final))
        reported_additions = set(payload["chessboard_stages"]["recovery_additions"])
        reported_drops = set(payload["chessboard_stages"]["final_drops"])
        assert reported_additions == recovered_sources - generic_sources
        assert reported_drops == recovered_sources - final_sources
        generic_fits = [component.get("fit") for component in payload["trace"]["final_components"]]
        reports.append({
            "image": truth["image"],
            "generic_projective_grid": evaluate(generic, truth, args.tolerance_px),
            "after_chessboard_recovery": evaluate(recovered, truth, args.tolerance_px),
            "final_public": evaluate(final, truth, args.tolerance_px),
            "stage_attribution": {
                "recovery_additions": sorted(reported_additions),
                "final_drops_or_refusal": sorted(reported_drops),
            },
            "generic_fit_residuals": generic_fits,
            "final_fit_residuals": {
                "available": False,
                "reason": "the chessboard final gate is local-geometry based and does not perform a second global projective refit",
            },
        })
        draw_ground_truth(
            image_root / truth["image"],
            truth,
            args.trace_dir / Path(truth["image"]).stem / "ground-truth.png",
        )
    measured_stages = ("generic_projective_grid", "after_chessboard_recovery", "final_public")
    wrong = sum(item[stage]["wrong_labels"] for item in reports for stage in measured_stages)
    canonical_failures = [
        item["image"]
        for item in reports
        if not item["final_public"]["canonical_primary"]
    ]
    report = {
        "schema": 2,
        "coordinate_frame": "native image pixels: origin top-left, x right, y down; ground-truth grid u right, v down",
        "detection_scale": args.detection_scale,
        "matching": f"one-to-one native-resolution pixel matching within {args.tolerance_px}px, then best D4 + integer translation per component",
        "acceptance": {
            "wrong_labels_must_equal": 0,
            "observed_wrong_labels": wrong,
            "final_canonical_primary_required": True,
            "canonical_failures": canonical_failures,
            "passed": wrong == 0 and not canonical_failures,
        },
        "images": reports,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2), encoding="utf-8")
    draw_contact_sheet(image_root, ground_truth["images"], args.trace_dir / "ground-truth-contact-sheet.png")
    if wrong or canonical_failures:
        raise SystemExit(
            f"quality gate failed: {wrong} wrong labels, "
            f"canonical failures={canonical_failures}"
        )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
