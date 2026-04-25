#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Building WASM package..."
wasm-pack build "$ROOT_DIR/crates/calib-targets-wasm" \
  --target web \
  --release \
  --out-dir "$ROOT_DIR/demo/pkg" \
  --out-name calib_targets_wasm

cp "$ROOT_DIR/crates/calib-targets-wasm/README.md" "$ROOT_DIR/demo/pkg/README.md"

# Append hand-written object-shape types to the auto-generated .d.ts so
# consumers of the npm package get typed result/parameter shapes (the
# wasm-bindgen output only types function signatures).
cat "$ROOT_DIR/crates/calib-targets-wasm/typescript-extras.d.ts" \
  >> "$ROOT_DIR/demo/pkg/calib_targets_wasm.d.ts"

# Override the published npm name (wasm-pack derives it from the Rust crate
# name; we ship as the scoped public package @vitavision/calib-targets).
(cd "$ROOT_DIR/demo/pkg" && npm pkg set name=@vitavision/calib-targets)

echo "WASM package built to demo/pkg/"
ls -lh "$ROOT_DIR/demo/pkg/"*.wasm
