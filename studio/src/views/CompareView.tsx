// A/B comparison: run two configs on the same snap, side-by-side with
// synchronised zoom/pan, or as a single position-matched diff overlay
// (A-only / B-only / common), plus a metric delta strip.

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router";
import { Badge, Empty, SegmentedControl, Select, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type { BaselineCorner, DetectResponse, EngineReq, OrientationMethodReq } from "../api/types";
import { TargetOverlay, type TargetHit } from "@vitavision/overlays";
import type { StageLayer, StageView } from "@vitavision/stage2d";
import { useDebounced } from "../hooks/useDebounced";
import { useImageSize, type ImageSize } from "../hooks/useImageSize";
import { eyebrow, quietLink, textLink } from "../theme/classes";
import { cornerOfId, describeCorner, targetDetectionOf } from "../viewer/detection";
import { AB_COLORS, AbDiffLayers, abSplit } from "../viewer/layers";
import { StudioStage, useHoverText } from "../viewer/StudioStage";

// What the A/B diff overlay draws each run in (stage and legend alike): the stage's overlay roles.
const A_COLOR = AB_COLORS.a;
const B_COLOR = AB_COLORS.b;
const COMMON_COLOR = AB_COLORS.common;

interface Slot {
  engine: EngineReq;
  orientationMethod: OrientationMethodReq;
}

const DEFAULT_A: Slot = {
  engine: "pipeline",
  orientationMethod: "ring_fit",
};

const DEFAULT_B: Slot = { ...DEFAULT_A };

const MODES = [
  { value: "side", label: "side by side" },
  { value: "overlay", label: "diff overlay" },
];

function useSlotDetect(label: string, slot: Slot) {
  const debounced = useDebounced(slot, 300);
  return useQuery({
    queryKey: ["detect-compare", label, debounced],
    placeholderData: (prev) => prev,
    queryFn: () =>
      api.detect({
        label,
        engine: debounced.engine,
        orientation_method: debounced.orientationMethod,
        compare_baseline: true,
      }),
  });
}

export function CompareView() {
  const [search] = useSearchParams();
  const label = search.get("label") ?? "";
  const [mode, setMode] = useState<"side" | "overlay">("side");
  const [a, setA] = useState<Slot>(DEFAULT_A);
  const [b, setB] = useState<Slot>(DEFAULT_B);

  const size = useImageSize(label || null);
  const da = useSlotDetect(label, a);
  const db = useSlotDetect(label, b);

  // One view shared by every stage → synced zoom/pan.
  const [view, setView] = useState<StageView | null>(null);

  const cornersA = useMemo(() => da.data?.detection?.corners ?? [], [da.data]);
  const cornersB = useMemo(() => db.data?.detection?.corners ?? [], [db.data]);
  const split = useMemo(() => abSplit(cornersA, cornersB), [cornersA, cornersB]);

  if (!label) {
    return (
      <Empty className="m-6">
        Pick an image from the{" "}
        <Link to="/" className={textLink}>
          dataset
        </Link>{" "}
        first, then hit “compare ⇄”.
      </Empty>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-4 border-b border-line bg-surface px-4 py-3">
        <Link to={`/image/${label.split("/").map(encodeURIComponent).join("/")}`} className={quietLink}>
          ← workspace
        </Link>
        <span className="font-mono text-xs font-semibold">{label}</span>
        <SegmentedControl
          aria-label="Compare mode"
          value={mode}
          options={MODES}
          onValueChange={(m) => setMode(m as "side" | "overlay")}
        />
        <DeltaStrip da={da.data} db={db.data} overlap={split.common.length} />
      </header>

      <div className="flex min-h-0 flex-1 gap-px">
        {mode === "side" ? (
          <>
            <Pane title="A" color={A_COLOR} slot={a} onSlot={setA} detect={da.data}>
              <RunStage
                label={label}
                size={size.data}
                corners={cornersA}
                view={view}
                onView={setView}
                busy={da.isFetching}
              />
            </Pane>
            <Pane title="B" color={B_COLOR} slot={b} onSlot={setB} detect={db.data}>
              <RunStage
                label={label}
                size={size.data}
                corners={cornersB}
                view={view}
                onView={setView}
                busy={db.isFetching}
              />
            </Pane>
          </>
        ) : (
          <div className="flex min-h-0 flex-1">
            <div className="min-w-0 flex-1">
              <DiffStage label={label} size={size.data} split={split} view={view} onView={setView} />
            </div>
            <aside className="flex w-60 shrink-0 flex-col gap-3 border-l border-line bg-surface p-4">
              <SlotControls title="A" color={A_COLOR} slot={a} onSlot={setA} />
              <SlotControls title="B" color={B_COLOR} slot={b} onSlot={setB} />
              <div className="text-[11px] text-fg-muted">
                <Legend color={A_COLOR} text={`A only (${split.aOnly.length})`} />
                <Legend color={B_COLOR} text={`B only (${split.bOnly.length})`} />
                <Legend color={COMMON_COLOR} text={`common (${split.common.length})`} />
              </div>
            </aside>
          </div>
        )}
      </div>
    </div>
  );
}

// --- stages ------------------------------------------------------------------

type StageLayerId = "detection" | "edges";

/** One run's corners over the image: the same detection overlay as the workspace. */
function RunStage({
  label,
  size,
  corners,
  view,
  onView,
  busy,
}: {
  label: string;
  size: ImageSize | undefined;
  corners: BaselineCorner[];
  view: StageView | null;
  onView: (view: StageView) => void;
  busy: boolean;
}) {
  const [visible, setVisible] = useState<Record<StageLayerId, boolean>>({ detection: true, edges: true });
  const hovering = useHoverText();
  const detection = useMemo(() => targetDetectionOf(corners), [corners]);
  const menu: StageLayer[] = [
    { id: "detection", label: "Corners", swatch: "var(--stage-feature)", visible: visible.detection },
    { id: "edges", label: "Grid edges", swatch: "var(--stage-structure)", visible: visible.edges },
  ];
  const report = hovering.reporter("corner");
  const onHoverChange = (hit: TargetHit | null) => {
    const c = hit === null ? null : cornerOfId(String(hit.id), corners);
    report(c === null ? null : describeCorner(c));
  };
  if (!size) return null;
  return (
    <StudioStage
      key={label}
      label={label}
      size={size}
      layers={menu}
      onLayer={(id, on) => setVisible((v) => ({ ...v, [id as StageLayerId]: on }))}
      hover={hovering.text}
      view={view}
      onView={onView}
      busy={busy}
    >
      {visible.detection && (
        <TargetOverlay detection={detection} showEdges={visible.edges} onHoverChange={onHoverChange} />
      )}
    </StudioStage>
  );
}

type DiffLayerId = "a" | "b" | "common";

/** The position-matched A/B diff over the image: A only, B only, common. */
function DiffStage({
  label,
  size,
  split,
  view,
  onView,
}: {
  label: string;
  size: ImageSize | undefined;
  split: ReturnType<typeof abSplit>;
  view: StageView | null;
  onView: (view: StageView) => void;
}) {
  const [visible, setVisible] = useState<Record<DiffLayerId, boolean>>({ a: true, b: true, common: true });
  const hovering = useHoverText();
  const shown = useMemo(
    () => ({
      aOnly: visible.a ? split.aOnly : [],
      bOnly: visible.b ? split.bOnly : [],
      common: visible.common ? split.common : [],
    }),
    [split, visible],
  );
  const menu: StageLayer[] = [
    { id: "a", label: "A only", swatch: A_COLOR, visible: visible.a },
    { id: "b", label: "B only", swatch: B_COLOR, visible: visible.b },
    { id: "common", label: "Common", swatch: COMMON_COLOR, visible: visible.common },
  ];
  if (!size) return null;
  return (
    <StudioStage
      key={label}
      label={label}
      size={size}
      layers={menu}
      onLayer={(id, on) => setVisible((v) => ({ ...v, [id as DiffLayerId]: on }))}
      hover={hovering.text}
      view={view}
      onView={onView}
    >
      <AbDiffLayers split={shown} onHover={hovering.reporter("diff")} />
    </StudioStage>
  );
}

function DeltaStrip({
  da,
  db,
  overlap,
}: {
  da?: DetectResponse | undefined;
  db?: DetectResponse | undefined;
  overlap: number;
}) {
  if (!da || !db) return <Badge className="font-mono">running…</Badge>;
  const ca = da.detection?.labelled_count ?? 0;
  const cb = db.detection?.labelled_count ?? 0;
  const dt = db.elapsed_ms - da.elapsed_ms;
  return (
    <div className="flex flex-wrap gap-1">
      <Badge className="font-mono">
        <span style={{ color: A_COLOR }}>
          A {ca} corners · {da.elapsed_ms.toFixed(1)} ms
        </span>
      </Badge>
      <Badge className="font-mono">
        <span style={{ color: B_COLOR }}>
          B {cb} corners · {db.elapsed_ms.toFixed(1)} ms
        </span>
      </Badge>
      <Badge tone={cb - ca > 0 ? "normal" : cb - ca < 0 ? "defect" : "neutral"} className="font-mono">
        Δ corners {cb - ca >= 0 ? "+" : ""}
        {cb - ca}
      </Badge>
      <Badge className="font-mono">
        Δ time {dt >= 0 ? "+" : ""}
        {dt.toFixed(1)} ms
      </Badge>
      <Badge className="font-mono">common {overlap}</Badge>
    </div>
  );
}

function Pane({
  title,
  color,
  slot,
  onSlot,
  detect,
  children,
}: {
  title: string;
  color: string;
  slot: Slot;
  onSlot: (s: Slot) => void;
  detect?: DetectResponse | undefined;
  children: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col border-r border-line">
      <div className="flex flex-wrap items-center gap-2 border-b border-line bg-surface px-3 py-2">
        <span className="font-bold" style={{ color }}>
          {title}
        </span>
        <SlotControls inline slot={slot} onSlot={onSlot} />
        {detect && (
          <Badge className="font-mono">
            {detect.detection?.labelled_count ?? 0} · {detect.elapsed_ms.toFixed(1)} ms
          </Badge>
        )}
      </div>
      <div className="min-h-0 flex-1">{children}</div>
    </div>
  );
}

function SlotControls({
  title,
  color,
  slot,
  onSlot,
  inline,
}: {
  title?: string;
  color?: string;
  slot: Slot;
  onSlot: (s: Slot) => void;
  inline?: boolean;
}) {
  const sel = (name: string, value: string, options: string[], onChange: (v: string) => void) => (
    <Select
      aria-label={`${title ?? "Run"} ${name}`}
      value={value}
      options={options.map((o) => ({ value: o, label: o }))}
      onValueChange={onChange}
      className={cn("text-xs", inline && "w-auto min-w-28")}
    />
  );
  const body = (
    <>
      {sel("engine", slot.engine, ["pipeline", "grid"], (v) => onSlot({ ...slot, engine: v as EngineReq }))}
      {sel("axis fit", slot.orientationMethod, ["ring_fit", "disk_fit"], (v) =>
        onSlot({ ...slot, orientationMethod: v as OrientationMethodReq }),
      )}
    </>
  );
  if (inline) {
    return <div className="flex flex-wrap gap-1">{body}</div>;
  }
  return (
    <div>
      <div className={cn(eyebrow, "mb-1")} style={{ color }}>
        {title}
      </div>
      <div className="flex flex-col gap-1">{body}</div>
    </div>
  );
}

/** A legend entry: a ring in the colour the overlay draws that class in. */
function Legend({ color, text }: { color: string; text: string }) {
  return (
    <div className="flex items-center gap-1.5">
      <span aria-hidden className="inline-block size-2.5 rounded-full border-2" style={{ borderColor: color }} />
      {text}
    </div>
  );
}
