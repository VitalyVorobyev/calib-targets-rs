// Schema-driven advanced-param form. Renders the `advanced` tuning block as
// grouped, labelled, tooltipped fields using the metadata catalogue served by
// GET /api/params/schema (Rust source of truth:
// crates/calib-targets-studio/src/routes/params_schema.rs).
//
// Any scalar leaf the schema doesn't cover (or every leaf, if the schema fails
// to load) falls through to an "Unmapped" section rendered from the raw JSON
// key — so no knob is ever hidden, and the form degrades gracefully.

import { useQuery } from "@tanstack/react-query";
import { Checkbox, Disclosure, InfoHint, NumberInput, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type { ParamKind } from "../api/types";

type Obj = Record<string, unknown>;

const ADVANCED_PREFIX = "/advanced/";

function relPath(pointer: string): string {
  return pointer.startsWith(ADVANCED_PREFIX)
    ? pointer.slice(ADVANCED_PREFIX.length)
    : pointer;
}

function getAt(obj: Obj, path: string): unknown {
  let cur: unknown = obj;
  for (const key of path.split("/")) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Obj)[key];
  }
  return cur;
}

function setAt(obj: Obj, path: string, value: unknown): Obj {
  const [head = "", ...rest] = path.split("/");
  if (rest.length === 0) return { ...obj, [head]: value };
  const child = (obj[head] ?? {}) as Obj;
  return { ...obj, [head]: setAt(child, rest.join("/"), value) };
}

function isScalar(v: unknown): boolean {
  return typeof v === "boolean" || typeof v === "number";
}

/** Collect `topological/foo`-style relpaths of every scalar leaf in `node`,
 *  recursing one level into nested objects — the leaves a form can edit. */
function scalarRelPaths(node: Obj): string[] {
  const out: string[] = [];
  for (const [key, val] of Object.entries(node)) {
    if (val !== null && typeof val === "object" && !Array.isArray(val)) {
      for (const [subKey, subVal] of Object.entries(val as Obj)) {
        if (isScalar(subVal)) out.push(`${key}/${subKey}`);
      }
    } else if (isScalar(val)) {
      out.push(key);
    }
  }
  return out;
}

export function ParamForm({
  node,
  defaults,
  onChange,
}: {
  node: Obj;
  defaults: Obj;
  onChange: (next: Obj) => void;
}) {
  const schema = useQuery({
    queryKey: ["param-schema"],
    staleTime: Infinity,
    queryFn: api.paramSchema,
  });

  if (schema.isLoading) {
    return <div className="text-[11px] text-fg-subtle">loading param schema…</div>;
  }

  // On error, fall through with an empty schema → every leaf renders Unmapped.
  const s = schema.data ?? { groups: [], fields: [] };
  const covered = new Set(s.fields.map((f) => relPath(f.pointer)));

  const update = (path: string, value: unknown) =>
    onChange(setAt(node, path, value));

  // Sections from the schema, in declared order; skip any with no field that
  // resolves to a present value in the current advanced block.
  const sections = s.groups
    .map((g) => ({
      ...g,
      fields: s.fields.filter(
        (f) => f.group === g.id && getAt(node, relPath(f.pointer)) !== undefined,
      ),
    }))
    .filter((g) => g.fields.length > 0);

  // Any scalar leaf the schema didn't claim (new knob, or schema load failed).
  const unmapped = scalarRelPaths(node).filter((p) => !covered.has(p));

  return (
    <div className="flex flex-col gap-2">
      {schema.isError && (
        <div className="text-[11px] text-warn">
          param schema unavailable — showing raw keys
        </div>
      )}
      {sections.map((g) => (
        <Section key={g.id} title={g.title}>
          {g.fields.map((f) => {
            const path = relPath(f.pointer);
            const gatePath = f.gated_by ? relPath(f.gated_by) : null;
            const disabled =
              gatePath != null && getAt(node, gatePath) === false;
            return (
              <ParamRow
                key={f.pointer}
                label={f.label}
                help={f.help}
                kind={f.kind}
                value={getAt(node, path)}
                defaultValue={getAt(defaults, path)}
                disabled={disabled}
                onChange={(v) => update(path, v)}
              />
            );
          })}
        </Section>
      ))}
      {unmapped.length > 0 && (
        <Section title="unmapped" defaultOpen>
          {unmapped.map((path) => (
            <ParamRow
              key={path}
              label={path}
              help=""
              kind={inferKind(getAt(node, path))}
              value={getAt(node, path)}
              defaultValue={getAt(defaults, path)}
              disabled={false}
              onChange={(v) => update(path, v)}
            />
          ))}
        </Section>
      )}
    </div>
  );
}

function inferKind(value: unknown): ParamKind {
  if (typeof value === "boolean") return "bool";
  if (typeof value === "number") return Number.isInteger(value) ? "int" : "float";
  return "float";
}

/** One schema leaf: its label (tinted when it differs from the default) and a ui control. */
function ParamRow({
  label,
  help,
  kind,
  value,
  defaultValue,
  disabled,
  onChange,
}: {
  label: string;
  help: string;
  kind: ParamKind;
  value: unknown;
  defaultValue: unknown;
  disabled: boolean;
  onChange: (v: unknown) => void;
}) {
  const modified =
    defaultValue !== undefined &&
    JSON.stringify(value) !== JSON.stringify(defaultValue);
  const row = cn(
    "grid grid-cols-[1fr_92px] items-center gap-2 text-[11px]",
    disabled && "opacity-45",
  );
  const labelText = (
    <span className={modified ? "text-signal" : "text-fg-muted"}>{label}</span>
  );
  const hint = help ? <InfoHint label={`About ${label}`}>{help}</InfoHint> : null;

  if (kind === "bool") {
    return (
      <div className={row}>
        <span className="inline-flex items-center gap-1">
          {labelText}
          {hint}
        </span>
        <Checkbox
          aria-label={label}
          checked={value === true}
          disabled={disabled}
          onCheckedChange={(checked) => onChange(checked)}
        />
      </div>
    );
  }

  const isInt = kind === "int";
  return (
    <div className={row}>
      <span className="inline-flex items-center gap-1">
        {labelText}
        {hint}
      </span>
      <NumberInput
        aria-label={label}
        step={isInt ? 1 : "any"}
        value={typeof value === "number" ? value : ""}
        disabled={disabled}
        className="px-1.5 text-xs"
        onChange={(e) => {
          const v = e.target.valueAsNumber;
          if (Number.isNaN(v)) return;
          onChange(isInt ? Math.round(v) : v);
        }}
      />
    </div>
  );
}

function Section({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  return (
    <Disclosure
      summary={title}
      defaultOpen={defaultOpen}
      className="rounded-control border border-line bg-raised/40 px-2.5 py-1 [&>div]:flex [&>div]:flex-col [&>div]:gap-1.5 [&>div]:pt-2 [&>div]:pb-1.5"
    >
      {children}
    </Disclosure>
  );
}
