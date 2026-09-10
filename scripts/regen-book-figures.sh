#!/usr/bin/env bash
# Regenerate the book's generated figures.
#
# Every figure this produces is drawn from the *shipped* code maps through the
# *real* renderer, so a figure cannot drift from the thing it illustrates. Run
# it whenever the pattern, the renderer, or a geometry convention changes, and
# commit the result — the SVGs are committed so the book builds without a
# toolchain.
#
# PUBLIC DATA ONLY. Nothing here reads `privatedata/`, and nothing here may:
# the book is published, and the disclosure policy in
# `docs/development/private-dataset-policy.md` is not negotiable. Figures are
# synthesised from the code maps, not rendered from captured frames.
#
# Usage:
#   scripts/regen-book-figures.sh            # write into book/src/img
#   scripts/regen-book-figures.sh --check    # fail if anything would change
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="book/src/img"
check=0
if [[ "${1:-}" == "--check" ]]; then
  check=1
  out="$(mktemp -d)"
  trap 'rm -rf "$out"' EXIT
fi

cargo run --quiet -p calib-targets --example book_figures -- "$out"

# The rendered PuzzlePole, from the same renderer the end-to-end tests assert
# against. It is a PNG rather than an SVG because it is a *render* — a ray-cast
# of a curved surface, not a drawing — and PNG bytes are not stable across
# encoder versions, so `--check` below deliberately diffs only the SVGs. A
# figure that changed here is reviewed by looking at it.
cargo run --quiet -p calib-targets-puzzleboard --example render_puzzlepole -- "$out"

# The same render is the Python bindings' detection fixture. Copying it here
# rather than rendering it twice is what keeps the figure and the fixture from
# drifting apart -- a test passing against an image the book no longer shows
# would be worse than either problem alone.
if [[ "$check" != "1" ]]; then
  cp "$out/puzzlepole_view.png" testdata/puzzlepole_view.png
fi

if [[ "$check" == "1" ]]; then
  status=0
  for generated in "$out"/*.svg; do
    committed="book/src/img/$(basename "$generated")"
    if ! diff -q "$committed" "$generated" >/dev/null 2>&1; then
      echo "stale figure: $committed (re-run scripts/regen-book-figures.sh)"
      status=1
    fi
  done
  exit "$status"
fi

echo
echo "Regenerated into $out. Review the diff before committing:"
echo "  git diff --stat $out"
