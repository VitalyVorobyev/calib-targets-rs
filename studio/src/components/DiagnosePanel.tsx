// Diagnose tab content: stage-count chips + iteration traces for the
// seed-and-grow DebugFrame, or the prefilter funnel + component list for
// the topological diagnosis.

import { useState } from "react";
import type {
  DiagnoseAlgorithm,
  DiagnoseResponse,
  ExtensionTraceWire,
  IterationTraceWire,
  TopologicalDiagnosisWire,
} from "../api/types";
import { STAGE_COLORS, TOPO_COLORS } from "./diagnoseOverlays";

const STAGE_ORDER = [
  "Raw",
  "Strong",
  "NoCluster",
  "Clustered",
  "AttachmentAmbiguous",
  "AttachmentFailedInvariants",
  "Labeled",
  "LabeledThenBlacklisted",
] as const;

export function DiagnosePanel({
  data,
  isLoading,
  error,
  algorithm,
  onAlgorithm,
  axesOn,
  onAxes,
  stageVis,
  onStageVis,
}: {
  data: DiagnoseResponse | undefined;
  isLoading: boolean;
  error: unknown;
  algorithm: DiagnoseAlgorithm;
  onAlgorithm: (a: DiagnoseAlgorithm) => void;
  axesOn: boolean;
  onAxes: (on: boolean) => void;
  stageVis: Record<string, boolean>;
  onStageVis: (v: Record<string, boolean>) => void;
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
          <option value="seed_and_grow">seed_and_grow</option>
        </select>
      </label>
      <div style={{ fontSize: 11, color: "var(--text-faint)" }}>
        {algorithm === "seed_and_grow"
          ? "Full per-stage DebugFrame: stage cursors, iteration traces, homography residuals."
          : "Topological path exposes the prefilter funnel + labelled/unlabelled split (no per-stage trace by construction)."}
      </div>

      {isLoading && <div style={{ color: "var(--text-muted)" }}>running…</div>}
      {error != null && (
        <div style={{ color: "var(--err)", fontSize: 12 }}>{String(error)}</div>
      )}

      {data?.kind === "seed_and_grow" && (
        <SagPanel
          data={data}
          axesOn={axesOn}
          onAxes={onAxes}
          stageVis={stageVis}
          onStageVis={onStageVis}
        />
      )}
      {data?.kind === "topological" && <TopoPanel d={data.diagnosis} />}
    </div>
  );
}

// --- seed-and-grow ----------------------------------------------------------

function SagPanel({
  data,
  axesOn,
  onAxes,
  stageVis,
  onStageVis,
}: {
  data: Extract<DiagnoseResponse, { kind: "seed_and_grow" }>;
  axesOn: boolean;
  onAxes: (on: boolean) => void;
  stageVis: Record<string, boolean>;
  onStageVis: (v: Record<string, boolean>) => void;
}) {
  const f = data.frame;
  return (
    <>
      <div
        className="mono"
        style={{ fontSize: 11, color: "var(--text-muted)", lineHeight: 1.7 }}
      >
        input {f.input_count}
        {f.grid_directions && (
          <>
            {" "}
            · θ ({((f.grid_directions[0] * 180) / Math.PI).toFixed(1)}°,{" "}
            {((f.grid_directions[1] * 180) / Math.PI).toFixed(1)}°)
          </>
        )}
        {f.cell_size != null && <> · cell {f.cell_size.toFixed(1)} px</>}
        {f.seed && <> · seed [{f.seed.join(", ")}]</>}
      </div>

      <div>
        <div className="label" style={{ marginBottom: "var(--s2)" }}>
          Stages · click to toggle
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
          {STAGE_ORDER.filter((s) => data.stage_counts[s]).map((s) => {
            const on = stageVis[s] !== false;
            return (
              <button
                key={s}
                className="chip"
                onClick={() => onStageVis({ ...stageVis, [s]: !on })}
                style={{
                  cursor: "pointer",
                  opacity: on ? 1 : 0.35,
                  borderColor: STAGE_COLORS[s],
                  color: "var(--text)",
                }}
                title={s}
              >
                <span
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: STAGE_COLORS[s],
                    display: "inline-block",
                    marginRight: 5,
                  }}
                />
                {abbrev(s)} {data.stage_counts[s]}
              </button>
            );
          })}
        </div>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            marginTop: "var(--s2)",
            fontSize: 12,
            cursor: "pointer",
            color: "var(--text-muted)",
          }}
        >
          <input
            type="checkbox"
            checked={axesOn}
            onChange={(e) => onAxes(e.target.checked)}
            style={{ accentColor: "var(--accent)" }}
          />
          Axis vectors{" "}
          <span style={{ color: "#ff9632" }}>axes[0]</span>
          <span style={{ color: "#32c8c8" }}>axes[1]</span>
        </label>
      </div>

      {f.iterations.length > 0 && (
        <div>
          <div className="label" style={{ marginBottom: "var(--s2)" }}>
            Validation iterations
          </div>
          <div
            style={{ display: "flex", flexDirection: "column", gap: 4 }}
          >
            {f.iterations.map((it) => (
              <IterationRow key={it.iter} it={it} />
            ))}
          </div>
        </div>
      )}

      {f.boosters != null && (
        <details>
          <summary
            className="label"
            style={{ cursor: "pointer", marginBottom: "var(--s1)" }}
          >
            Boosters
          </summary>
          <pre
            className="mono"
            style={{
              fontSize: 10,
              color: "var(--text-muted)",
              whiteSpace: "pre-wrap",
              margin: 0,
            }}
          >
            {JSON.stringify(f.boosters, null, 1)}
          </pre>
        </details>
      )}

      <div className="mono" style={{ fontSize: 11, color: "var(--text-muted)" }}>
        detection:{" "}
        {f.detection ? `${f.detection.corners.length} labelled` : "NONE"}
      </div>
    </>
  );
}

