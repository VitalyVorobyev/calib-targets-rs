// The detector parameters as a form over the `ChessboardParams` JSON Schema, with the
// advanced-knob catalogue's sections, labels and tooltips laid over it.
//
// The params-passthrough contract: what the user edits is exactly what the server receives.
// The draft is a partial override (an absent top-level key means "the family default"), but
// the schema form wants a complete value, so it is given `defaults ⊕ draft`; what comes back is
// reduced to a draft again by `overrideOf`, which keeps every key that was already in the draft
// (unknown keys included: the forms never drop what the schema does not name) and every key whose
// value now differs from the default. `advanced`, when present, is the complete block.

import { useQuery } from "@tanstack/react-query";
import { SchemaValueForm, getAtPath } from "@vitavision/forms";
import { ErrorBox, Field, NumberInput, Switch } from "@vitavision/ui";
import { useMemo } from "react";

import { api } from "../api/client";
import type { DetectorParamsOverride, DetectorReq } from "../api/types";
import { CORE_FIELDS, PARAMS_SCHEMA, gates, paramsUi } from "../lib/schemas";

type Params = Record<string, unknown>;

/** The effective defaults of a family: the complete params its detector runs with. */
export function useEffectiveDefaults(family: DetectorReq) {
  return useQuery({
    queryKey: ["effective-defaults", family],
    queryFn: () => api.effectiveDefaults(family),
    staleTime: Infinity,
  });
}

/** The draft the form's next value implies. Exported for the tests. */
export function overrideOf(next: Params, prev: Params, defaults: Params): DetectorParamsOverride {
  const out: Params = {};
  for (const [key, value] of Object.entries(next)) {
    if (key in prev || JSON.stringify(value) !== JSON.stringify(defaults[key])) out[key] = value;
  }
  return out;
}

export function ParamsForm({
  draft,
  family,
  onChange,
  part,
}: {
  draft: DetectorParamsOverride;
  family: DetectorReq;
  onChange: (next: DetectorParamsOverride) => void;
  /** `core`: the three stable top-level fields. `all`: those and the advanced block, when overridden. */
  part: "core" | "all";
}) {
  const defaults = useEffectiveDefaults(family);
  const catalogue = useQuery({ queryKey: ["param-schema"], staleTime: Infinity, queryFn: api.paramSchema });

  const showAdvanced = part === "all" && draft.advanced != null;
  const ui = useMemo(() => {
    const base = paramsUi(catalogue.data, showAdvanced);
    if (part === "all") return base;
    // The Detect tab edits the core only: hide the rest, keep its value as it is.
    return { fields: { ...base.fields, advanced: { hidden: true } } };
  }, [catalogue.data, showAdvanced, part]);
  const gated = useMemo(() => gates(catalogue.data), [catalogue.data]);

  if (defaults.isError) return <ErrorBox>{`defaults unavailable: ${String(defaults.error)}`}</ErrorBox>;
  if (defaults.data === undefined) return <div className="text-[11px] text-fg-subtle">loading defaults…</div>;
  const base = defaults.data as Params;
  const value = { ...base, ...draft } as Params;

  return (
    <div className="flex flex-col gap-2">
      {catalogue.isError && (
        <div className="text-[11px] text-warn">knob catalogue unavailable: labels come from the schema</div>
      )}
      <SchemaValueForm
        schema={PARAMS_SCHEMA}
        ui={ui}
        value={value}
        columns={1}
        onValueChange={(next) => onChange(overrideOf(next as Params, draft as Params, base))}
        renderField={({ path, value: current, onChange: set }) => {
          const knob = gated.get(path);
          if (knob === undefined || getAtPath(value, knob.gate) !== false) return undefined;
          // A knob whose flag is off keeps its value but cannot be edited: shown greyed, as before.
          return knob.kind === "bool" ? (
            <Switch
              checked={current === true}
              disabled
              label={knob.label}
              description={knob.help}
              onCheckedChange={set}
            />
          ) : (
            <Field label={knob.label} description={`${knob.help} Off while ${knob.gate.split(".").pop()} is off.`}>
              <NumberInput disabled value={typeof current === "number" ? current : ""} onChange={() => undefined} />
            </Field>
          );
        }}
      />
    </div>
  );
}

export { CORE_FIELDS };
