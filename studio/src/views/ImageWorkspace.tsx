// Image workspace: interactive overlay canvas + detection side panel.
// Later milestones add the Config, Diagnose, and Baseline tabs.

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { api } from "../api/client";
import type {
  BaselineCorner,
  DetectRequest,
  DetectorParamsOverride,
  EngineReq,
  GraphBuildAlgorithm,
  OrientationMethodReq,
  OrientationSource,
} from "../api/types";
import { CanvasViewport, type HitPoint } from "../components/CanvasViewport";
import { LayerToggles } from "../components/LayerToggles";
import { useImageBitmap } from "../hooks/useImageBitmap";
import {
  baselineDiffLayer,
  cornersLayer,
  edgesLayer,
  idsLayer,
  OVERLAY_COLORS,
  ringsLayer,
} from "../components/overlays";

interface RunOptions {
  engine: EngineReq;
  algorithm: GraphBuildAlgorithm;
  orientationSource: OrientationSource;
  orientationMethod: OrientationMethodReq;
}

const DEFAULT_RUN_OPTIONS: RunOptions = {
  engine: "pipeline",
  algorithm: "topological",
  orientationSource: "chess_axes",
  orientationMethod: "ring_fit",
};

export function ImageWorkspace() {
  const label = useParams()["*"] ?? "";
  const [run, setRun] = useState<RunOptions>(DEFAULT_RUN_OPTIONS);
  const [visible, setVisible] = useState<Record<string, boolean>>({
    edges: true,
    corners: true,
    rings: true,
    ids: false,
    "baseline-diff": true,
  });

  const bitmap = useImageBitmap(label);

  const params: DetectorParamsOverride = useMemo(
    () => ({
      graph_build_algorithm: run.algorithm,
      orientation_source: run.orientationSource,
    }),
    [run.algorithm, run.orientationSource],
  );

  const detectReq: DetectRequest = useMemo(
    () => ({
      label,
      engine: run.engine,
      params,
      orientation_method: run.orientationMethod,
      compare_baseline: true,
    }),
    [label, run.engine, params, run.orientationMethod],
  );

  const detect = useQuery({
    queryKey: ["detect", detectReq],
    queryFn: () => api.detect(detectReq),
  });

  const baseline = useQuery({
    queryKey: ["baseline", label],
    queryFn: () => api.baseline(label),
    retry: false,
  });

  const corners = detect.data?.detection?.corners ?? [];
  const diff = detect.data?.baseline?.diff ?? null;

  const layers = useMemo(
    () => [
      edgesLayer(corners, visible["edges"] ?? true),
      cornersLayer(corners, visible["corners"] ?? true),
      ringsLayer(corners, visible["rings"] ?? true),
      idsLayer(corners, visible["ids"] ?? false),
      baselineDiffLayer(
        diff,
        baseline.data?.corners ?? null,
        corners,
        visible["baseline-diff"] ?? true,
      ),
    ],
    [corners, diff, baseline.data, visible],
  );

  const hitPoints: HitPoint<BaselineCorner>[] = useMemo(
    () => corners.map((c) => ({ x: c.x, y: c.y, data: c })),
    [corners],
  );

  return (
    <div style={{ display: "flex", height: "100%" }}>
      <div style={{ flex: 1, minWidth: 0, position: "relative" }}>
        {bitmap.error ? (
          <div style={{ padding: "var(--s5)", color: "var(--err)" }}>
            {String(bitmap.error)}
          </div>
        ) : (
          <CanvasViewport
            image={bitmap.data ?? null}
            layers={layers}
            hitPoints={hitPoints}
            renderTooltip={(c) => (
              <>
                <div>
                  (i, j) = ({c.i}, {c.j}){c.id != null && <> · id {c.id}</>}
                </div>
                <div style={{ color: "var(--text-muted)" }}>
                  x {c.x.toFixed(2)} · y {c.y.toFixed(2)} · score{" "}
                  {c.score.toFixed(1)}
                </div>
              </>
            )}
          />
        )}
      </div>

      <aside
        style={{
          width: 300,
          flexShrink: 0,
          borderLeft: "1px solid var(--border)",
          background: "var(--bg1)",
          overflowY: "auto",
          padding: "var(--s4)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--s4)",
        }}
      >
        <div>
          <Link to="/" style={{ fontSize: 12 }}>
            ← dataset
          </Link>
          <div
            className="mono"
            style={{ fontWeight: 600, marginTop: 4, wordBreak: "break-all" }}
          >
            {label}
          </div>
        </div>

        <StatsBlock detect={detect} />

        <div>
          <div className="label" style={{ marginBottom: "var(--s2)" }}>
            Run options
          </div>
          <div
            style={{ display: "flex", flexDirection: "column", gap: "var(--s2)" }}
          >
            <SelectRow
              label="Algorithm"
              value={run.algorithm}
              options={["topological", "seed_and_grow"]}
              onChange={(v) =>
                setRun({ ...run, algorithm: v as GraphBuildAlgorithm })
              }
            />
            <SelectRow
              label="Engine"
              value={run.engine}
              options={["pipeline", "grid"]}
              onChange={(v) => setRun({ ...run, engine: v as EngineReq })}
            />
            <SelectRow
              label="Orientation"
              value={run.orientationSource}
              options={["chess_axes", "neighbour_edges"]}
              disabledOptions={
                run.algorithm === "seed_and_grow" && run.engine === "pipeline"
                  ? ["neighbour_edges"]
                  : []
              }
              onChange={(v) =>
                setRun({ ...run, orientationSource: v as OrientationSource })
              }
            />
            <SelectRow
              label="Axis fit"
              value={run.orientationMethod}
              options={["ring_fit", "disk_fit"]}
              onChange={(v) =>
                setRun({
                  ...run,
                  orientationMethod: v as OrientationMethodReq,
                })
              }
            />
          </div>
        </div>

        <div>
          <div className="label" style={{ marginBottom: "var(--s2)" }}>
            Layers
          </div>
          <LayerToggles
            toggles={[
              {
                id: "edges",
                label: "Grid edges",
                checked: visible["edges"] ?? true,
                swatch: OVERLAY_COLORS.edge,
              },
              {
                id: "corners",
                label: "Corners",
                checked: visible["corners"] ?? true,
                swatch: OVERLAY_COLORS.corner,
              },
              {
                id: "rings",
                label: "Origin / far rings",
                checked: visible["rings"] ?? true,
                swatch: OVERLAY_COLORS.origin,
              },
              {
                id: "ids",
                label: "(i, j) labels · zoom ≥ 2×",
                checked: visible["ids"] ?? false,
              },
              {
                id: "baseline-diff",
                label: "Baseline diff",
                checked: visible["baseline-diff"] ?? true,
                swatch: OVERLAY_COLORS.missing,
              },
            ]}
            onChange={(id, checked) =>
              setVisible((v) => ({ ...v, [id]: checked }))
            }
          />
        </div>
      </aside>
    </div>
  );
}

