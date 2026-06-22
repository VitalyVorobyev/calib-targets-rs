#!/usr/bin/env python3
"""Merge freshly measured PUBLIC perf numbers into the committed report data.

Reads the raw outputs produced by scripts/gen-perf-data.sh (per-stage
topo_stage_timing JSONs + the synthetic puzzleboard_sizes criterion stdout) and
refreshes ONLY the numeric fields of .github/pages/performance/data.json. The
editorial content (labels, notes, kind, roadmap, end_to_end) is preserved.

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


def topo_by_image(raw_dir, slug, om):
    """Return {basename: image_obj} for one topo run, or {} if absent."""
    path = os.path.join(raw_dir, f"topo.{slug}.{om}.json")
    if not os.path.exists(path):
        return {}, None
    doc = load(path)
    return {img["image"]: img for img in doc.get("images", [])}, doc.get("metadata")


def p50(img, stage):
    s = img.get("summary", {}).get(stage)
    return round(s["p50_ms"], 3) if s else None


def src_slug(file_path):
    if "02-topo-grid" in file_path:
        return "chess"
    if "puzzleboard" in file_path:
        return "puzzle"
    return None


def parse_sweep(raw_dir):
    """Parse `puzzleboard/full/<N>  time: [lo med hi]` medians from sweep.txt."""
    path = os.path.join(raw_dir, "sweep.txt")
    if not os.path.exists(path):
        return {}
    out = {}
    pat = re.compile(
        r"puzzleboard/full/(\d+)\s+time:\s+\[\s*([\d.]+)\s*(\w+)\s+([\d.]+)\s*(\w+)"
    )
    unit = {"ns": 1e-6, "us": 1e-3, "ms": 1.0, "s": 1e3}
    with open(path) as f:
        for line in f:
            m = pat.search(line)
            if m:
                size = int(m.group(1))
                med = float(m.group(4)) * unit.get(m.group(5), 1.0)
                out[size] = round(med, 3)
    return out


def main():
    raw_dir, data_path = sys.argv[1], sys.argv[2]
    data = load(data_path)

    ring = {s: topo_by_image(raw_dir, s, "ring_fit") for s in ("chess", "puzzle")}
    disk = {s: topo_by_image(raw_dir, s, "disk_fit") for s in ("chess", "puzzle")}

    # ---- meta (from any available topo run metadata) ----
    meta_src = next((m for (_, m) in ring.values() if m), None)
    if meta_src:
        data["meta"]["cpu"] = meta_src.get("cpu", data["meta"]["cpu"])
        rv = meta_src.get("rustc", "")
        rm = re.search(r"\b(\d+\.\d+\.\d+)\b", rv)
        if rm:
            data["meta"]["rustc"] = rm.group(1)
        data["meta"]["git_sha"] = meta_src.get("git_sha", data["meta"]["git_sha"])
        data["meta"]["repeats"] = meta_src.get("repeats", data["meta"]["repeats"])
        data["meta"]["warmup"] = meta_src.get("warmup", data["meta"]["warmup"])
    data["meta"]["generated"] = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    # ---- per-image cards ----
    for card in data.get("images", []):
        slug = src_slug(card["file"])
        if slug is None:
            continue
        base = os.path.basename(card["file"])
        rimg = ring[slug][0].get(base)
        dimg = disk[slug][0].get(base)
        if rimg:
            card["corner_detection"]["ring_fit"] = p50(rimg, "corner_detection")
            card["grid_total"] = p50(rimg, "grid_total")
            card["width"] = rimg.get("width", card["width"])
            card["height"] = rimg.get("height", card["height"])
            card["raw_corners"] = rimg.get("raw_corners", card["raw_corners"])
            card["labelled"] = rimg.get("labelled_count", card["labelled"])
            for st in card.get("stages", []):
                v = p50(rimg, st["name"])
                if v is not None:
                    st["ms"] = v
        if dimg:
            card["corner_detection"]["disk_fit"] = p50(dimg, "corner_detection")

    # ---- synthetic decode sweep: refresh / add the "after" series ----
    sweep = parse_sweep(raw_dir)
    if sweep:
        sizes = data["sweep"]["sizes"]
        after = [sweep.get(sz) for sz in sizes]
        series = data["sweep"]["series"]
        existing = next((s for s in series if s["key"] == "full_after"), None)
        if existing:
            existing["ms"] = after
        else:
            series.append({
                "key": "full_after",
                "label": "full master sweep (after — O(8·501) scan)",
                "ms": after,
            })

    with open(data_path, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    print(f"Updated {data_path}")


if __name__ == "__main__":
    main()
