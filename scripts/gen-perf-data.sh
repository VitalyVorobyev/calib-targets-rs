#!/usr/bin/env bash
# Regenerate the PUBLIC performance-report data consumed by
# .github/pages/performance/index.html.
#
# Uses PUBLIC testdata images ONLY — it deliberately does NOT call `bench run`
# (whose datasets.toml may reference private dirs) and never touches
# privatedata/. The output (.github/pages/performance/data.json) is committed
# and published, so everything here must stay public.
#
# It refreshes only the measured numbers in data.json (per-stage timings + the
# raw/labelled/marker counts for the four report images); the editorial fields
# (labels, notes, end_to_end prose) are preserved by scripts/gen_perf_data.py.
# It ALSO regenerates the committed report previews under
# .github/pages/performance/img/ as detection overlays (corners + grid + decoded
# marker quads) via `full_stage_timing --overlay-dir`.
#
# Usage:
#   bash scripts/gen-perf-data.sh
#   REPEATS=200 WARMUP=20 bash scripts/gen-perf-data.sh
#
# `set -e` (plus the pre-merge guard below) is load-bearing: a failed
# `cargo run` must abort BEFORE the merge step, otherwise gen_perf_data.py
# treats the missing raw file as "no measurement" and would happily rewrite
# the published data.json with stale/partial numbers while exiting
# successfully.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPEATS="${REPEATS:-100}"
WARMUP="${WARMUP:-10}"
RAW="$(mktemp -d)"
trap 'rm -rf "$RAW"' EXIT

log() { printf '\n==== %s ====\n' "$*"; }

# Every raw file the merger expects; checked for existence + non-emptiness
# before we touch the published data.json.
expected=()

# ---- 1. Full-detector per-stage timing on the four PUBLIC report images ----
# `full_stage_timing` hard-codes the four images
# (small/mid/large/author_like_oblique) and runs the COMPLETE detector for
# each: ChArUco for small+large, plain chessboard for mid, PuzzleBoard for
# author_like_oblique. It emits corner_detection / grid_build / decode p50s
# plus raw/labelled/marker counts.
out="$RAW/full.json"
log "full_stage_timing (four public report images + detection overlays)"
# `--overlay-dir` refreshes the committed report previews as DETECTION OVERLAYS
# (grid corners + edges + decoded ArUco marker quads; large.png at half size).
# Those PNGs are committed published assets, like the data.json this feeds.
cargo run --release -q -p calib-targets-bench --bin full_stage_timing -- \
  --repeats "$REPEATS" --warmup "$WARMUP" \
  --out "$out" \
  --overlay-dir .github/pages/performance/img
expected+=("$out")

# ---- 2. Guard: never merge unless every measurement produced output --------
# `set -e` already aborts on a non-zero cargo exit; this also catches a command
# that exits 0 but wrote nothing, so a stale/partial data.json can never ship.
for f in "${expected[@]}"; do
  if [[ ! -s "$f" ]]; then
    echo "ERROR: missing or empty measurement: $f — aborting before merge." >&2
    exit 1
  fi
done

# ---- 3. Merge measured numbers into data.json (editorial preserved) --------
log "merge -> .github/pages/performance/data.json"
python3 scripts/gen_perf_data.py "$RAW" .github/pages/performance/data.json

echo "Done. Review the diff in .github/pages/performance/data.json before committing."
