// Image workspace: the image stage (stage2d) with the detection over it + a tabbed side panel
// (Detect: stats / run options / corner inspector; Config: full ChessboardParams
// editor with named configs; Diagnose: the topological prefilter funnel and labelled/dropped
// split; Baseline: structured diff vs the pinned baseline). Layers are toggled from the stage's
// own layers menu. Param edits re-detect automatically (debounced).

import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import {
  Badge,
  Button,
  DensityProvider,
  ErrorBox,
  Field,
  NumberInput,
  Select,
  Switch,
  Tabs,
  type TabItem,
} from "@vitavision/ui";
import { TargetOverlay, type TargetHit } from "@vitavision/overlays";
import { api, encodeLabel, errorText } from "../api/client";
import {
  type BaselineCorner,
  type BoardReq,
  type DetectorParamsOverride,
  type DetectorReq,
  type DetectRequest,
  type DetectResponse,
  type DiagnoseAlgorithm,
  type EngineReq,
  type OrientationMethodReq,
} from "../api/types";
import { ConfigEditor } from "../components/ConfigEditor";
import { DiagnosePanel } from "../components/DiagnosePanel";
import { BaselineDiffSummary } from "../components/BaselineDiffSummary";
import { ParamsForm } from "../components/ParamsForm";
import { PresetPicker } from "../components/PresetPicker";
import { useDebounced } from "../hooks/useDebounced";
import { useImageSize } from "../hooks/useImageSize";
import { eyebrow, quietLink } from "../theme/classes";
import { CornerInspector } from "../viewer/CornerInspector";
import { cornerOfId, describeCorner, hasCovariance, targetDetectionOf } from "../viewer/detection";
import {
  BaselineDiffLayers,
  DEFAULT_VISIBILITY,
  DiagnoseLayers,
  RingsLayer,
  detectMenu,
  diagnoseMenu,
  type LayerId,
  type Visibility,
} from "../viewer/layers";
import { StageNote, StudioStage, useHoverText } from "../viewer/StudioStage";

interface RunOptions {
  engine: EngineReq;
  orientationMethod: OrientationMethodReq;
}

type Tab = "detect" | "config" | "diagnose" | "baseline";

const TABS: TabItem<Tab>[] = [
  { id: "detect", label: "Detect" },
  { id: "config", label: "Config" },
  { id: "diagnose", label: "Diagnose" },
  { id: "baseline", label: "Baseline" },
];

