import { Button } from "@vitavision/ui";

import { useActions, useAppState } from "../state/AppContext";

export function RunBar() {
  const { image, chess, params, run, auto } = useAppState();
  const { detect, cancel } = useActions();
  const running = run.status === "running";
  const ready = image !== null && chess !== null && params !== null;
  return (
    <div className="flex gap-2">
      <Button
        variant="primary"
        className="flex-1"
        disabled={!ready || running}
        loading={running}
        onClick={detect}
        title={ready ? undefined : image === null ? "Open an image first" : "Loading the detectors"}
      >
        {running ? "Detecting…" : auto ? "Detect now" : "Detect"}
      </Button>
      {running && (
        <Button variant="secondary" onClick={cancel}>
          Cancel
        </Button>
      )}
    </div>
  );
}
