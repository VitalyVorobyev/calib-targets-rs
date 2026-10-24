// The full detector configuration, as forms over the shipped JSON Schemas.

import { SchemaValueForm } from "@vitavision/forms";
import { Button, Disclosure } from "@vitavision/ui";

import { CHESS_SCHEMA, CHESS_UI, PARAMS_SCHEMA, PARAMS_UI } from "../lib/schemas";
import { useAppState, useDispatch } from "../state/AppContext";
import type { TargetParams } from "../worker/protocol";
import { MODES } from "./TargetSection";

export function SettingsSection() {
  const { mode, chess, params, run } = useAppState();
  const dispatch = useDispatch();
  const busy = run.status === "running";
  if (chess === null || params === null) return null;

  const boardMode = mode === "corners" ? null : mode;
  const label = MODES.find((entry) => entry.value === mode)?.label ?? mode;

  return (
    <div className="flex flex-col gap-2.5">
      <Disclosure summary="ChESS corner detector">
        <SchemaValueForm
          schema={CHESS_SCHEMA}
          ui={CHESS_UI}
          value={chess}
          columns={1}
          disabled={busy}
          onValueChange={(next) => dispatch({ type: "setChess", chess: next as typeof chess })}
        />
      </Disclosure>
      {boardMode !== null && (
        <Disclosure summary={`${label} parameters`}>
          <SchemaValueForm
            key={boardMode}
            schema={PARAMS_SCHEMA[boardMode]}
            ui={PARAMS_UI[boardMode]}
            value={params[boardMode]}
            columns={1}
            disabled={busy}
            onValueChange={(next) =>
              dispatch({ type: "setParams", mode: boardMode, params: next as TargetParams[typeof boardMode] })
            }
          />
        </Disclosure>
      )}
      <Button size="sm" variant="ghost" className="self-start" onClick={() => dispatch({ type: "resetSettings" })}>
        Reset all settings
      </Button>
    </div>
  );
}
