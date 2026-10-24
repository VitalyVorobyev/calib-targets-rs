import { Badge, Button, Callout, ErrorBox, Field, SegmentedControl, Skeleton } from "@vitavision/ui";
import { useMemo } from "react";

import { downloadText } from "../lib/download";
import { useAppState, useDispatch } from "../state/AppContext";
import type { DetectInput, DetectOutcome } from "../worker/protocol";
import { CornerTable } from "./CornerTable";
import { cornerRows, hasCovariance, itemFacts, summaryFacts } from "./model";

const ELLIPSE_SCALES = [10, 50, 200].map((scale) => ({ value: String(scale), label: `×${scale}` }));

function Facts({ facts }: { facts: [string, string][] }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
      {facts.map(([label, value]) => (
        <div key={label} className="contents">
          <dt className="text-fg-muted">{label}</dt>
          <dd className="font-mono text-fg">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function downloadResult(outcome: DetectOutcome, input: DetectInput): void {
  // The raw detector output, with what produced it so the file is reproducible.
  const body = { tool: "calib-targets", ...input, time_ms: outcome.timeMs, result: outcome.payload };
  downloadText(JSON.stringify(body, null, 2), `calib-targets-${input.mode}.json`, "application/json");
}

export function ResultsPanel() {
  const state = useAppState();
  const dispatch = useDispatch();
  const { run, selectedId } = state;
  const payload = run.status === "done" ? run.outcome.payload : null;
  const rows = useMemo(() => (payload === null ? [] : cornerRows(payload)), [payload]);

  if (run.status === "idle") {
    return (
      <p className="text-xs text-fg-muted">{state.image === null ? "Open an image to begin." : "Press Detect."}</p>
    );
  }
  if (run.status === "running") {
    return (
      <div className="flex flex-col gap-2" role="status" aria-label="Detecting">
        <Skeleton className="h-5 w-2/3" />
        <Skeleton className="h-4 w-1/2" />
      </div>
    );
  }
  if (run.status === "error") return <ErrorBox>{run.message}</ErrorBox>;
  if (payload === null) return null;

  const stale = run.rev !== state.rev;
  const covariance = hasCovariance(payload);
  const found = rows.length > 0;
  const facts = found ? summaryFacts(payload) : [];
  const selected = selectedId !== null ? itemFacts(payload, rows, selectedId) : null;
  const note = "note" in payload ? payload.note : undefined;
  const markers = payload.kind === "charuco" ? (payload.detection?.markers.length ?? 0) : null;

  return (
    <div className="flex flex-col gap-3" aria-label="Detection result">
      <div className="flex flex-wrap items-center gap-1.5" role="status">
        <Badge tone={found ? "normal" : "defect"}>{found ? `${rows.length} corners` : "No detection"}</Badge>
        {markers !== null && <Badge tone="neutral">{`${markers} markers`}</Badge>}
        <Badge tone="neutral">{`${run.outcome.timeMs.toFixed(1)} ms`}</Badge>
        {run.input.sweep && <Badge tone="info">sweep</Badge>}
        {stale && <Badge tone="warning">stale</Badge>}
      </div>

      {!found && (
        <Callout tone="warning" title="Nothing was labelled">
          {note ??
            "The detector found no board in this image with these settings. Try another target, or relax the settings."}
        </Callout>
      )}

      {facts.length > 0 && <Facts facts={facts} />}

      {covariance ? (
        <Field
          label="Uncertainty ellipse size"
          description="Drawn as 1σ times this factor; σ is a small fraction of a pixel."
        >
          <SegmentedControl
            value={String(state.ellipseScale)}
            options={ELLIPSE_SCALES}
            onValueChange={(value) => dispatch({ type: "setEllipseScale", scale: Number(value) })}
            size="sm"
          />
        </Field>
      ) : (
        found && (
          <p className="text-xs text-fg-muted">
            This result carries no position covariance, so there are no ellipses to draw.
          </p>
        )
      )}

      {selected && (
        <section aria-label="Selected item" className="rounded-control border border-line bg-raised p-2.5">
          <h3 className="mb-1 text-xs font-semibold text-fg">Selected {selectedId?.replace("-", " ")}</h3>
          <Facts facts={selected} />
        </section>
      )}

      {found && <CornerTable rows={rows} selectedId={selectedId} onSelect={(id) => dispatch({ type: "select", id })} />}

      <Button
        size="sm"
        variant="secondary"
        className="self-start"
        onClick={() => downloadResult(run.outcome, run.input)}
      >
        Download JSON
      </Button>
    </div>
  );
}
