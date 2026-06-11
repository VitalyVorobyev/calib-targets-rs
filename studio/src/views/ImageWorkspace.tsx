// Image workspace: interactive overlay canvas + a tabbed side panel
// (Detect — stats / run options / layers; Config — full DetectorParams
// editor with named configs; Baseline — structured diff vs the pinned
// baseline). Param edits re-detect automatically (debounced).

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { api } from "../api/client";
import type {
  BaselineCorner,
  DetectorParamsOverride,
  DetectRequest,
  DetectResponse,
  EngineReq,
  GraphBuildAlgorithm,
  OrientationMethodReq,
  OrientationSource,
} from "../api/types";
import { CanvasViewport, type HitPoint } from "../components/CanvasViewport";
import { ConfigEditor } from "../components/ConfigEditor";
import { DiffTable } from "../components/DiffTable";
import { LayerToggles } from "../components/LayerToggles";
import {
  baselineDiffLayer,
  cornersLayer,
  edgesLayer,
  idsLayer,
  OVERLAY_COLORS,
  ringsLayer,
} from "../components/overlays";
import { useDebounced } from "../hooks/useDebounced";
import { useImageBitmap } from "../hooks/useImageBitmap";

interface RunOptions {
  engine: EngineReq;
  orientationMethod: OrientationMethodReq;
}

type Tab = "detect" | "config" | "baseline";

export function ImageWorkspace() {
  const label = useParams()["*"] ?? "";
  const [tab, setTab] = useState<Tab>("detect");
  const [draft, setDraft] = useState<DetectorParamsOverride>({});
  const [runOpts, setRunOpts] = useState<RunOptions>({
    engine: "pipeline",
    orientationMethod: "ring_fit",
  });
  const [visible, setVisible] = useState<Record<string, boolean>>({
    edges: true,
    corners: true,
    rings: true,
    ids: false,
    "baseline-diff": true,
  });

  const bitmap = useImageBitmap(label);
  const debouncedDraft = useDebounced(draft, 400);

  const detectReq: DetectRequest = useMemo(
    () => ({
      label,
      engine: runOpts.engine,
      params: debouncedDraft,
      orientation_method: runOpts.orientationMethod,
      compare_baseline: true,
    }),
    [label, runOpts, debouncedDraft],
  );

  const detect = useQuery({
    queryKey: ["detect", detectReq],
    queryFn: () => api.detect(detectReq),
    placeholderData: (prev) => prev,
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

  const setDraftField = (patch: Partial<DetectorParamsOverride>) =>
    setDraft((d) => ({ ...d, ...patch }));

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
          width: 320,
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
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <Link to="/" style={{ fontSize: 12 }}>
              ← dataset
            </Link>
            <Link
              to={`/compare?label=${encodeURIComponent(label)}`}
              style={{ fontSize: 12 }}
            >
              compare ⇄
            </Link>
          </div>
          <div
            className="mono"
            style={{ fontWeight: 600, marginTop: 4, wordBreak: "break-all" }}
          >
            {label}
          </div>
        </div>

        <StatsBlock detect={detect} />

        <div
          style={{
            display: "flex",
            borderBottom: "1px solid var(--border)",
            gap: 2,
          }}
        >
          {(["detect", "config", "baseline"] as Tab[]).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              style={{
                padding: "6px 12px",
                background: "transparent",
                border: "none",
                borderBottom:
                  tab === t
                    ? "2px solid var(--accent)"
                    : "2px solid transparent",
                color: tab === t ? "var(--text)" : "var(--text-muted)",
                cursor: "pointer",
                fontSize: 12,
                fontWeight: tab === t ? 600 : 400,
              }}
            >
              {t[0].toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>

        {tab === "detect" && (
          <>
            <div>
              <div className="label" style={{ marginBottom: "var(--s2)" }}>
                Run options
              </div>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: "var(--s2)",
                }}
              >
                <SelectRow
                  label="Algorithm"
                  value={draft.graph_build_algorithm ?? "topological"}
                  options={["topological", "seed_and_grow"]}
                  onChange={(v) =>
                    setDraftField({
                      graph_build_algorithm: v as GraphBuildAlgorithm,
                    })
                  }
                />
                <SelectRow
                  label="Engine"
                  value={runOpts.engine}
                  options={["pipeline", "grid"]}
                  onChange={(v) =>
                    setRunOpts({ ...runOpts, engine: v as EngineReq })
                  }
                />
                <SelectRow
                  label="Orientation"
                  value={draft.orientation_source ?? "chess_axes"}
                  options={["chess_axes", "neighbour_edges"]}
                  disabledOptions={
                    (draft.graph_build_algorithm ?? "topological") ===
                      "seed_and_grow" && runOpts.engine === "pipeline"
                      ? ["neighbour_edges"]
                      : []
                  }
                  onChange={(v) =>
                    setDraftField({
                      orientation_source: v as OrientationSource,
                    })
                  }
                />
                <SelectRow
                  label="Axis fit"
                  value={runOpts.orientationMethod}
                  options={["ring_fit", "disk_fit"]}
                  onChange={(v) =>
                    setRunOpts({
                      ...runOpts,
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
          </>
        )}

        {tab === "config" && (
          <ConfigEditor draft={draft} onChange={setDraft} />
        )}

        {tab === "baseline" && (
          <div>
            {detect.data?.baseline?.exists && diff ? (
              <DiffTable diff={diff} />
            ) : (
              <div style={{ color: "var(--text-muted)", fontSize: 12 }}>
                No baseline pinned for this snap. Baselines are blessed from
                the bench CLI (<code>cargo bench-bless</code>); the studio is
                read-only.
              </div>
            )}
          </div>
        )}
      </aside>
    </div>
  );
}

function StatsBlock({
  detect,
}: {
  detect: { isLoading: boolean; error: unknown; data?: DetectResponse };
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
