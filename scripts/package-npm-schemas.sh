#!/usr/bin/env bash
# Ship the JSON Schemas (schemas/*.json) in the npm package.
#
# wasm-pack writes `pkg/package.json` with a closed `files` list, so the schemas
# have to be copied in and added to it explicitly. Then `npm pack --dry-run`
# proves the tarball would really contain them. No `exports` field is added:
# consumers deep-import `@vitavision/calib-targets/schemas/<name>.json`.
#
# Run after `wasm-pack build` (and after `npm pkg set name=...`, which keeps
# `files`). Idempotent.
#
# Usage: scripts/package-npm-schemas.sh [pkg-dir]   (default: crates/calib-targets-wasm/pkg)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$(cd "${1:-$ROOT/crates/calib-targets-wasm/pkg}" && pwd)"

test -f "$PKG/package.json" || { echo "no package.json in $PKG; run wasm-pack first" >&2; exit 1; }
shopt -s nullglob
SCHEMAS=("$ROOT"/schemas/*.json)
test "${#SCHEMAS[@]}" -gt 0 || { echo "no schemas in $ROOT/schemas; run \`cargo xtask emit-schemas\`" >&2; exit 1; }

rm -rf "$PKG/schemas"
mkdir -p "$PKG/schemas"
cp "${SCHEMAS[@]}" "$PKG/schemas/"

# Append "schemas" to `files`, preserving the entries wasm-pack wrote.
node -e '
const fs = require("fs");
const file = process.argv[1] + "/package.json";
const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
const files = Array.isArray(pkg.files) ? pkg.files : [];
if (!files.includes("schemas")) files.push("schemas");
pkg.files = files;
if (pkg.exports !== undefined) {
  console.error("package.json has an `exports` field; schemas would not be deep-importable");
  process.exit(1);
}
fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
' "$PKG"

# Assert the tarball would contain every schema file (and still the wasm payload).
LISTING="$(cd "$PKG" && npm pack --dry-run --json)"
NAMES=()
for f in "${SCHEMAS[@]}"; do NAMES+=("schemas/$(basename "$f")"); done
node -e '
const listing = JSON.parse(process.argv[1])[0].files.map((f) => f.path);
const required = process.argv.slice(2);
const missing = required.filter((n) => !listing.includes(n));
if (missing.length) {
  console.error("npm pack is missing: " + missing.join(", "));
  console.error("package contents: " + listing.join(", "));
  process.exit(1);
}
console.log("npm package ships " + listing.filter((n) => n.startsWith("schemas/")).length + " schema files; contents:\n  " + listing.join("\n  "));
' "$LISTING" "${NAMES[@]}" calib_targets_wasm_bg.wasm calib_targets_wasm.js calib_targets_wasm.d.ts
