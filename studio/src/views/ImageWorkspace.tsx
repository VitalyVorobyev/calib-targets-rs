// Image workspace: interactive overlay canvas + a tabbed side panel
// (Detect — stats / run options / layers; Config — full DetectorParams
// editor with named configs; Diagnose — per-stage pipeline introspection;
// Baseline — structured diff vs the pinned baseline). Param edits
// re-detect automatically (debounced).

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { api } from "../api/client";
import {
  stageDetail,
  stageName,
  type BaselineCorner,
  type BoardReq,
  type CornerAugWire,
  type DetectorParamsOverride,
  type DetectorReq,
  type DetectRequest,
  type DetectResponse,
  type DiagnoseAlgorithm,
  type EngineReq,
  type GraphBuildAlgorithm,
  type OrientationMethodReq,
  type OrientationSource,
} from "../api/types";
import { CanvasViewport, type HitPoint } from "../components/CanvasViewport";
import { ConfigEditor } from "../components/ConfigEditor";
import { DiagnosePanel } from "../components/DiagnosePanel";
import {
  axesLayer,
  stageMarkersLayer,
  STAGE_COLORS,
  topoSplitLayer,
  TOPO_COLORS,
} from "../components/diagnoseOverlays";
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

type Tab = "detect" | "config" | "diagnose" | "baseline";