export function ImageWorkspace() {
  const label = useParams()["*"] ?? "";
  const navigate = useNavigate();
  // Shares the cached ["dataset"] query with the browser — no extra fetch.
  const dataset = useQuery({ queryKey: ["dataset"], queryFn: api.dataset });

  // Flattened, manifest-order snap labels for prev/next — scoped to the
  // current snap's dataset group so ←/→ steps through (say) all 120 snaps of
  // 130x130_puzzle without crossing into another dataset. Falls back to the
  // full list when the group can't be resolved.
  const flatLabels = useMemo(() => {
    const imgs = dataset.data?.images ?? [];
    const currentGroup = imgs.find((i) => i.snaps.some((s) => s.label === label))?.dataset;
    const out: string[] = [];
    for (const img of imgs) {
      if (!img.available) continue;
      if (currentGroup && img.dataset !== currentGroup) continue;
      for (const snap of img.snaps) out.push(snap.label);
    }
    return out;
  }, [dataset.data, label]);
  const pos = flatLabels.indexOf(label);
  const prevLabel = pos > 0 ? (flatLabels[pos - 1] ?? null) : null;
  const nextLabel = pos >= 0 && pos < flatLabels.length - 1 ? (flatLabels[pos + 1] ?? null) : null;
  const go = useCallback(
    (l: string | null) => {
      if (l) void navigate(`/image/${encodeLabel(l)}`);
    },
    [navigate],
  );
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A control that uses the arrow keys itself (the tab strip, an open select, a
      // radio group) keeps them.
      if (e.defaultPrevented) return;
      const el = e.target;
      if (
        el instanceof HTMLInputElement ||
        el instanceof HTMLSelectElement ||
        el instanceof HTMLTextAreaElement ||
        (el instanceof HTMLElement && el.closest('[role="tablist"], [role="listbox"], [role="combobox"]'))
      )
        return;
      if (e.key === "ArrowLeft") go(prevLabel);
      else if (e.key === "ArrowRight") go(nextLabel);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go, prevLabel, nextLabel]);

  const [tab, setTab] = useState<Tab>("detect");
  const [draft, setDraft] = useState<DetectorParamsOverride>({});
  const [runOpts, setRunOpts] = useState<RunOptions>({
    engine: "pipeline",
    orientationMethod: "ring_fit",
  });
  const [visible, setVisible] = useState<Visibility>(DEFAULT_VISIBILITY);
  const [ellipseScale, setEllipseScale] = useState(50);
  const [selection, setSelection] = useState<{ id: string; corners: readonly BaselineCorner[] } | null>(null);
  const hovering = useHoverText();
  const [diagAlgorithm, setDiagAlgorithm] = useState<DiagnoseAlgorithm>("topological");
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

  const size = useImageSize(label);
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
    queryKey: ["diagnose", label, diagAlgorithm, debouncedDraft, runOpts.orientationMethod],
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

  const corners = useMemo(() => detect.data?.detection?.corners ?? [], [detect.data]);
  const diff = detect.data?.baseline?.diff ?? null;
  const diagnoseMode = tab === "diagnose" && diagnose.data != null;

  const detection = useMemo(() => targetDetectionOf(corners), [corners]);
  const covariance = useMemo(() => hasCovariance(corners), [corners]);
  const ellipses = visible.ellipses && covariance;
  const uncertainty = useMemo(() => (ellipses ? { scale: ellipseScale } : undefined), [ellipses, ellipseScale]);
  // A selection names a corner by its index in one result; a new result (other params, other
  // image) clears it rather than pointing at whatever now sits at that index.
  const selectedId = selection !== null && selection.corners === corners ? selection.id : null;
  const selectedCorner = selectedId === null ? null : cornerOfId(selectedId, corners);
  const selectedIds = useMemo(() => (selectedId === null ? [] : [selectedId]), [selectedId]);
  const menu = useMemo(
    () => (diagnoseMode ? diagnoseMenu(visible) : detectMenu(visible, { baseline: diff !== null, covariance })),
    [diagnoseMode, visible, diff, covariance],
  );
  const onLayer = (id: string, on: boolean) => setVisible((v) => ({ ...v, [id as LayerId]: on }));
  const reportCorner = hovering.reporter("corner");
  const onCornerHover = (hit: TargetHit | null) => {
    const c = hit === null ? null : cornerOfId(String(hit.id), corners);
    reportCorner(c === null ? null : describeCorner(c));
  };

  return (
    <div className="flex h-full">
      <div className="relative min-w-0 flex-1">
        {size.error ? (
          <ErrorBox className="m-6">{String(size.error)}</ErrorBox>
        ) : size.data ? (
          <StudioStage
            key={label}
            label={label}
            size={size.data}
            layers={menu}
            onLayer={onLayer}
            hover={hovering.text}
            onBackgroundClick={() => setSelection(null)}
            banner={ellipses ? <StageNote>{`Uncertainty ellipses: 1σ × ${ellipseScale}`}</StageNote> : undefined}
            busy={detect.isFetching || (diagnoseMode && diagnose.isFetching)}
          >
            {diagnoseMode && diagnose.data ? (
              <DiagnoseLayers
                diagnosis={diagnose.data.diagnosis}
                showLabelled={visible.labelled}
                showDropped={visible.dropped}
                onHover={hovering.reporter("diagnose")}
              />
            ) : (
              <>
                {visible.detection && (
                  <TargetOverlay
                    detection={detection}
                    showEdges={visible.edges}
                    showLabels={visible.labels}
                    selectedIds={selectedIds}
                    onHoverChange={onCornerHover}
                    onItemPress={(hit) => setSelection({ id: String(hit.id), corners })}
                    uncertainty={uncertainty}
                  />
                )}
                {visible.rings && <RingsLayer corners={corners} onHover={hovering.reporter("rings")} />}
                {visible.diff && diff && (
                  <BaselineDiffLayers
                    diff={diff}
                    baselineCorners={baseline.data?.corners ?? null}
                    runCorners={corners}
                    onHover={hovering.reporter("diff")}
                  />
                )}
              </>
            )}
          </StudioStage>
        ) : null}
      </div>

      <aside className="flex w-[330px] shrink-0 flex-col gap-4 overflow-y-auto border-l border-line bg-surface p-4 text-xs">
        <DensityProvider value="compact">
          <div>
            <div className="flex justify-between">
              <Link to="/" className={quietLink}>
                ← dataset
              </Link>
              <Link to={`/compare?label=${encodeURIComponent(label)}`} className={quietLink}>
                compare ⇄
              </Link>
            </div>
            <div className="mt-1 font-mono text-[13px] font-semibold break-all">{label}</div>
            {flatLabels.length > 1 && (
              <div className="mt-2 flex items-center gap-2">
                <Button
                  disabled={!prevLabel}
                  onClick={() => go(prevLabel)}
                  title="previous snap (←)"
                  aria-label="Previous snap"
                >
                  ←
                </Button>
                <span className="flex-1 text-center font-mono text-[11px] text-fg-muted">
                  {pos >= 0 ? `${pos + 1} / ${flatLabels.length}` : "—"}
                </span>
                <Button
                  disabled={!nextLabel}
                  onClick={() => go(nextLabel)}
                  title="next snap (→)"
                  aria-label="Next snap"
                >
                  →
                </Button>
              </div>
            )}
          </div>

          <StatsBlock detect={detect} />

          <Tabs
            label="Workspace panels"
            idPrefix="workspace"
            items={detector === "chessboard" ? TABS : TABS.filter((t) => t.id !== "baseline")}
            active={tab}
            onSelect={setTab}
            className="self-start"
          />

          <div
            role="tabpanel"
            id={`workspace-panel-${tab}`}
            aria-labelledby={`workspace-tab-${tab}`}
            className="flex flex-col gap-4"
          >
            {tab === "detect" && (
              <DetectTab
                draft={draft}
                setDraft={setDraft}
                runOpts={runOpts}
                setRunOpts={setRunOpts}
                covariance={covariance}
                ellipseScale={ellipseScale}
                setEllipseScale={setEllipseScale}
                selectedCorner={selectedCorner}
                detector={detector}
                setDetector={setDetector}
                board={board}
                setBoard={setBoard}
                sweep={sweep}
                setSweep={setSweep}
              />
            )}

            {tab === "config" && <ConfigEditor draft={draft} family={detector} onChange={setDraft} />}

            {tab === "diagnose" && (
              <DiagnosePanel
                data={diagnose.data}
                isLoading={diagnose.isLoading}
                error={diagnose.error}
                algorithm={diagAlgorithm}
                onAlgorithm={setDiagAlgorithm}
              />
            )}

            {tab === "baseline" &&
              (detect.data?.baseline?.exists && diff ? (
                <BaselineDiffSummary diff={diff} />
              ) : (
                <p className="text-fg-muted">
                  No baseline pinned for this snap. Baselines are blessed from the bench CLI (
                  <code className="font-mono">cargo bench-bless</code>); the studio is read-only.
                </p>
              ))}
          </div>
        </DensityProvider>
      </aside>
    </div>
  );
}

