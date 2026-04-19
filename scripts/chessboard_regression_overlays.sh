#!/usr/bin/env bash
# Chessboard-v2 regression inspection driver.
#
# Runs the v2 detector on every image in the curated testdata set,
# emits per-image CompactFrame JSONs for both the default config and
# the 3-config sweep best, renders overlays, and writes a summary TSV.
# Everything lands under bench_results/chessboard_regression/
# (already in .gitignore).
#
# Usage:
#   scripts/chessboard_regression_overlays.sh
#
# No arguments. Re-running overwrites prior outputs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$ROOT_DIR"

OUT_ROOT="bench_results/chessboard_regression"
JSON_DEFAULT_DIR="$OUT_ROOT/default"
JSON_SWEEP_DIR="$OUT_ROOT/sweep"
PNG_DEFAULT_DIR="$OUT_ROOT/default_png"
PNG_SWEEP_DIR="$OUT_ROOT/sweep_png"
SUMMARY_TSV="$OUT_ROOT/_summary.tsv"

mkdir -p "$JSON_DEFAULT_DIR" "$JSON_SWEEP_DIR" "$PNG_DEFAULT_DIR" "$PNG_SWEEP_DIR"

# Curated image set — 19 images per the Phase 1 plan.
IMAGES=(
  "testdata/mid.png"
  "testdata/small.png"
  "testdata/large.png"
  "testdata/small0.png"
  "testdata/small1.png"
  "testdata/small2.png"
  "testdata/small3.png"
  "testdata/small4.png"
  "testdata/small5.png"
  "testdata/puzzleboard_reference/example0.png"
  "testdata/puzzleboard_reference/example1.png"
  "testdata/puzzleboard_reference/example2.png"
  "testdata/puzzleboard_reference/example3.png"
  "testdata/puzzleboard_reference/example4.png"
  "testdata/puzzleboard_reference/example5.png"
  "testdata/puzzleboard_reference/example6.png"
  "testdata/puzzleboard_reference/example7.png"
  "testdata/puzzleboard_reference/example8.png"
  "testdata/puzzleboard_reference/example9.png"
)

# Build the Rust example once.
echo "[1/3] Building debug_single..."
cargo build --release -p calib-targets-chessboard --example debug_single --features dataset

BIN="target/release/examples/debug_single"

# Slug helper: "testdata/puzzleboard_reference/example4.png" -> "puzzleboard_reference_example4"
slugify() {
  local p="$1"
  local stem
  stem="${p#testdata/}"
  stem="${stem%.png}"
  printf '%s' "${stem//\//_}"
}

# Collect TSV rows (image, config, det, labelled, blacklisted, components, input_corners).
: > "$SUMMARY_TSV"
printf 'image\tconfig\tdet\tlabelled\tblacklisted\tcomponents\tinput_corners\n' >> "$SUMMARY_TSV"

echo "[2/3] Running detection on ${#IMAGES[@]} images..."
for img in "${IMAGES[@]}"; do
  if [[ ! -f "$img" ]]; then
    echo "  [skip] missing $img"
    continue
  fi
  slug="$(slugify "$img")"
  json_default="$JSON_DEFAULT_DIR/${slug}.json"
  json_sweep="$JSON_SWEEP_DIR/${slug}.json"
  # Runner prints one TSV row per config on stdout. Capture both rows
  # and append to summary without the redundant leading column shuffle —
  # the runner already emits `<path>\t<config>\tdet=...` which we
  # normalise to clean columns here.
  "$BIN" --image "$img" --out-default "$json_default" --out-sweep "$json_sweep" \
    | awk -F'\t' 'BEGIN{OFS="\t"} {
        det=""; lab=""; bl=""; comp=""; inp="";
        for(i=3;i<=NF;i++){
          split($i,kv,"=");
          if(kv[1]=="det") det=kv[2];
          else if(kv[1]=="labelled") lab=kv[2];
          else if(kv[1]=="blacklisted") bl=kv[2];
          else if(kv[1]=="components") comp=kv[2];
          else if(kv[1]=="input") inp=kv[2];
        }
        print $1, $2, det, lab, bl, comp, inp
      }' >> "$SUMMARY_TSV"
done

echo "[3/3] Rendering overlays..."
render_count=0
for img in "${IMAGES[@]}"; do
  if [[ ! -f "$img" ]]; then
    continue
  fi
  slug="$(slugify "$img")"
  json_default="$JSON_DEFAULT_DIR/${slug}.json"
  json_sweep="$JSON_SWEEP_DIR/${slug}.json"
  png_default="$PNG_DEFAULT_DIR/${slug}_default.png"
  png_sweep="$PNG_SWEEP_DIR/${slug}_sweep.png"
  if [[ -f "$json_default" ]]; then
    uv run python crates/calib-targets-py/examples/overlay_chessboard_v2.py \
      --single-image "$img" --frame-json "$json_default" --out "$png_default" --tag default \
      >/dev/null
    render_count=$((render_count + 1))
  fi
  if [[ -f "$json_sweep" ]]; then
    uv run python crates/calib-targets-py/examples/overlay_chessboard_v2.py \
      --single-image "$img" --frame-json "$json_sweep" --out "$png_sweep" --tag sweep \
      >/dev/null
    render_count=$((render_count + 1))
  fi
done

echo
echo "done. $render_count overlays rendered."
echo "  summary:  $SUMMARY_TSV"
echo "  default:  $PNG_DEFAULT_DIR/*.png"
echo "  sweep:    $PNG_SWEEP_DIR/*.png"
column -t -s $'\t' "$SUMMARY_TSV"
