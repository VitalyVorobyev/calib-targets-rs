#!/usr/bin/env bash
# Refresh the OpenCV-baseline comparison block in the PUBLIC performance report
# (.github/pages/performance/data.json → "comparison", plus img/cmp_*.png).
#
# Kept SEPARATE from gen-perf-data.sh because it needs OpenCV
# (opencv-python-headless) in the calib-targets Python binding venv — the
# pure-Rust per-stage refresh deliberately stays opencv-free.
#
# Public images only (testdata/small.png + testdata/mid.png). Run
# gen-perf-data.sh FIRST: the comparison reads the committed data.json for the
# native (Rust) "ours" runtime, so that must be fresh before this runs.
#
# Usage:
#   bash scripts/gen-comparison-data.sh
#   REPEATS=40 WARMUP=5 bash scripts/gen-comparison-data.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VENV="crates/calib-targets-py/.venv"
PY="$VENV/bin/python"
REPEATS="${REPEATS:-40}"
WARMUP="${WARMUP:-5}"
RAW="$(mktemp -d)"
trap 'rm -rf "$RAW"' EXIT

if [ ! -x "$PY" ]; then
  echo "ERROR: binding venv not found at $VENV — build it with:" >&2
  echo "  uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml" >&2
  exit 1
fi

# OpenCV + deps are required; install on demand (idempotent).
if ! "$PY" -c "import cv2, numpy, PIL" >/dev/null 2>&1; then
  echo "==== installing opencv-python-headless into the binding venv ===="
  uv pip install --python "$PY" opencv-python-headless numpy pillow
fi

echo "==== compare_opencv_baseline (small.png ChArUco + mid.png chessboard) ===="
"$PY" tools/compare_opencv_baseline.py \
  --out "$RAW/comparison.json" \
  --img-dir .github/pages/performance/img \
  --repeats "$REPEATS" --warmup "$WARMUP"

if [ ! -s "$RAW/comparison.json" ]; then
  echo "ERROR: comparison.json missing/empty — aborting before merge." >&2
  exit 1
fi

echo "==== merge -> .github/pages/performance/data.json ===="
python3 scripts/gen_perf_data.py "$RAW" .github/pages/performance/data.json

echo "Done. Review the diff in data.json + the new img/cmp_*.png overlays."