function DetectTab({
  draft,
  setDraft,
  runOpts,
  setRunOpts,
  covariance,
  ellipseScale,
  setEllipseScale,
  selectedCorner,
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
  covariance: boolean;
  ellipseScale: number;
  setEllipseScale: (scale: number) => void;
  selectedCorner: BaselineCorner | null;
  detector: DetectorReq;
  setDetector: (d: DetectorReq) => void;
  board: BoardReq;
  setBoard: (b: BoardReq) => void;
  sweep: boolean;
  setSweep: (s: boolean) => void;
}) {
  return (
    <>
      <PresetPicker onLoad={setDraft} />
      <Group title="Target">
        <SelectRow
          label="Family"
          value={detector}
          options={["chessboard", "charuco", "puzzleboard"]}
          onChange={(v) => setDetector(v as DetectorReq)}
        />
        {detector !== "chessboard" && (
          <BoardForm detector={detector} board={board} setBoard={setBoard} sweep={sweep} setSweep={setSweep} />
        )}
      </Group>
      <Group title="Run options">
        <div className="grid grid-cols-2 gap-2">
          <SelectRow
            label="Engine"
            value={runOpts.engine}
            options={["pipeline", "grid"]}
            onChange={(v) => setRunOpts({ ...runOpts, engine: v as EngineReq })}
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
      </Group>

      <Group title="Basic config">
        <ParamsForm draft={draft} family={detector} onChange={setDraft} part="core" />
        <div className="flex items-start justify-between gap-2">
          <p className="text-[11px] leading-snug text-fg-muted">
            Shown values are the <strong className="text-fg">{detector}</strong> defaults until edited. charuco /
            puzzleboard pin a different strength floor, which is why the same image detects differently across families.
          </p>
          <Button size="sm" disabled={Object.keys(draft).length === 0} onClick={() => setDraft({})}>
            Reset
          </Button>
        </div>
      </Group>

      {covariance && (
        <Group title="Uncertainty">
          <Field
            label="Ellipse scale"
            description="Ellipses are drawn at 1σ × this factor; σ is a small fraction of a pixel."
          >
            <NumberInput
              min={1}
              step={5}
              value={ellipseScale}
              onChange={(e) => {
                const v = e.target.valueAsNumber;
                if (Number.isFinite(v) && v > 0) setEllipseScale(v);
              }}
            />
          </Field>
        </Group>
      )}

      <CornerInspector corner={selectedCorner} />
    </>
  );
}

