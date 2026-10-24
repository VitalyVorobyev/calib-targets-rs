import { Field, Select, Switch } from "@vitavision/ui";

import { useAppState, useDispatch } from "../state/AppContext";
import type { DetectMode } from "../worker/protocol";
import { GeometryFields } from "./GeometryFields";

export const MODES: { value: DetectMode; label: string; note: string }[] = [
  { value: "corners", label: "Corners", note: "Raw ChESS corners, no grid" },
  { value: "chessboard", label: "Chessboard", note: "Labelled inner corners" },
  { value: "charuco", label: "ChArUco", note: "Chessboard with ArUco markers" },
  { value: "markerboard", label: "Marker board", note: "Chessboard with three circles" },
  { value: "puzzleboard", label: "PuzzleBoard", note: "Self-identifying chessboard" },
];

export function TargetSection() {
  const { mode, sweep, auto } = useAppState();
  const dispatch = useDispatch();
  return (
    <div className="flex flex-col gap-2.5">
      <Field label="Target">
        <Select
          value={mode}
          onValueChange={(value) => dispatch({ type: "setMode", mode: value as DetectMode })}
          options={MODES}
          aria-label="Target"
        />
      </Field>
      <GeometryFields />
      {mode !== "corners" && (
        <Switch
          checked={sweep}
          onCheckedChange={(checked) => dispatch({ type: "setSweep", sweep: checked })}
          label="Multi-config sweep"
          description="Try the built-in presets and keep the best result. Slower, more robust; it uses its own ChESS front-ends and leaves out the circle and edge-bit layers."
        />
      )}
      <Switch
        checked={auto}
        onCheckedChange={(checked) => dispatch({ type: "setAuto", auto: checked })}
        label="Detect on every change"
        description="Off: edit freely, then press Detect."
      />
    </div>
  );
}
