// The detector parameter schema (the repository's `schemas/chessboard_params.json`, generated
// from `ChessboardParams`) and what the studio adds to it: the advanced-knob catalogue's
// sections, labels and tooltips, as a `UiSchema`.
//
// The schema says what the data is; the catalogue (`GET /api/params/schema`, hand-maintained
// beside the server, with a test that every knob has an entry) says how to present it. A knob
// the catalogue does not know still renders, from the schema, in the form's ungrouped remainder.

import type { JsonSchema, UiSchema } from "@vitavision/forms";

import chessboardParams from "../../../schemas/chessboard_params.json";
import type { ParamSchema } from "../api/types";

export const PARAMS_SCHEMA: JsonSchema = chessboardParams;

/** The stable core of `ChessboardParams`: the top-level keys the Detect tab edits too. */
export const CORE_FIELDS = ["min_labeled_corners", "max_components", "min_corner_strength"] as const;

const CORE_UI: NonNullable<UiSchema["fields"]> = {
  min_labeled_corners: { label: "Min labeled corners", span: 1 },
  max_components: { label: "Max components", span: 1 },
  min_corner_strength: { label: "Min corner strength", span: 1 },
};

/** `/advanced/topological/foo` as the dot path `advanced.topological.foo` the forms use. */
export function dotPath(pointer: string): string {
  return pointer.split("/").filter(Boolean).join(".");
}

/**
 * The presentation of the params form.
 *
 * `advanced` is shown only when the draft overrides it (the whole block, as the CLI merge
 * requires); otherwise the field stays in the value, hidden, and is never edited.
 */
export function paramsUi(catalogue: ParamSchema | undefined, showAdvanced: boolean): UiSchema {
  const fields: NonNullable<UiSchema["fields"]> = { ...CORE_UI };
  if (!showAdvanced) return { fields: { ...fields, advanced: { hidden: true } } };
  const groups: NonNullable<UiSchema["groups"]> = [{ id: "core", title: "Core", fields: [...CORE_FIELDS] }];
  for (const group of catalogue?.groups ?? []) {
    const members = (catalogue?.fields ?? []).filter((f) => f.group === group.id);
    if (members.length === 0) continue;
    groups.push({
      id: group.id,
      title: group.title,
      collapsible: true,
      fields: members.map((f) => dotPath(f.pointer)),
    });
  }
  for (const f of catalogue?.fields ?? []) {
    fields[dotPath(f.pointer)] = { label: f.label, hint: f.help, span: 1 };
  }
  return { groups, fields };
}

/** A catalogue knob that another flag gates. */
export interface GatedKnob {
  /** Path of the boolean flag. */
  gate: string;
  kind: string;
  label: string;
  help: string;
}

/** Knob path → its gate, for the knobs the catalogue gates. */
export function gates(catalogue: ParamSchema | undefined): Map<string, GatedKnob> {
  const out = new Map<string, GatedKnob>();
  for (const f of catalogue?.fields ?? []) {
    if (f.gated_by) {
      out.set(dotPath(f.pointer), { gate: dotPath(f.gated_by), kind: f.kind, label: f.label, help: f.help });
    }
  }
  return out;
}
