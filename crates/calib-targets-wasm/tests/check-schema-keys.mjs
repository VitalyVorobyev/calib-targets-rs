// Cross-check the hand-written object-shape declarations in
// `typescript-extras.d.ts` against the generated JSON Schemas in `schemas/`.
//
// Usage (from anywhere):
//   node crates/calib-targets-wasm/tests/check-schema-keys.mjs
//
// A declaration is matched to a schema type (the root title or a `$defs` entry
// of any shipped schema) by its Rust name: its own name, or the one
// `rustName` maps it to, or — for an alias of another declaration
// (`type RefinerConfig = ChessRefiner`) — the aliased type's Rust name. Three
// checks then apply:
//
//   1. Keys. For every `export interface X { ... }`, the declared keys and the
//      schema's properties are the same set (a key may be declared optional),
//      so a renamed, added or removed config field cannot slip past the
//      declarations — including an added field with a serde default, which no
//      schema marks `required`.
//   2. Referenced type names. Where the schema types a property as a `$ref` to
//      another schema type (directly, nullable, or as an array's items) and the
//      declaration names a type there, the named type must resolve to that same
//      schema type. This is what catches a declaration that has the right keys
//      but points a field at the wrong shape — `refiner: RefinerConfig`, a
//      `{ kind, center_of_mass, ... }` object, where the schema says
//      `ChessRefiner`, an externally tagged union.
//   3. Tagged unions. For every `export type X = A | B | ...` matched to a
//      schema `oneOf` / `enum` of unit strings and single-key objects (serde's
//      externally tagged enums), the declared variant tags — `"name"` for a
//      string literal, `{name}` for a single-key object — must equal the
//      schema's. A missing variant (`DetectionStrategy` without `radon`) or a
//      wrong encoding (`{ mode: "fixed" }` for `{ fixed: k }`) fails here.
//
// Field *value* types beyond names (numbers vs strings, inline object shapes)
// are left to the construction check in `typescript-extras.check.ts` and the
// schema validation tests; declarations with no schema counterpart (detection
// results, diagnostics, ...) are ignored.
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const dtsPath = path.join(here, "..", "typescript-extras.d.ts");
const schemaDir = path.resolve(process.argv[2] ?? path.join(here, "..", "..", "..", "schemas"));

// name -> [{file, node}]
const schemaTypes = new Map();
const addType = (name, file, node) => {
  if (!name || !node || typeof node !== "object") return;
  schemaTypes.set(name, [...(schemaTypes.get(name) ?? []), { file, node }]);
};
for (const file of readdirSync(schemaDir).filter((f) => f.endsWith(".json"))) {
  const schema = JSON.parse(readFileSync(path.join(schemaDir, file), "utf8"));
  addType(schema.title, file, schema);
  for (const [name, def] of Object.entries(schema.$defs ?? {})) addType(name, file, def);
}

// Declarations whose name differs from the Rust type they mirror. `ChessConfig`
// is the TypeScript name of `chess_corners::DetectorConfig`, and
// `ChessStrategyConfig` that of the inner `chess_corners::ChessConfig` (the
// strategy payload), so both are looked up by their Rust names. The printable
// model's `TargetSpec` is declared as `PrintableTargetSpec` to keep the one flat
// TypeScript namespace unambiguous.
const rustName = {
  ChessConfig: "DetectorConfig",
  ChessStrategyConfig: "ChessConfig",
  PrintableTargetSpec: "TargetSpec",
};

const dts = readFileSync(dtsPath, "utf8");

// `export type X = <body>;` — the body may span lines; it ends at the first `;`
// outside any bracket. Comments are dropped first so their text cannot end a
// body early or pass for a field.
const code = dts.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
const aliases = new Map();
for (const m of code.matchAll(/^export type (\w+)(?:<[^>]*>)? =/gm)) {
  let depth = 0;
  let i = m.index + m[0].length;
  for (; i < code.length; i += 1) {
    const ch = code[i];
    if ("{[(<".includes(ch)) depth += 1;
    else if ("}])>".includes(ch)) depth -= 1;
    else if (ch === ";" && depth === 0) break;
  }
  aliases.set(m[1], code.slice(m.index + m[0].length, i).trim());
}

/** The Rust (schema) name a declared TypeScript type name stands for. */
const resolve = (name, seen = new Set()) => {
  if (seen.has(name)) return name;
  seen.add(name);
  const body = aliases.get(name);
  if (body && /^\w+$/.test(body)) return resolve(body, seen);
  return rustName[name] ?? name;
};

