#!/usr/bin/env bash
# Regenerate the PUBLIC performance-report data consumed by
# .github/pages/performance/index.html.
#
# Uses PUBLIC testdata images + synthetic fixtures ONLY — it deliberately does
# NOT call `bench run` (whose datasets.toml may reference private dirs) and
# never touches privatedata/. The output (.github/pages/performance/data.json)
# is committed and published, so everything here must stay public.
#
# It refreshes only the measured numbers in data.json (per-stage timings + the
# synthetic decode sweep); the editorial fields (labels, notes, roadmap) are
# preserved by scripts/gen_perf_data.py.
#
# Usage:
#   bash scripts/gen-perf-data.sh
#   REPEATS=200 WARMUP=20 bash scripts/gen-perf-data.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPEATS="${REPEATS:-100}"
WARMUP="${WARMUP:-10}"
RAW="$(mktemp -d)"
trap 'rm -rf "$RAW"' EXIT

log() { printf '\n==== %s ====\n' "$*"; }

# ---- 1. Per-stage breakdown on public image dirs (ring-fit + disk-fit) ----
# slug:dir pairs — keep to PUBLIC testdata directories only.
for pair in "chess:testdata/02-topo-grid" "puzzle:testdata/puzzleboard_reference"; do
  slug="${pair%%:*}"; dir="${pair##*:}"
  for om in ring-fit disk-fit; do
    omslug="${om/-/_}"
    log "topo_stage_timing $dir ($om)"
    cargo run --release -q -p calib-targets-bench --bin topo_stage_timing -- \
      --image-dir "$dir" --orientation-method "$om" \
      --repeats "$REPEATS" --warmup "$WARMUP" \
      --out "$RAW/topo.$slug.$omslug.json"
  done
done

# ---- 2. Synthetic PuzzleBoard decode sweep (public, ungated) --------------
log "cargo bench puzzleboard_sizes (synthetic full master sweep)"
cargo bench -p calib-targets --bench puzzleboard_sizes 2>&1 | tee "$RAW/sweep.txt"

# ---- 3. Merge measured numbers into data.json (editorial preserved) -------
log "merge -> .github/pages/performance/data.json"
python3 scripts/gen_perf_data.py "$RAW" .github/pages/performance/data.json

echo "Done. Review the diff in .github/pages/performance/data.json before committing."