function StatsBlock({
  detect,
}: {
  detect: ReturnType<typeof useQuery<import("../api/types").DetectResponse>>;
}) {
  if (detect.isLoading) {
    return <div style={{ color: "var(--text-muted)" }}>detecting…</div>;
  }
  if (detect.error) {
    return (
      <div style={{ color: "var(--err)", fontSize: 12 }}>
        {String(detect.error)}
      </div>
    );
  }
  const d = detect.data;
  if (!d) return null;
  const diff = d.baseline?.diff;
  const passed =
    diff != null &&
    diff.missing_labels.length === 0 &&
    diff.wrong_position.length === 0 &&
    diff.wrong_id.length === 0 &&
    !diff.inconsistent_shift &&
    diff.duplicate_run_positions.length === 0;
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--s1)" }}>
      <span className="chip" title="labelled corners">
        {d.detection?.labelled_count ?? 0} corners
      </span>
      <span className="chip" title="detection time">
        {d.elapsed_ms.toFixed(1)} ms
      </span>
      {d.detection != null && d.detection.cell_size_px > 0 && (
        <span className="chip" title="estimated cell size">
          cell {d.detection.cell_size_px.toFixed(1)} px
        </span>
      )}
      {d.baseline?.exists ? (
        passed ? (
          <span className="chip ok">
            baseline PASS
            {diff && diff.extra_labels.length > 0
              ? `+${diff.extra_labels.length}`
              : ""}
          </span>
        ) : (
          <span className="chip err">
            baseline FAIL
            {diff &&
              ` · miss ${diff.missing_labels.length} · pos ${diff.wrong_position.length}`}
          </span>
        )
      ) : (
        <span className="chip">no baseline</span>
      )}
      {d.detection == null && <span className="chip err">no detection</span>}
    </div>
  );
}

function SelectRow({
  label,
  value,
  options,
  disabledOptions = [],
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  disabledOptions?: string[];
  onChange: (v: string) => void;
}) {
  return (
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
      {label}
      <select
        className="select"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((o) => (
          <option key={o} value={o} disabled={disabledOptions.includes(o)}>
            {o}
          </option>
        ))}
      </select>
    </label>
  );
}
