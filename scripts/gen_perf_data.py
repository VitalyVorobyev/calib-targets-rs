#!/usr/bin/env python3
"""Merge freshly measured PUBLIC perf numbers into the committed report data.

Reads the raw output produced by scripts/gen-perf-data.sh (the
`full_stage_timing` JSON for the four public report images) and refreshes ONLY
the numeric fields of .github/pages/performance/data.json. The editorial
content (per-card label, note, kind, file, img, the end_to_end prose) is
preserved.

Public data only — never read or emit private-dataset numbers.

Usage: python3 scripts/gen_perf_data.py <raw_dir> <data_json_path>
"""
import json
import os
import re
import sys
from datetime import datetime, timezone


def load(path):
    with open(path) as f:
        return json.load(f)


def full_by_image(raw_dir):
    """Return ({basename: image_obj}, metadata) for the full_stage_timing run."""
    path = os.path.join(raw_dir, "full.json")
    if not os.path.exists(path):
        return {}, None
    doc = load(path)
    by_image = {os.path.basename(img["image"]): img for img in doc.get("images", [])}
    return by_image, doc.get("metadata")


def p50(stat):
    """p50_ms of a {p50_ms, mean_ms} stat object, rounded; None passthrough."""
    if stat is None:
        return None
    return round(stat["p50_ms"], 3)


def merge_comparison(raw_dir, data):
    """Write the OpenCV comparison block from <raw_dir>/comparison.json, if
    present. Guarded: a missing file leaves any existing block untouched (never
    blanks it), so the pure-Rust refresh and the opencv refresh stay decoupled."""
    path = os.path.join(raw_dir, "comparison.json")
    if not os.path.exists(path):
        return
    comparison = load(path)
    if not comparison.get("groups"):
        print(f"WARNING: {path} has no groups — leaving comparison block as-is.")
        return
    data["comparison"] = comparison


def main():
    raw_dir, data_path = sys.argv[1], sys.argv[2]
    data = load(data_path)

    measured, meta_src = full_by_image(raw_dir)

    # ---- meta (from the full_stage_timing run) ----
    if meta_src:
        data["meta"]["cpu"] = meta_src.get("cpu", data["meta"]["cpu"])
        rv = meta_src.get("rustc", "")
        rm = re.search(r"\b(\d+\.\d+\.\d+)\b", rv)
        if rm:
            data["meta"]["rustc"] = rm.group(1)
        if meta_src.get("git_sha"):
            data["meta"]["git_sha"] = meta_src["git_sha"]
        data["meta"]["repeats"] = meta_src.get("repeats", data["meta"]["repeats"])
        data["meta"]["warmup"] = meta_src.get("warmup", data["meta"]["warmup"])
    data["meta"]["generated"] = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    # The per-image and end_to_end refresh is DESTRUCTIVE (it rebuilds the
    # frames list), so it runs only when full.json was actually measured.
    # A comparison-only refresh (gen-comparison-data.sh) leaves them untouched.
    if measured:
        # ---- per-image cards: refresh measured numbers, keep editorial fields --
        for card in data.get("images", []):
            base = os.path.basename(card["file"])
            m = measured.get(base)
            if not m:
                continue
            card["kind"] = m.get("kind", card.get("kind"))
            card["width"] = m.get("width", card.get("width"))
            card["height"] = m.get("height", card.get("height"))
            card["raw_corners"] = m.get("raw_corners", card.get("raw_corners"))
            card["labelled"] = m.get("labelled", card.get("labelled"))
            card["markers"] = m.get("markers")  # null for chessboard cards
            card["corner_detection_ms"] = p50(m.get("corner_detection"))
            card["grid_build_ms"] = p50(m.get("grid_build"))
            card["decode_ms"] = p50(m.get("decode"))  # null for chessboard cards

        # ---- end-to-end table: per-frame total = sum of measured stages ----
        # Driven by the same full_stage_timing measurements as the cards, so the
        # table can never drift from the per-stage breakdown above.
        frames = []
        for card in data.get("images", []):
            m = measured.get(os.path.basename(card["file"]))
            if not m:
                continue
            total = 0.0
            for stage in ("corner_detection", "grid_build", "decode"):
                s = m.get(stage)
                if s:
                    total += s["p50_ms"]
            kind = m.get("kind", card.get("kind", ""))
            frames.append({
                "file": card["file"],
                "ms": round(total, 3),
                "note": f"{m.get('width')}x{m.get('height')} {kind}",
            })
        frames.sort(key=lambda f: f["ms"])
        data.setdefault("end_to_end", {})["frames"] = frames

    # ---- OpenCV comparison block (independent, partial-safe) ----------------
    merge_comparison(raw_dir, data)

    with open(data_path, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    print(f"Updated {data_path}")


if __name__ == "__main__":
    main()
