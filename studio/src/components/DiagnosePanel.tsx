// Diagnose tab content: prefilter funnel + labelled/unlabelled split for
// the topological diagnosis.

import { Badge, ErrorBox, Field, Select, cn } from "@vitavision/ui";
import type {
  DiagnoseAlgorithm,
  DiagnoseResponse,
  TopologicalDiagnosisWire,
} from "../api/types";
import { TOPO_COLORS } from "./diagnoseOverlays";
import { errorText } from "../api/client";
import { eyebrow } from "../theme/classes";

const ALGORITHMS = [{ value: "topological", label: "topological" }];

export function DiagnosePanel({
  data,
  isLoading,
  error,
  algorithm,
  onAlgorithm,
}: {
  data: DiagnoseResponse | undefined;
  isLoading: boolean;
  error: unknown;
  algorithm: DiagnoseAlgorithm;
  onAlgorithm: (a: DiagnoseAlgorithm) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <Field
        label="Diagnose"
        description="Topological path exposes the prefilter funnel + labelled/unlabelled split (no per-stage trace by construction)."
      >
        <Select
          value={algorithm}
          options={ALGORITHMS}
          onValueChange={(v) => onAlgorithm(v as DiagnoseAlgorithm)}
        />
      </Field>

      {isLoading && <div className="text-fg-muted">running…</div>}
      {error != null && <ErrorBox>{errorText(error)}</ErrorBox>}

      {data?.kind === "topological" && <TopoPanel d={data.diagnosis} />}
    </div>
  );
}

// --- topological -------------------------------------------------------------

function TopoPanel({ d }: { d: TopologicalDiagnosisWire }) {
  const deg = (rad: number) => ((rad * 180) / Math.PI).toFixed(1);
  const funnel = [
    ["input", d.input_count],
    ["strength", d.prefilter.survives_strength],
    ["axis σ", d.prefilter.survives_axis],
  ] as const;
  const labelled = d.labelled_indices.length;
  return (
    <>
      <div>
        <div className={cn(eyebrow, "mb-2")}>Pre-filter funnel</div>
        <div className="flex flex-col gap-0.5">
          {funnel.map(([name, count], k) => (
            <div key={name} className="flex items-center gap-2">
              <span className="w-16 shrink-0 font-mono text-[11px] text-fg-muted">{name}</span>
              <div className="min-w-0 flex-1">
                <div
                  className={cn(
                    "h-2.5 min-w-0.5 rounded-sm",
                    k === 0 ? "bg-line-strong" : "bg-signal",
                  )}
                  style={{ width: `${(count / Math.max(d.input_count, 1)) * 100}%` }}
                />
              </div>
              <span className="w-8 shrink-0 text-right font-mono text-[11px]">{count}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap gap-1">
        <Badge className="font-mono text-fg">
          <Dot color={TOPO_COLORS.labelled} /> labelled {labelled}
        </Badge>
        <Badge className="font-mono text-fg">
          <Dot color={TOPO_COLORS.dropped} /> dropped {d.input_count - labelled}
        </Badge>
      </div>

      <div className="font-mono text-[11px] leading-relaxed text-fg-muted">
        axis_align {deg(d.effective_tols.axis_align_tol_rad)}° · max σ{" "}
        {deg(d.effective_tols.max_axis_sigma_rad)}° · cluster{" "}
        {deg(d.effective_tols.cluster_axis_tol_rad)}° · edge_max ×
        {d.effective_tols.edge_length_max_rel.toFixed(2)}
      </div>

      <div>
        <div className={cn(eyebrow, "mb-2")}>Components ({d.components.length})</div>
        {d.components.map((c, k) => (
          // Components are positional (#k is their index in the diagnosis).
          // eslint-disable-next-line @eslint-react/no-array-index-key -- positional list, see above
          <div key={k} className="font-mono text-[11px] text-fg-muted">
            #{k}: {c.labelled} corners · i[{c.bbox[0]}, {c.bbox[1]}] j[
            {c.bbox[2]}, {c.bbox[3]}] ({c.bbox[1] - c.bbox[0] + 1}×
            {c.bbox[3] - c.bbox[2] + 1})
          </div>
        ))}
      </div>
    </>
  );
}

/** A dot in a data colour: the colour the overlay draws that class in. */
function Dot({ color }: { color: string }) {
  return (
    <span
      aria-hidden
      className="inline-block size-2 rounded-full"
      style={{ backgroundColor: color }}
    />
  );
}
