// Cross-check the hand-written object-shape declarations in
// `typescript-extras.d.ts` against the generated JSON Schemas in `schemas/`.
//
// Usage (from anywhere):
//   node crates/calib-targets-wasm/tests/check-schema-keys.mjs
//
// For every `export interface X { ... }` whose name matches a schema type
// (the root title or a `$defs` entry of any shipped schema), it requires that
//   - every key the interface declares is a property in the schema, and
//   - every property the schema marks `required` is declared in the interface
//     (as optional or not),
// so a renamed, added or removed config field cannot slip past the hand-written
// declarations. It deliberately ignores types (the construction check in
// `typescript-extras.check.ts` and the schema validation tests cover shapes) and
// interfaces with no schema counterpart (detection results, diagnostics, ...).
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const dtsPath = path.join(here, "..", "typescript-extras.d.ts");
const schemaDir = path.resolve(process.argv[2] ?? path.join(here, "..", "..", "..", "schemas"));

// name -> [{file, props: Set, required: Set}]
const schemaTypes = new Map();
const addType = (name, file, node) => {
  if (!node || typeof node !== "object" || !node.properties) return;
  const entry = {
    file,
    props: new Set(Object.keys(node.properties)),
    required: new Set(node.required ?? []),
  };
  schemaTypes.set(name, [...(schemaTypes.get(name) ?? []), entry]);
};
for (const file of readdirSync(schemaDir).filter((f) => f.endsWith(".json"))) {
  const schema = JSON.parse(readFileSync(path.join(schemaDir, file), "utf8"));
  addType(schema.title, file, schema);
  for (const [name, def] of Object.entries(schema.$defs ?? {})) addType(name, file, def);
}

// Declarations whose name differs from the Rust type they mirror. `ChessConfig`
// is the TypeScript name of `chess_corners::DetectorConfig` (the schema's
// `ChessConfig` is a different, inner type), so it is looked up by its Rust name.
const rustName = { ChessConfig: "DetectorConfig" };

const dts = readFileSync(dtsPath, "utf8");
const interfaces = [...dts.matchAll(/^export interface (\w+)(?:<[^>]*>)? \{\n([\s\S]*?)^\}/gm)];
let checked = 0;
const problems = [];
for (const [, name, body] of interfaces) {
  const candidates = schemaTypes.get(rustName[name] ?? name);
  if (!candidates) continue;
  const keys = new Set([...body.matchAll(/^  (\w+)\??:/gm)].map((m) => m[1]));
  // The same name can mean different shapes in different schemas (the printable
  // and detector `MarkerCircleSpec`); the declaration must fit at least one.
  const verdicts = candidates.map((c) => {
    const extra = [...keys].filter((k) => !c.props.has(k));
    const missing = [...c.required].filter((k) => !keys.has(k));
    return { file: c.file, extra, missing };
  });
  checked += 1;
  if (!verdicts.some((v) => v.extra.length === 0 && v.missing.length === 0)) {
    for (const v of verdicts) {
      problems.push(
        `${name} vs ${v.file}: ` +
          `not in schema: [${v.extra.join(", ")}]; required but undeclared: [${v.missing.join(", ")}]`,
      );
    }
  }
}

if (problems.length) {
  console.error("typescript-extras.d.ts disagrees with the JSON Schemas:\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log(`typescript-extras.d.ts agrees with the schemas on ${checked} interfaces`);