function IterationRow({ it }: { it: IterationTraceWire }) {
  const [open, setOpen] = useState(false);
  const traces: [string, ExtensionTraceWire | null | undefined][] = [
    ["stage6 extension", it.extension],
    ["stage6.5 rescue", it.rescue],
    ["extension2", it.extension2],
    ["rescue2", it.rescue2],
  ];
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: "var(--radius-sm)",
        overflow: "hidden",
      }}
    >
      <button
        onClick={() => setOpen(!open)}
        className="mono"
        style={{
          width: "100%",
          textAlign: "left",
          padding: "4px 8px",
          background: "var(--bg2)",
          border: "none",
          cursor: "pointer",
          fontSize: 11,
          color: "var(--text)",
        }}
      >
        {open ? "▾" : "▸"} iter {it.iter}: labelled {it.labelled_count} ·
        blacklist {it.new_blacklist.length} ·{" "}
        {it.converged ? "converged" : "not converged"}
      </button>
      {open && (
        <div style={{ padding: "6px 8px" }}>
          {traces.map(
            ([name, t]) =>
              t && (
                <div
                  key={name}
                  className="mono"
                  style={{
                    fontSize: 10,
                    color: "var(--text-muted)",
                    marginBottom: 4,
                    lineHeight: 1.6,
                  }}
                >
                  <span style={{ color: "var(--text)" }}>{name}</span>: trusted{" "}
                  {String(t.h_trusted)} · res med{" "}
                  {t.h_residual_median_px?.toFixed(2) ?? "—"} / max{" "}
                  {t.h_residual_max_px?.toFixed(2) ?? "—"} px · iters{" "}
                  {t.iterations} · attached {t.attached}
                  <br />
                  rej: no_cand {t.rejected_no_candidate} · ambig{" "}
                  {t.rejected_ambiguous} · label {t.rejected_label} · policy{" "}
                  {t.rejected_policy} · edge {t.rejected_edge}
                </div>
              ),
          )}
          {it.geometry_check != null && (
            <pre
              className="mono"
              style={{
                fontSize: 10,
                color: "var(--text-muted)",
                whiteSpace: "pre-wrap",
                margin: "4px 0 0",
              }}
            >
              geometry_check {JSON.stringify(it.geometry_check)}
            </pre>
          )}
        </div>
      )}
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

function abbrev(s: string): string {
  switch (s) {
    case "AttachmentAmbiguous":
      return "Ambig";
    case "AttachmentFailedInvariants":
      return "FailInv";
    case "LabeledThenBlacklisted":
      return "Blacklist";
    default:
      return s;
  }
}
