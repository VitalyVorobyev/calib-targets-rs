import { ErrorBox } from "@vitavision/ui";
import { useMemo } from "react";

import { useAppState, useDispatch } from "../state/AppContext";
import { cornerRows, detectionOf, hasCovariance, itemLabel } from "../results/model";
import { ViewerStage } from "./ViewerStage";

export function Viewer() {
  const state = useAppState();
  const dispatch = useDispatch();
  const { image, run } = state;

  const payload = run.status === "done" ? run.outcome.payload : null;
  const detection = useMemo(() => (payload === null ? null : detectionOf(payload)), [payload]);
  const rows = useMemo(() => (payload === null ? [] : cornerRows(payload)), [payload]);
  const covariance = useMemo(() => payload !== null && hasCovariance(payload), [payload]);

  return (
    <div className="relative h-full w-full bg-canvas">
      {image === null ? (
        // On the canvas well, which is dark in both themes: label colour, not the chrome's muted text.
        <p className="grid h-full place-items-center px-6 text-center text-sm text-stage-label">
          {state.imageStatus.status === "loading"
            ? `Opening ${state.imageStatus.name}…`
            : "Drop an image anywhere, pick a sample, or generate a target."}
        </p>
      ) : (
        <ViewerStage
          key={image.key}
          image={image}
          detection={detection}
          covariance={covariance}
          layers={state.layers}
          onLayer={(layer, visible) => dispatch({ type: "setLayer", layer, visible })}
          ellipseScale={state.ellipseScale}
          selectedId={state.selectedId}
          onSelect={(id) => dispatch({ type: "select", id })}
          describe={(id) => (payload === null ? null : itemLabel(payload, rows, id))}
          running={run.status === "running"}
          stale={run.status === "done" && run.rev !== state.rev}
        />
      )}
      {state.imageStatus.status === "error" && (
        <div className="absolute inset-x-3 top-3">
          <ErrorBox>{state.imageStatus.message}</ErrorBox>
        </div>
      )}
    </div>
  );
}
