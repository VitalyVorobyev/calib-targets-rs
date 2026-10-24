// `diagnose_*` returns its payload through `serde_json::Value`, which the WASM boundary hands
// over as JS `Map`s, not objects. Convert them so the rest of the app (and the overlays adapter,
// which indexes plain objects) reads one shape.
//
// TODO: drop this when the WASM `diagnose_*` entry points return plain objects (the fix is in
// flight on the covariance branch); until then it is a no-op on already-plain values.

export function plain<T>(value: unknown): T {
  return convert(value) as T;
}

function convert(value: unknown): unknown {
  if (value instanceof Map) {
    const out: Record<string, unknown> = {};
    for (const [key, entry] of value) out[String(key)] = convert(entry);
    return out;
  }
  if (Array.isArray(value)) return value.map(convert);
  if (value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
    const out: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(value)) out[key] = convert(entry);
    return out;
  }
  return value;
}
