// The Generate form's geometry fields. The same two numbers mean different things per kind, so
// the labels say which: inner corners for a chessboard and a marker board, squares for the others.

import { Field, NumberInput, Select } from "@vitavision/ui";

import { useAppState, useDispatch } from "../state/AppContext";
import type { GenerateForm } from "../state/types";

const COUNT = { min: 2, max: 64 } as const;

/** `7 × 10` inner corners is an `8 × 11` board: say so, since that is what gets printed. */
function squaresNote(form: GenerateForm): string | null {
  if (form.kind === "chessboard" || form.kind === "marker_board") {
    return `Board has ${form.rows + 1} × ${form.cols + 1} squares (rows × columns).`;
  }
  return null;
}

export function GeometryForm() {
  const { gen, defaults } = useAppState();
  const dispatch = useDispatch();
  const set = (patch: Partial<GenerateForm>) => dispatch({ type: "setGenerate", patch });
  const innerCorners = gen.kind === "chessboard" || gen.kind === "marker_board";
  const note = squaresNote(gen);
  const screen = gen.medium === "screen";
  const count = gen.kind === "puzzleboard" ? { ...COUNT, min: 4 } : COUNT;

  return (
    <fieldset className="flex flex-col gap-2.5">
      <legend className="sr-only">Board geometry</legend>
      {gen.kind === "puzzlepole" ? (
        <div className="grid grid-cols-2 gap-2.5">
          <Field label="Pieces around" description="Only these close seamlessly.">
            <Select
              value={String(gen.circumference)}
              onValueChange={(value) => set({ circumference: Number(value) })}
              options={(defaults?.poleperiods ?? [[gen.circumference, 0]]).map(([squares]) => ({
                value: String(squares),
                label: String(squares),
              }))}
              aria-label="Pieces around the circumference"
            />
          </Field>
          <Field label="Pieces along axis">
            <NumberInput value={gen.axial} onValueChange={(axial) => set({ axial: Math.round(axial) })} {...COUNT} />
          </Field>
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-2.5">
          <Field label={innerCorners ? "Rows (inner corners)" : "Rows (squares)"}>
            <NumberInput value={gen.rows} onValueChange={(rows) => set({ rows: Math.round(rows) })} {...count} />
          </Field>
          <Field label={innerCorners ? "Columns (inner corners)" : "Columns (squares)"}>
            <NumberInput value={gen.cols} onValueChange={(cols) => set({ cols: Math.round(cols) })} {...count} />
          </Field>
        </div>
      )}
      {note && <p className="text-xs text-fg-muted">{note}</p>}
      {gen.kind === "charuco" && (
        <div className="grid grid-cols-2 gap-2.5">
          <Field label="Dictionary">
            <Select
              value={gen.dictionary}
              onValueChange={(dictionary) => set({ dictionary })}
              options={(defaults?.dictionaries ?? [gen.dictionary]).map((name) => ({ value: name, label: name }))}
              aria-label="ArUco dictionary"
            />
          </Field>
          <Field label="Marker size">
            <NumberInput
              value={gen.markerRel}
              onValueChange={(markerRel) => set({ markerRel })}
              min={0.1}
              max={1}
              step={0.05}
              precision={2}
              unit="× square"
            />
          </Field>
        </div>
      )}
      <div className="grid grid-cols-2 gap-2.5">
        <Field label={screen ? "Requested square size" : "Square size"}>
          <NumberInput
            value={gen.squareMm}
            onValueChange={(squareMm) => set({ squareMm })}
            min={screen ? 0.001 : 1}
            max={screen ? undefined : 100}
            step={1}
            precision={screen ? undefined : 1}
            unit="mm"
          />
        </Field>
        {screen ? (
          <Field label="Minimum margin">
            <NumberInput
              value={gen.screenMargin}
              onValueChange={(value) => set({ screenMargin: Math.round(value) })}
              min={0}
              step={1}
              unit="px"
            />
          </Field>
        ) : (
          <Field label="Resolution">
            <NumberInput
              value={gen.dpi}
              onValueChange={(dpi) => set({ dpi: Math.round(dpi) })}
              min={72}
              max={1200}
              step={50}
              unit="dpi"
            />
          </Field>
        )}
        {screen && (
          <>
            <Field label="Native screen width">
              <NumberInput
                value={gen.screenWidth}
                onValueChange={(value) => set({ screenWidth: Math.round(value) })}
                onClear={() => set({ screenWidth: null })}
                min={1}
                step={1}
                unit="px"
              />
            </Field>
            <Field label="Native screen height">
              <NumberInput
                value={gen.screenHeight}
                onValueChange={(value) => set({ screenHeight: Math.round(value) })}
                onClear={() => set({ screenHeight: null })}
                min={1}
                step={1}
                unit="px"
              />
            </Field>
            <Field label="Physical pixel pitch" description="Width of one display pixel in millimeters.">
              <NumberInput
                value={gen.screenPitchMm}
                onValueChange={(value) => set({ screenPitchMm: value })}
                onClear={() => set({ screenPitchMm: null })}
                min={0.000001}
                step={0.001}
                unit="mm"
              />
            </Field>
          </>
        )}
      </div>
    </fieldset>
  );
}