/** Split `text` on `sep` at bracket depth 0. */
const splitTop = (text, sep) => {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if ("{[(<".includes(ch)) depth += 1;
    else if ("}])>".includes(ch)) depth -= 1;
    else if (ch === sep && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((p) => p.trim()).filter((p) => p.length > 0);
};

/** Schema type names a property schema refers to (`$ref`, nullable, array items). */
const refNames = (prop) => {
  if (!prop || typeof prop !== "object") return [];
  const own = prop.$ref ? [prop.$ref.split("/").pop()] : [];
  const alts = [...(prop.anyOf ?? []), ...(prop.oneOf ?? [])].flatMap(refNames);
  const items = prop.type === "array" && prop.items ? refNames(prop.items) : [];
  return [...own, ...alts, ...items];
};

/**
 * Type names a declared field type names, or `null` when the declaration is not
 * just names (an inline object or literal union) — those are not checked here.
 */
const declaredNames = (typeText) => {
  const names = [];
  for (const part of splitTop(typeText.replace(/\s+/g, " "), "|")) {
    if (part === "null" || part === "undefined") continue;
    const m = /^(\w+)(?:\[\])?$/.exec(part);
    if (!m || ["number", "string", "boolean"].includes(m[1])) return null;
    names.push(m[1]);
  }
  return names.length ? names : null;
};

/**
 * Variant tags of a schema enum, or `null` if it is not a tagged union. A unit
 * variant is `"name"`; an externally tagged one `{name}` (its single key); an
 * internally tagged one `{key="name"}` (the property pinned by `const`).
 */
const schemaTags = (node) => {
  const variants = node.oneOf ?? (node.enum ? node.enum.map((c) => ({ const: c })) : null);
  if (!variants) return null;
  const tags = [];
  for (const v of variants) {
    const props = v.type === "object" && v.properties ? Object.entries(v.properties) : [];
    const pinned = props.find(([, p]) => typeof p.const === "string");
    if (typeof v.const === "string") tags.push(`"${v.const}"`);
    else if (Array.isArray(v.enum) && v.enum.every((c) => typeof c === "string"))
      tags.push(...v.enum.map((c) => `"${c}"`));
    else if (pinned) tags.push(`{${pinned[0]}="${pinned[1].const}"}`);
    else if (props.length === 1) tags.push(`{${props[0][0]}}`);
    else return null; // untagged — not this check's business
  }
  return new Set(tags);
};

/** Variant tags of a declared union alias body (see `schemaTags`), or `null`. */
const declaredTags = (body) => {
  const tags = [];
  for (const member of splitTop(body.replace(/\s+/g, " "), "|")) {
    const lit = /^"([^"]*)"$/.exec(member);
    if (lit) {
      tags.push(`"${lit[1]}"`);
      continue;
    }
    const obj = /^\{(.*)\}$/.exec(member);
    if (!obj) return null;
    const fields = splitTop(obj[1], ";")
      .flatMap((f) => splitTop(f, ","))
      .map((f) => /^(\w+)\??: ?(.*)$/.exec(f))
      .filter(Boolean);
    const pinned = fields.find(([, , type]) => /^"[^"]*"$/.test(type.trim()));
    if (pinned) tags.push(`{${pinned[1]}=${pinned[2].trim()}}`);
    else if (fields.length === 1) tags.push(`{${fields[0][1]}}`);
    else return null;
  }
  return new Set(tags);
};

const sameSet = (a, b) => a.size === b.size && [...a].every((x) => b.has(x));
const show = (set) => `[${[...set].sort().join(", ")}]`;

let checkedInterfaces = 0;
let checkedRefs = 0;
let checkedUnions = 0;
const problems = [];

// 1 + 2: interfaces.
const interfaces = [...dts.matchAll(/^export interface (\w+)(?:<[^>]*>)? \{\n([\s\S]*?)^\}/gm)];
for (const [, name, body] of interfaces) {
  const candidates = (schemaTypes.get(resolve(name)) ?? []).filter((c) => c.node.properties);
  if (!candidates.length) continue;
  const fields = new Map(
    [...body.matchAll(/^  (\w+)\??: ([\s\S]*?);\s*$/gm)].map((m) => [m[1], m[2]]),
  );
  const keys = new Set([...body.matchAll(/^  (\w+)\??:/gm)].map((m) => m[1]));
  // The same name can mean different shapes in different schemas (the printable
  // and detector `MarkerCircleSpec`); the declaration must fit at least one.
  let refsHere = 0;
  const verdicts = candidates.map((c) => {
    const props = c.node.properties;
    const extra = [...keys].filter((k) => !(k in props));
    const missing = Object.keys(props).filter((k) => !keys.has(k));
    const wrongRefs = [];
    for (const [key, typeText] of fields) {
      const want = new Set(refNames(props[key]));
      const have = declaredNames(typeText);
      if (!want.size || !have) continue;
      refsHere += 1;
      const got = new Set(have.map((n) => resolve(n)));
      if (!sameSet(got, want)) wrongRefs.push(`${key}: ${typeText} (schema: ${show(want)})`);
    }
    return { file: c.file, extra, missing, wrongRefs };
  });
  checkedInterfaces += 1;
  checkedRefs += refsHere;
  const ok = (v) => v.extra.length === 0 && v.missing.length === 0 && v.wrongRefs.length === 0;
  if (!verdicts.some(ok)) {
    for (const v of verdicts) {
      problems.push(
        `${name} vs ${v.file}: ` +
          `not in schema: [${v.extra.join(", ")}]; in schema but undeclared: [${v.missing.join(", ")}]; ` +
          `field names a different schema type: [${v.wrongRefs.join("; ")}]`,
      );
    }
  }
}

// 3: externally tagged unions.
for (const [name, body] of aliases) {
  if (/^\w+$/.test(body)) continue; // a plain alias; checked as its target
  const candidates = (schemaTypes.get(resolve(name)) ?? [])
    .map((c) => ({ file: c.file, tags: schemaTags(c.node) }))
    .filter((c) => c.tags);
  if (!candidates.length) continue;
  const declared = declaredTags(body);
  checkedUnions += 1;
  if (!declared) {
    problems.push(`${name}: union members are not string literals / single-key objects: ${body}`);
    continue;
  }
  if (!candidates.some((c) => sameSet(c.tags, declared))) {
    for (const c of candidates) {
      problems.push(`${name} vs ${c.file}: declared variants ${show(declared)}, schema ${show(c.tags)}`);
    }
  }
}

if (problems.length) {
  console.error("typescript-extras.d.ts disagrees with the JSON Schemas:\n  " + problems.join("\n  "));
  process.exit(1);
}
console.log(
  `typescript-extras.d.ts agrees with the schemas on ${checkedInterfaces} interfaces ` +
    `(${checkedRefs} referenced type names) and ${checkedUnions} tagged unions`,
);
