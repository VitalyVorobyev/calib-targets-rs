import { StatusBar } from "@vitavision/workbench";

import { useAppState } from "./state/AppContext";

export function StatusLine() {
  const { image, mode, run } = useAppState();
  const result =
    run.status === "done"
      ? `${run.outcome.timeMs.toFixed(1)} ms`
      : run.status === "running"
        ? "running…"
        : run.status === "error"
          ? "failed"
          : null;
  return (
    <StatusBar
      live
      start={[
        { label: "image", value: image ? `${image.name} · ${image.width}×${image.height}` : "none" },
        { label: "target", value: mode },
      ]}
      end={[{ label: "detect", value: result }]}
    />
  );
}
