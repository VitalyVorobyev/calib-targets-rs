// The friendly board-geometry fields of the active target. They edit the board inside the
// detector parameters; the full parameter forms are under "Detector settings".

import { Field, NumberInput, Select } from "@vitavision/ui";

import { useAppState, useDispatch } from "../state/AppContext";

const COUNT = { min: 2, max: 200 } as const;

export function GeometryFields() {
  const { mode, params, defaults } = useAppState();
  const dispatch = useDispatch();
  if (params === null) return null;

  switch (mode) {
    case "charuco": {
      const board = params.charuco.board;
      const set = (patch: Partial<typeof board>) =>
        dispatch({ type: "setBoard", sync: { mode: "charuco", board: { ...board, ...patch } } });
      return (
        <fieldset className="grid grid-cols-2 gap-2.5">
          <legend className="sr-only">ChArUco board</legend>
          <Field label="Rows (squares)">
            <NumberInput value={board.rows} onValueChange={(rows) => set({ rows: Math.round(rows) })} {...COUNT} />
          </Field>
          <Field label="Columns (squares)">
            <NumberInput value={board.cols} onValueChange={(cols) => set({ cols: Math.round(cols) })} {...COUNT} />
          </Field>
          <Field label="Dictionary">
            <Select
              value={board.dictionary}
              onValueChange={(dictionary) => set({ dictionary })}
              options={(defaults?.dictionaries ?? [board.dictionary]).map((name) => ({ value: name, label: name }))}
              aria-label="ArUco dictionary"
            />
          </Field>
          <Field label="Marker size">
            <NumberInput
              value={board.marker_size_rel}
              onValueChange={(marker_size_rel) => set({ marker_size_rel })}
              min={0.1}
              max={1}
              step={0.05}
              precision={2}
              unit="× square"
            />
          </Field>
        </fieldset>
      );
    }
    case "markerboard": {
      const board = params.markerboard.board;
      const set = (patch: Partial<typeof board>) =>
        dispatch({ type: "setBoard", sync: { mode: "markerboard", board: { ...board, ...patch } } });
      return (
        <fieldset className="grid grid-cols-2 gap-2.5">
          <legend className="sr-only">Marker board</legend>
          <Field label="Rows (inner corners)">
            <NumberInput value={board.rows} onValueChange={(rows) => set({ rows: Math.round(rows) })} {...COUNT} />
          </Field>
          <Field label="Columns (inner corners)">
            <NumberInput value={board.cols} onValueChange={(cols) => set({ cols: Math.round(cols) })} {...COUNT} />
          </Field>
          <p className="col-span-2 text-xs text-fg-muted">
            {`Circles: ${board.circles.map((c) => `${c.polarity} at (${c.cell.i}, ${c.cell.j})`).join(", ")}. A generated board sets them.`}
          </p>
        </fieldset>
      );
    }
    case "puzzleboard": {
      const board = params.puzzleboard.board;
      const set = (patch: Partial<typeof board>) =>
        dispatch({ type: "setBoard", sync: { mode: "puzzleboard", board: { ...board, ...patch } } });
      return (
        <fieldset className="grid grid-cols-2 gap-2.5">
          <legend className="sr-only">PuzzleBoard</legend>
          <Field label="Rows (squares)">
            <NumberInput value={board.rows} onValueChange={(rows) => set({ rows: Math.round(rows) })} {...COUNT} />
          </Field>
          <Field label="Columns (squares)">
            <NumberInput value={board.cols} onValueChange={(cols) => set({ cols: Math.round(cols) })} {...COUNT} />
          </Field>
        </fieldset>
      );
    }
    default:
      return null;
  }
}
