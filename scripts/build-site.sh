#!/usr/bin/env bash
# Build the unified GitHub Pages tree locally:
#   public/                -> landing page (.github/pages/) + performance report
#   public/book/           -> mdBook (book chapters + embedded demo)
#   public/api/            -> cargo doc workspace API reference
#   public/demo/           -> built React + WASM demo
#
# Mirrors the layout produced by .github/workflows/docs.yml.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$ROOT_DIR"

echo "==> Building cargo doc..."
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
printf '<meta http-equiv="refresh" content="0; url=calib_targets_core/index.html">\n' \
  > target/doc/index.html

echo "==> Building mdBook..."
mdbook build book

echo "==> Building WASM + demo..."
./scripts/build-wasm.sh
(cd demo && bun install && bun run build)

echo "==> Assembling public/..."
rm -rf public
mkdir -p public/api public/book public/demo
rsync -a .github/pages/ public/
rsync -a target/doc/    public/api/
rsync -a book/book/     public/book/
rsync -a demo/dist/     public/demo/

echo "==> Done. Serve the site with: python3 -m http.server -d public 8080"
