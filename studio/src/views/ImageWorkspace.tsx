// Image workspace: interactive overlay canvas + a tabbed side panel
// (Detect — stats / run options / layers; Config — full ChessboardParams
// editor with named configs; Diagnose — per-stage pipeline introspection;
// Baseline — structured diff vs the pinned baseline). Param edits
// re-detect automatically (debounced).

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
  ToggleChip,
  type TabItem,
} from "@vitavision/ui";
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
import { CanvasViewport, type HitPoint } from "../components/CanvasViewport";
import { ConfigEditor } from "../components/ConfigEditor";
import { DiagnosePanel } from "../components/DiagnosePanel";
import {
  topoSplitLayer,
  TOPO_COLORS,
} from "../components/diagnoseOverlays";
import { BaselineDiffSummary } from "../components/BaselineDiffSummary";
import { PresetPicker } from "../components/PresetPicker";
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
import { eyebrow, quietLink } from "../theme/classes";

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

type HoverData =
  | { kind: "corner"; c: BaselineCorner }
  | {
      kind: "topo";
      c: { x: number; y: number; sigma0: number; sigma1: number; labelled: boolean };
    };

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
    const currentGroup = imgs.find((i) =>
      i.snaps.some((s) => s.label === label),
    )?.dataset;
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
  const nextLabel =
    pos >= 0 && pos < flatLabels.length - 1 ? (flatLabels[pos + 1] ?? null) : null;
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
        (el instanceof HTMLElement &&
          el.closest('[role="tablist"], [role="listbox"], [role="combobox"]'))
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
  const [visible, setVisible] = useState<Record<string, boolean>>({
    edges: true,
    corners: true,
    rings: true,
    ids: false,
    "baseline-diff": true,
  });
  const [diagAlgorithm, setDiagAlgorithm] =
    useState<DiagnoseAlgorithm>("topological");
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
    corners,
    diff,
    baseline.data,
    visible,
  ]);

  const hitPoints: HitPoint<HoverData>[] = useMemo(() => {
    if (diagnoseMode && diagnose.data) {
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
    <div className="flex h-full">
      <div className="relative min-w-0 flex-1">
        {bitmap.error ? (
          <ErrorBox className="m-6">{String(bitmap.error)}</ErrorBox>
        ) : (
          <CanvasViewport
            image={bitmap.data ?? null}
            layers={layers}
            hitPoints={hitPoints}
            renderTooltip={(h) => <HoverTooltip h={h} />}
          />
        )}
      </div>

      <aside className="flex w-[330px] shrink-0 flex-col gap-4 overflow-y-auto border-l border-line bg-surface p-4 text-xs">
        <DensityProvider value="compact">
          <div>
            <div className="flex justify-between">
              <Link to="/" className={quietLink}>
                ← dataset
              </Link>
              <Link
                to={`/compare?label=${encodeURIComponent(label)}`}
                className={quietLink}
              >
                compare ⇄
              </Link>
            </div>
            <div className="mt-1 font-mono text-[13px] font-semibold break-all">
              {label}
            </div>
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
            items={
              detector === "chessboard"
                ? TABS
                : TABS.filter((t) => t.id !== "baseline")
            }
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
              />
            )}

            {tab === "baseline" &&
              (detect.data?.baseline?.exists && diff ? (
                <BaselineDiffSummary diff={diff} />
              ) : (
                <p className="text-fg-muted">
                  No baseline pinned for this snap. Baselines are blessed from
                  the bench CLI (<code className="font-mono">cargo bench-bless</code>);
                  the studio is read-only.
                </p>
              ))}
          </div>
        </DensityProvider>
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
        <div className="text-fg-muted">
          x {c.x.toFixed(2)} · y {c.y.toFixed(2)} · score {c.score.toFixed(1)}
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
      <div className="text-fg-muted">
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
  // Effective per-family defaults seed the basic-config placeholders, so the
  // shown values track the family the detector actually runs with (charuco /
  // puzzle pin a different strength floor + algorithm).
  const effectiveDefaults = useQuery({
    queryKey: ["effective-defaults", detector],
    queryFn: () => api.effectiveDefaults(detector),
    staleTime: Infinity,
  });
  const ed = effectiveDefaults.data;
  const layers: { id: string; label: string; swatch?: string }[] = [
    { id: "edges", label: "Grid edges", swatch: OVERLAY_COLORS.edge },
    { id: "corners", label: "Corners", swatch: OVERLAY_COLORS.corner },
    { id: "rings", label: "Origin / far rings", swatch: OVERLAY_COLORS.origin },
    { id: "ids", label: "(i, j) labels · zoom ≥ 2×" },
    { id: "baseline-diff", label: "Baseline diff", swatch: OVERLAY_COLORS.missing },
  ];
  const layerDefault = (id: string) => id !== "ids";
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
          <BoardForm
            detector={detector}
            board={board}
            setBoard={setBoard}
            sweep={sweep}
            setSweep={setSweep}
          />
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
        <div className="grid grid-cols-3 gap-2">
          <NumberRow
            label="Min strength"
            value={draft.min_corner_strength}
            placeholder={ed?.min_corner_strength}
            onChange={(v) => setDraftField({ min_corner_strength: v })}
          />
          <NumberRow
            label="Min labels"
            value={draft.min_labeled_corners}
            placeholder={ed?.min_labeled_corners}
            integer
            onChange={(v) => setDraftField({ min_labeled_corners: v })}
          />
          <NumberRow
            label="Max comps"
            value={draft.max_components}
            placeholder={ed?.max_components}
            integer
            onChange={(v) => setDraftField({ max_components: v })}
          />
        </div>
        <p className="text-[11px] leading-snug text-fg-muted">
          Placeholders are the <strong className="text-fg">{detector}</strong> defaults.
          charuco / puzzleboard pin a different strength floor — that is why the same
          image detects differently across families. Edit to override; clear to
          restore the default.
        </p>
      </Group>

      <Group title="Layers">
        <div className="flex flex-wrap gap-1.5">
          {layers.map((l) => {
            const checked = visible[l.id] ?? layerDefault(l.id);
            return (
              <ToggleChip
                key={l.id}
                checked={checked}
                {...(l.swatch !== undefined ? { swatch: l.swatch } : {})}
                onCheckedChange={(next) =>
                  setVisible((v) => ({ ...v, [l.id]: next }))
                }
              >
                {l.label}
              </ToggleChip>
            );
          })}
        </div>
      </Group>
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
  const num = (
    label: string,
    value: number | undefined,
    onChange: (v: number) => void,
    step = 1,
  ) => (
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
        {num(
          "Cell size",
          board.cell_size,
          (v) => setBoard({ ...board, cell_size: v }),
          0.001,
        )}
      </div>
      {detector === "charuco" && (
        <div className="grid grid-cols-[1fr_2fr] gap-2">
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
        </div>
      )}
      {detector === "puzzleboard" && (
        <div className="grid grid-cols-2 gap-2">
          {num("Origin row", board.origin_row, (v) =>
            setBoard({ ...board, origin_row: v }),
          )}
          {num("Origin col", board.origin_col, (v) =>
            setBoard({ ...board, origin_col: v }),
          )}
        </div>
      )}
      <Switch
        checked={sweep}
        onCheckedChange={setSweep}
        label="Multi-config sweep"
        description="detect_*_best"
      />
    </>
  );
}