type HoverData =
  | { kind: "corner"; c: BaselineCorner }
  | { kind: "aug"; c: CornerAugWire }
  | {
      kind: "topo";
      c: { x: number; y: number; sigma0: number; sigma1: number; labelled: boolean };
    };

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
  const [diagAlgorithm, setDiagAlgorithm] =
    useState<DiagnoseAlgorithm>("topological");
  const [axesOn, setAxesOn] = useState(false);
  const [stageVis, setStageVis] = useState<Record<string, boolean>>({});
  const [detector, setDetector] = useState<DetectorReq>("chessboard");
  const [board, setBoard] = useState<BoardReq>({
    rows: 22,
    cols: 22,
    cell_size: 1.0,
    marker_size_rel: 0.75,
    dictionary: "DICT_4X4_1000",
    origin_row: 0,
    origin_col: 0,
  });
  const [sweep, setSweep] = useState(false);
  const debouncedBoard = useDebounced(board, 400);

  const bitmap = useImageBitmap(label);
  const debouncedDraft = useDebounced(draft, 400);

  const detectReq: DetectRequest = useMemo(
    () => ({
      label,
      detector,
      board: detector === "chessboard" ? undefined : debouncedBoard,
      engine: runOpts.engine,
      params: debouncedDraft,
      orientation_method: runOpts.orientationMethod,
      compare_baseline: detector === "chessboard",
      sweep: detector !== "chessboard" && sweep,
    }),
    [label, detector, debouncedBoard, runOpts, debouncedDraft, sweep],
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

  const diagnose = useQuery({
    queryKey: [
      "diagnose",
      label,
      diagAlgorithm,
      debouncedDraft,
      runOpts.orientationMethod,
    ],
    enabled: tab === "diagnose",
    placeholderData: (prev) => prev,
    queryFn: () =>
      api.diagnose({
        label,
        algorithm: diagAlgorithm,
        params: debouncedDraft,
        orientation_method: runOpts.orientationMethod,
      }),
  });

  const corners = detect.data?.detection?.corners ?? [];
  const diff = detect.data?.baseline?.diff ?? null;
  const diagnoseMode = tab === "diagnose" && diagnose.data != null;

  const layers = useMemo(() => {
    if (diagnoseMode && diagnose.data) {
      if (diagnose.data.kind === "seed_and_grow") {
        return [
          stageMarkersLayer(diagnose.data.frame.corners, stageVis, true),
          axesLayer(diagnose.data.frame.corners, axesOn),
        ];
      }
      return [topoSplitLayer(diagnose.data.diagnosis, true)];
    }
    return [
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
    ];
  }, [
    diagnoseMode,
    diagnose.data,
    stageVis,
    axesOn,
    corners,
    diff,
    baseline.data,
    visible,
  ]);

  const hitPoints: HitPoint<HoverData>[] = useMemo(() => {
    if (diagnoseMode && diagnose.data) {
      if (diagnose.data.kind === "seed_and_grow") {
        return diagnose.data.frame.corners.map((c) => ({
          x: c.position[0],
          y: c.position[1],
          data: { kind: "aug", c },
        }));
      }
      return diagnose.data.diagnosis.corners.map((c) => ({
        x: c.x,
        y: c.y,
        data: { kind: "topo", c },
      }));
    }
    return corners.map((c) => ({
      x: c.x,
      y: c.y,
      data: { kind: "corner", c },
    }));
  }, [diagnoseMode, diagnose.data, corners]);

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
            renderTooltip={(h) => <HoverTooltip h={h} />}
          />
        )}
      </div>

      <aside
        style={{
          width: 330,
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
          {(detector === "chessboard"
            ? (["detect", "config", "diagnose", "baseline"] as Tab[])
            : (["detect", "config", "diagnose"] as Tab[])
          ).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              style={{
                padding: "6px 10px",
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
          <DetectTab
            draft={draft}
            setDraft={setDraft}
            runOpts={runOpts}
            setRunOpts={setRunOpts}
            visible={visible}
            setVisible={setVisible}
            detector={detector}
            setDetector={setDetector}
            board={board}
            setBoard={setBoard}
            sweep={sweep}
            setSweep={setSweep}
          />
        )}

        {tab === "config" && <ConfigEditor draft={draft} onChange={setDraft} />}

        {tab === "diagnose" && (
          <DiagnosePanel
            data={diagnose.data}
            isLoading={diagnose.isLoading}
            error={diagnose.error}
            algorithm={diagAlgorithm}
            onAlgorithm={setDiagAlgorithm}
            axesOn={axesOn}
            onAxes={setAxesOn}
            stageVis={stageVis}
            onStageVis={setStageVis}
          />
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

function HoverTooltip({ h }: { h: HoverData }) {
  if (h.kind === "corner") {
    const c = h.c;
    return (
      <>
        <div>
          (i, j) = ({c.i}, {c.j}){c.id != null && <> · id {c.id}</>}
        </div>
        <div style={{ color: "var(--text-muted)" }}>
          x {c.x.toFixed(2)} · y {c.y.toFixed(2)} · score {c.score.toFixed(1)}
        </div>
      </>
    );
  }
  if (h.kind === "aug") {
    const c = h.c;
    const name = stageName(c.stage);
    const detail = stageDetail(c.stage);
    return (
      <>
        <div style={{ color: STAGE_COLORS[name] }}>
          ● {name}
          <span style={{ color: "var(--text-muted)" }}> #{c.input_index}</span>
        </div>
        {detail && (
          <div style={{ maxWidth: 320, whiteSpace: "normal" }}>{detail}</div>
        )}
        <div style={{ color: "var(--text-muted)" }}>
          strength {c.strength.toFixed(0)} · σ (
          {((c.axes[0].sigma * 180) / Math.PI).toFixed(1)}°,{" "}
          {((c.axes[1].sigma * 180) / Math.PI).toFixed(1)}°)
        </div>
      </>
    );
  }
  const c = h.c;
  return (
    <>
      <div
        style={{ color: c.labelled ? TOPO_COLORS.labelled : TOPO_COLORS.dropped }}
      >
        ● {c.labelled ? "labelled" : "dropped"}
      </div>
      <div style={{ color: "var(--text-muted)" }}>
        σ ({((c.sigma0 * 180) / Math.PI).toFixed(1)}°,{" "}
        {((c.sigma1 * 180) / Math.PI).toFixed(1)}°)
      </div>
    </>
  );
}

function DetectTab({
  draft,
  setDraft,
  runOpts,
  setRunOpts,
  visible,
  setVisible,
  detector,
  setDetector,
  board,
  setBoard,
  sweep,
  setSweep,
}: {
  draft: DetectorParamsOverride;
  setDraft: (d: DetectorParamsOverride) => void;
  runOpts: RunOptions;
  setRunOpts: (r: RunOptions) => void;
  visible: Record<string, boolean>;
  setVisible: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  detector: DetectorReq;
  setDetector: (d: DetectorReq) => void;
  board: BoardReq;
  setBoard: (b: BoardReq) => void;
  sweep: boolean;
  setSweep: (s: boolean) => void;
}) {
  const setDraftField = (patch: Partial<DetectorParamsOverride>) =>
    setDraft({ ...draft, ...patch });
  return (
    <>
      <div>
        <div className="label" style={{ marginBottom: "var(--s2)" }}>
          Target
        </div>
        <div
          style={{ display: "flex", flexDirection: "column", gap: "var(--s2)" }}
        >
          <SelectRow
            label="Family"
            value={detector}
            options={["chessboard", "charuco", "puzzleboard"]}
            onChange={(v) => setDetector(v as DetectorReq)}
          />
          {detector !== "chessboard" && (
            <BoardForm
              detector={detector}
              board={board}
              setBoard={setBoard}
              sweep={sweep}
              setSweep={setSweep}
            />
          )}
        </div>
      </div>
      <div>
        <div className="label" style={{ marginBottom: "var(--s2)" }}>
          Run options
        </div>
        <div
          style={{ display: "flex", flexDirection: "column", gap: "var(--s2)" }}
        >
          <SelectRow
            label="Algorithm"
            value={draft.graph_build_algorithm ?? "topological"}
            options={["topological", "seed_and_grow"]}
            onChange={(v) =>
              setDraftField({ graph_build_algorithm: v as GraphBuildAlgorithm })
            }
          />
          <SelectRow
            label="Engine"
            value={runOpts.engine}
            options={["pipeline", "grid"]}
            onChange={(v) => setRunOpts({ ...runOpts, engine: v as EngineReq })}
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
              setDraftField({ orientation_source: v as OrientationSource })
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
  );
}

function BoardForm({
  detector,
  board,
  setBoard,
  sweep,
  setSweep,
}: {
  detector: DetectorReq;
  board: BoardReq;
  setBoard: (b: BoardReq) => void;
  sweep: boolean;
  setSweep: (s: boolean) => void;
}) {
  const num = (
    label: string,
    value: number | undefined,
    onChange: (v: number) => void,
    step = 1,
  ) => (
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
      <input
        className="input"
        type="number"
        step={step}
        value={value ?? ""}
        onChange={(e) => {
          const v = e.target.valueAsNumber;
          if (!Number.isNaN(v)) onChange(v);
        }}
      />
    </label>
  );
  return (
    <>
      {num("Rows", board.rows, (v) => setBoard({ ...board, rows: v }))}
      {num("Cols", board.cols, (v) => setBoard({ ...board, cols: v }))}
      {num(
        "Cell size",
        board.cell_size,
        (v) => setBoard({ ...board, cell_size: v }),
        0.001,
      )}
      {detector === "charuco" && (
        <>
          {num(
            "Marker rel",
            board.marker_size_rel,
            (v) => setBoard({ ...board, marker_size_rel: v }),
            0.05,
          )}
          <SelectRow
            label="Dictionary"
            value={board.dictionary ?? "DICT_4X4_1000"}
            options={[
              "DICT_4X4_50",
              "DICT_4X4_100",
              "DICT_4X4_250",
              "DICT_4X4_1000",
              "DICT_5X5_50",
              "DICT_5X5_100",
              "DICT_5X5_250",
              "DICT_5X5_1000",
              "DICT_6X6_50",
              "DICT_6X6_100",
              "DICT_6X6_250",
              "DICT_6X6_1000",
              "DICT_APRILTAG_36h11",
            ]}
            onChange={(v) => setBoard({ ...board, dictionary: v })}
          />
        </>
      )}
      {detector === "puzzleboard" && (
        <>
          {num("Origin row", board.origin_row, (v) =>
            setBoard({ ...board, origin_row: v }),
          )}
          {num("Origin col", board.origin_col, (v) =>
            setBoard({ ...board, origin_col: v }),
          )}
        </>
      )}
      <label
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          fontSize: 12,
          cursor: "pointer",
          color: "var(--text-muted)",
        }}
      >
        <input
          type="checkbox"
          checked={sweep}
          onChange={(e) => setSweep(e.target.checked)}
          style={{ accentColor: "var(--accent)" }}
        />
        Multi-config sweep (detect_*_best)
      </label>
    </>
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
      {d.info?.markers != null && (
        <span className="chip" title="decoded ArUco markers">
          {d.info.markers} markers
        </span>
      )}
      {d.info?.decode != null && (
        <>
          <span className="chip" title="decode bit error rate">
            BER {(d.info.decode.bit_error_rate * 100).toFixed(2)}%
          </span>
          <span className="chip" title="master pattern origin">
            origin ({d.info.decode.master_origin_row},{" "}
            {d.info.decode.master_origin_col})
          </span>
        </>
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