/** A titled group of controls in the side panel. */
function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className={eyebrow}>{title}</h3>
      {children}
    </section>
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
  const num = (label: string, value: number | undefined, onChange: (v: number) => void, step = 1) => (
    <Field label={label}>
      <NumberInput
        step={step}
        value={value ?? ""}
        onChange={(e) => {
          const v = e.target.valueAsNumber;
          if (!Number.isNaN(v)) onChange(v);
        }}
      />
    </Field>
  );
  return (
    <>
      <div className="grid grid-cols-3 gap-2">
        {num("Rows", board.rows, (v) => setBoard({ ...board, rows: v }))}
        {num("Cols", board.cols, (v) => setBoard({ ...board, cols: v }))}
        {num("Cell size", board.cell_size, (v) => setBoard({ ...board, cell_size: v }), 0.001)}
      </div>
      {detector === "charuco" && (
        <div className="grid grid-cols-[1fr_2fr] gap-2">
          {num("Marker rel", board.marker_size_rel, (v) => setBoard({ ...board, marker_size_rel: v }), 0.05)}
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
        </div>
      )}
      {detector === "puzzleboard" && (
        <div className="grid grid-cols-2 gap-2">
          {num("Origin row", board.origin_row, (v) => setBoard({ ...board, origin_row: v }))}
          {num("Origin col", board.origin_col, (v) => setBoard({ ...board, origin_col: v }))}
        </div>
      )}
      <Switch checked={sweep} onCheckedChange={setSweep} label="Multi-config sweep" description="detect_*_best" />
    </>
  );
}

function StatsBlock({ detect }: { detect: { isLoading: boolean; error: unknown; data?: DetectResponse | undefined } }) {
  if (detect.isLoading) {
    return <div className="text-fg-muted">detecting…</div>;
  }
  if (detect.error) {
    return <ErrorBox>{errorText(detect.error)}</ErrorBox>;
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
  const stat = "font-mono";
  return (
    <div className="flex flex-wrap gap-1">
      <Badge className={stat}>
        <span title="labelled corners">{d.detection?.labelled_count ?? 0} corners</span>
      </Badge>
      <Badge className={stat}>
        <span title="detection time">{d.elapsed_ms.toFixed(1)} ms</span>
      </Badge>
      {d.detection != null && d.detection.cell_size_px > 0 && (
        <Badge className={stat}>
          <span title="estimated cell size">cell {d.detection.cell_size_px.toFixed(1)} px</span>
        </Badge>
      )}
      {d.baseline?.exists ? (
        passed ? (
          <Badge tone="normal" className={stat}>
            baseline PASS
            {diff && diff.extra_labels.length > 0 ? `+${diff.extra_labels.length}` : ""}
          </Badge>
        ) : (
          <Badge tone="defect" className={stat}>
            baseline FAIL
            {diff && ` · miss ${diff.missing_labels.length} · pos ${diff.wrong_position.length}`}
          </Badge>
        )
      ) : (
        <Badge className={stat}>no baseline</Badge>
      )}
      {d.info?.markers != null && (
        <Badge className={stat}>
          <span title="decoded ArUco markers">{d.info.markers} markers</span>
        </Badge>
      )}
      {d.info?.decode != null && (
        <>
          <Badge className={stat}>
            <span title="decode bit error rate">BER {(d.info.decode.bit_error_rate * 100).toFixed(2)}%</span>
          </Badge>
          <Badge className={stat}>
            <span title="master pattern origin">
              origin ({d.info.decode.master_origin_row}, {d.info.decode.master_origin_col})
            </span>
          </Badge>
        </>
      )}
      {d.detection == null && (
        <Badge tone="defect" className={stat}>
          no detection
        </Badge>
      )}
    </div>
  );
}

function SelectRow({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <Field label={label}>
      <Select value={value} options={options.map((o) => ({ value: o, label: o }))} onValueChange={onChange} />
    </Field>
  );
}
