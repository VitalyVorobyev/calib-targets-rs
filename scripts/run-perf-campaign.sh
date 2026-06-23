#!/usr/bin/env bash
# Reproducible performance campaign for the calib-targets-rs grid + decode
# hot paths. Chains the four measurement layers and lands EVERY artifact under
# bench_results/perf-campaign/ (gitignored — never commit):
#
#   1. End-to-end p50/p95   — `bench run` over every datasets.toml image.
#   2. Per-stage breakdown  — `topo_stage_timing` (ring-fit + disk-fit) on the
#                             public 02-topo-grid set (14 tracing-span stages).
#   3. Criterion micro-bench— `cargo bench --workspace` (synthetic grid +
#                             chessboard/puzzleboard/charuco corners/decode split).
#   4. Flamegraphs          — samply on the topological pipeline + the puzzleboard
#                             and charuco decode benches (`--profile-time`).
#
# Private-dataset numbers land ONLY here. The committed
# docs/development/performance.md carries general / public-image numbers only
# (private-dataset disclosure policy).
#
# Usage:
#   bash scripts/run-perf-campaign.sh           # full campaign
#   FLAME=0 bash scripts/run-perf-campaign.sh   # skip flamegraph capture
#   REPEATS=200 WARMUP=20 bash scripts/run-perf-campaign.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="bench_results/perf-campaign"
REPEATS="${REPEATS:-100}"
WARMUP="${WARMUP:-10}"
FLAME="${FLAME:-1}"
TOPO_DIR="testdata/02-topo-grid"
mkdir -p "$OUT"

log() { printf '\n==== %s ====\n' "$*"; }

# ---- 0. Environment metadata -------------------------------------------
log "metadata"
{
  echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "git_sha: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "rustc: $(rustc --version 2>/dev/null || echo unknown)"
  echo "uname: $(uname -a)"
  command -v sysctl >/dev/null 2>&1 && echo "cpu: $(sysctl -n machdep.cpu.brand_string 2>/dev/null)"
  echo "repeats: $REPEATS  warmup: $WARMUP  flame: $FLAME"
} | tee "$OUT/metadata.txt"

# ---- 1. End-to-end latency (bench run, all images) ---------------------
log "end-to-end: bench run (every datasets.toml image, pipeline/ring-fit)"
cargo run --release -p calib-targets-bench --bin bench -- run \
  --engine pipeline --orientation-method ring-fit \
  2>&1 | tee "$OUT/bench-run.txt"
newest_report="$(ls -t bench_results/chessboard.*.json 2>/dev/null | head -1)"
[ -n "$newest_report" ] && cp -f "$newest_report" "$OUT/end-to-end-report.json"

# ---- 2. Per-stage breakdown (both orientation methods) -----------------
log "per-stage: topo_stage_timing on $TOPO_DIR (ring-fit + disk-fit)"
for om in ring-fit disk-fit; do
  slug="${om/-/_}"
  cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
    --image-dir "$TOPO_DIR" \
    --orientation-method "$om" \
    --repeats "$REPEATS" --warmup "$WARMUP" \
    --out "$OUT/topo-stage.02-topo-grid.$slug.json"
done

# ---- 3. Criterion micro-benches (whole workspace) ----------------------
log "criterion: cargo bench --workspace"
cargo bench --workspace 2>&1 | tee "$OUT/criterion.txt"
# The puzzleboard real-dataset decode bench is gated behind `dataset`, so
# --workspace skips it; run it explicitly and append.
log "criterion: puzzleboard dataset decode (--features dataset)"
cargo bench -p calib-targets-puzzleboard --bench dataset_decode --features dataset \
  2>&1 | tee -a "$OUT/criterion.txt"

# ---- 4. Flamegraphs (samply, optional) ---------------------------------
if [ "$FLAME" = "1" ] && command -v samply >/dev/null 2>&1; then
  log "flamegraphs: build profiling binaries"
  cargo build --profile profiling -p calib-targets-bench --bins

  # 4a. Topological pipeline — one clean [[bin]], high repeat count gives the
  #     profiler a multi-second steady-state window.
  log "flamegraph: topological pipeline"
  samply record --save-only --no-open -o "$OUT/flame.topological.json.gz" -- \
    ./target/profiling/topo_stage_timing \
    --image-dir "$TOPO_DIR" --repeats 400 --warmup 10 \
    --out "$OUT/topo-stage.flame-driver.json"

  # 4b/4c. Decode benches via criterion `--profile-time`. Both decode benches
  #     are named `dataset_decode`, so resolve the exact binary from cargo's
  #     JSON artifact stream rather than globbing target/profiling/deps.
  # crate bench filter outfile [extra cargo args...]
  flame_bench() {
    local crate="$1" bench="$2" filter="$3" outfile="$4"; shift 4
    local exe
    exe="$(cargo bench -p "$crate" --bench "$bench" --no-run --profile profiling \
      "$@" --message-format=json 2>/dev/null \
      | python3 -c 'import sys, json
name = sys.argv[1]
hit = ""
for line in sys.stdin:
    try:
        m = json.loads(line)
    except Exception:
        continue
    if m.get("executable") and m.get("target", {}).get("name") == name:
        hit = m["executable"]
print(hit)' "$bench")"
    if [ -n "$exe" ] && [ -x "$exe" ]; then
      samply record --save-only --no-open -o "$outfile" -- \
        "$exe" --bench --profile-time 12 "$filter"
    else
      echo "skip flamegraph $crate/$bench: bench binary not resolved (dataset absent?)"
    fi
  }

  # Puzzleboard 501² sweep — profile the PUBLIC, ungated puzzleboard_sizes
  # bench (its `full` path runs the full master sweep), so no private data and
  # no `dataset` feature needed.
  log "flamegraph: puzzleboard 501² sweep (public puzzleboard_sizes/full)"
  flame_bench calib-targets puzzleboard_sizes \
    'puzzleboard/full/30' "$OUT/flame.puzzleboard-sweep.json.gz"

  log "flamegraph: charuco decode (board-match attribution)"
  flame_bench calib-targets-charuco dataset_decode \
    'charuco/dataset/decode' "$OUT/flame.charuco-decode.json.gz"
else
  samply_state="$(command -v samply >/dev/null 2>&1 && echo present || echo absent)"
  log "flamegraphs: skipped (FLAME=$FLAME, samply $samply_state)"
fi

log "campaign complete — artifacts under $OUT/"
ls -la "$OUT/"
echo
echo "Open a flamegraph offline with:  samply load $OUT/flame.<name>.json.gz"
