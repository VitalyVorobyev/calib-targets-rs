// Diagnose tab content: prefilter funnel + labelled/unlabelled split for
// the topological diagnosis.

import type {
  DiagnoseAlgorithm,
  DiagnoseResponse,
  TopologicalDiagnosisWire,
} from "../api/types";
import { TOPO_COLORS } from "./diagnoseOverlays";

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
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--s3)" }}>
      <label
        style={{
          display: "grid",
          gridTemplateColumns: "90px 1fr",
          alignItems: "center",
          gap: "var(--s2)",
          fontSize: 12,
          color: "var(--text-muted)",
        }}
      >
        Diagnose
        <select
          className="select"
          value={algorithm}
          onChange={(e) => onAlgorithm(e.target.value as DiagnoseAlgorithm)}
        >
          <option value="topological">topological</option>
        </select>
      </label>
      <div style={{ fontSize: 11, color: "var(--text-faint)" }}>
        Topological path exposes the prefilter funnel + labelled/unlabelled
        split (no per-stage trace by construction).
      </div>

      {isLoading && <div style={{ color: "var(--text-muted)" }}>running…</div>}
      {error != null && (
        <div style={{ color: "var(--err)", fontSize: 12 }}>{String(error)}</div>
      )}

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
    ["fit", d.prefilter.survives_fit],
    ["axis σ", d.prefilter.survives_axis],
  ] as const;
  const labelled = d.labelled_indices.length;
  return (
    <>
      <div>
        <div className="label" style={{ marginBottom: "var(--s2)" }}>
          Pre-filter funnel
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          {funnel.map(([name, count], k) => (
            <div
              key={name}
              style={{ display: "flex", alignItems: "center", gap: 8 }}
            >
              <span
                className="mono"
                style={{
                  width: 64,
                  fontSize: 11,
                  color: "var(--text-muted)",
                }}
              >
                {name}
              </span>
              <div
                style={{
                  height: 10,
                  borderRadius: 3,
                  width: `${(count / Math.max(d.input_count, 1)) * 100}%`,
                  minWidth: 2,
                  background:
                    k === 0 ? "var(--bg3)" : "var(--accent-dim)",
                }}
              />
              <span className="mono" style={{ fontSize: 11 }}>
                {count}
              </span>
            </div>
          ))}
        </div>
      </div>

      <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
        <span
          className="chip"
          style={{ borderColor: TOPO_COLORS.labelled, color: "var(--text)" }}
        >
          <Dot color={TOPO_COLORS.labelled} /> labelled {labelled}
        </span>
        <span
          className="chip"
          style={{ borderColor: TOPO_COLORS.dropped, color: "var(--text)" }}
        >
          <Dot color={TOPO_COLORS.dropped} /> dropped{" "}
          {d.input_count - labelled}
        </span>
      </div>

      <div className="mono" style={{ fontSize: 11, color: "var(--text-muted)", lineHeight: 1.7 }}>
        axis_align {deg(d.effective_tols.axis_align_tol_rad)}° · max σ{" "}
        {deg(d.effective_tols.max_axis_sigma_rad)}° · cluster{" "}
        {deg(d.effective_tols.cluster_axis_tol_rad)}° · edge_max ×
        {d.effective_tols.edge_length_max_rel.toFixed(2)}
      </div>

      <div>
        <div className="label" style={{ marginBottom: "var(--s2)" }}>
          Components ({d.components.length})
        </div>
        {d.components.map((c, k) => (
          <div
            key={k}
            className="mono"
            style={{ fontSize: 11, color: "var(--text-muted)" }}
          >
            #{k}: {c.labelled} corners · i[{c.bbox[0]}, {c.bbox[1]}] j[
            {c.bbox[2]}, {c.bbox[3]}] ({c.bbox[1] - c.bbox[0] + 1}×
            {c.bbox[3] - c.bbox[2] + 1})
          </div>
        ))}
      </div>
    </>
  );
}

function Dot({ color }: { color: string }) {
  return (
    <span
      style={{
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: color,
        display: "inline-block",
        marginRight: 5,
      }}
    />
  );
}