function StatsBlock({
  detect,
}: {
  detect: { isLoading: boolean; error: unknown; data?: DetectResponse | undefined };
}) {
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
          <span title="estimated cell size">
            cell {d.detection.cell_size_px.toFixed(1)} px
          </span>
        </Badge>
      )}
      {d.baseline?.exists ? (
        passed ? (
          <Badge tone="normal" className={stat}>
            baseline PASS
            {diff && diff.extra_labels.length > 0
              ? `+${diff.extra_labels.length}`
              : ""}
          </Badge>
        ) : (
          <Badge tone="defect" className={stat}>
            baseline FAIL
            {diff &&
              ` · miss ${diff.missing_labels.length} · pos ${diff.wrong_position.length}`}
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
            <span title="decode bit error rate">
              BER {(d.info.decode.bit_error_rate * 100).toFixed(2)}%
            </span>
          </Badge>
          <Badge className={stat}>
            <span title="master pattern origin">
              origin ({d.info.decode.master_origin_row},{" "}
              {d.info.decode.master_origin_col})
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

function NumberRow({
  label,
  value,
  placeholder,
  integer,
  onChange,
}: {
  label: string;
  value: number | undefined;
  placeholder: number | undefined;
  integer?: boolean;
  onChange: (v: number | undefined) => void;
}) {
  return (
    <Field label={label}>
      <NumberInput
        step={integer ? 1 : "any"}
        value={value ?? ""}
        placeholder={placeholder !== undefined ? String(placeholder) : ""}
        onChange={(e) => {
          if (e.target.value === "") {
            onChange(undefined);
            return;
          }
          const v = e.target.valueAsNumber;
          if (!Number.isNaN(v)) onChange(v);
        }}
      />
    </Field>
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
      <Select
        value={value}
        options={options.map((o) => ({ value: o, label: o }))}
        onValueChange={onChange}
      />
    </Field>
  );
}
