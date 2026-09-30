// A/B comparison: run two configs on the same snap, side-by-side with
// synchronised zoom/pan, or as a single position-matched diff overlay
// (A-only / B-only / common), plus a metric delta strip.

import { useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router";
import { Badge, Empty, SegmentedControl, Select, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type {
  BaselineCorner,
  DetectResponse,
  EngineReq,
  OrientationMethodReq,
} from "../api/types";
import {
  CanvasViewport,
  type HitPoint,
  type OverlayLayer,
  type ViewTransform,
} from "../components/CanvasViewport";
import { cornersLayer, edgesLayer } from "../components/overlays";
import { useDebounced } from "../hooks/useDebounced";
import { useImageBitmap } from "../hooks/useImageBitmap";
import { eyebrow, quietLink, textLink } from "../theme/classes";

// Data colours: what the A/B diff overlay draws each run in (canvas and legend alike).
const A_COLOR = "rgb(255, 120, 60)";
const B_COLOR = "rgb(80, 200, 255)";
const COMMON_COLOR = "rgba(170, 180, 190, 0.8)";

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

  const bitmap = useImageBitmap(label || null);
  const da = useSlotDetect(label, a);
  const db = useSlotDetect(label, b);

  // One transform object shared by both viewports → synced zoom/pan.
  const shared = useRef<ViewTransform | null>(null);

  const cornersA = da.data?.detection?.corners ?? [];
  const cornersB = db.data?.detection?.corners ?? [];

  const layersA = useMemo(
    () => [edgesLayer(cornersA, true), cornersLayer(cornersA, true)],
    [cornersA],
  );
  const layersB = useMemo(
    () => [edgesLayer(cornersB, true), cornersLayer(cornersB, true)],
    [cornersB],
  );

  const overlay = useMemo(
    () => diffOverlay(cornersA, cornersB),
    [cornersA, cornersB],
  );

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
        <Link
          to={`/image/${label.split("/").map(encodeURIComponent).join("/")}`}
          className={quietLink}
        >
          ← workspace
        </Link>
        <span className="font-mono text-xs font-semibold">{label}</span>
        <SegmentedControl
          aria-label="Compare mode"
          value={mode}
          options={MODES}
          onValueChange={(m) => setMode(m as "side" | "overlay")}
        />
        <DeltaStrip da={da.data} db={db.data} overlap={overlay.commonCount} />
      </header>

      <div className="flex min-h-0 flex-1 gap-px">
        {mode === "side" ? (
          <>
            <Pane
              title="A"
              color={A_COLOR}
              slot={a}
              onSlot={setA}
              detect={da.data}
            >
              <CanvasViewport
                image={bitmap.data ?? null}
                layers={layersA}
                hitPoints={hitPoints(cornersA)}
                renderTooltip={tooltip}
                sharedTransform={shared}
              />
            </Pane>
            <Pane
              title="B"
              color={B_COLOR}
              slot={b}
              onSlot={setB}
              detect={db.data}
            >
              <CanvasViewport
                image={bitmap.data ?? null}
                layers={layersB}
                hitPoints={hitPoints(cornersB)}
                renderTooltip={tooltip}
                sharedTransform={shared}
              />
            </Pane>
          </>
        ) : (
          <div className="flex min-h-0 flex-1">
            <div className="min-w-0 flex-1">
              <CanvasViewport
                image={bitmap.data ?? null}
                layers={[overlay.layer]}
                hitPoints={overlay.hits}
                renderTooltip={(d) => (
                  <>
                    <div style={{ color: d.color }}>● {d.where}</div>
                    <div className="text-fg-muted">
                      (i, j) = ({d.c.i}, {d.c.j}) · x {d.c.x.toFixed(1)} · y{" "}
                      {d.c.y.toFixed(1)}
                    </div>
                  </>
                )}
              />
            </div>
            <aside className="flex w-60 shrink-0 flex-col gap-3 border-l border-line bg-surface p-4">
              <SlotControls title="A" color={A_COLOR} slot={a} onSlot={setA} />
              <SlotControls title="B" color={B_COLOR} slot={b} onSlot={setB} />
              <div className="text-[11px] text-fg-muted">
                <Legend color={A_COLOR} text={`A only (${overlay.aOnly})`} />
                <Legend color={B_COLOR} text={`B only (${overlay.bOnly})`} />
                <Legend
                  color={COMMON_COLOR}
                  text={`common (${overlay.commonCount})`}
                />
              </div>
            </aside>
          </div>
        )}
      </div>
    </div>
  );
}

// --- helpers -----------------------------------------------------------------

function hitPoints(corners: BaselineCorner[]): HitPoint<BaselineCorner>[] {
  return corners.map((c) => ({ x: c.x, y: c.y, data: c }));
}

function tooltip(c: BaselineCorner) {
  return (
    <>
      <div>
        (i, j) = ({c.i}, {c.j})
      </div>
      <div className="text-fg-muted">
        x {c.x.toFixed(2)} · y {c.y.toFixed(2)} · score {c.score.toFixed(1)}
      </div>
    </>
  );
}

interface OverlayHit {
  c: BaselineCorner;
  where: "A only" | "B only" | "common";
  color: string;
}

function diffOverlay(a: BaselineCorner[], b: BaselineCorner[]) {
  const key = (c: BaselineCorner) => `${c.x.toFixed(2)},${c.y.toFixed(2)}`;
  const bKeys = new Set(b.map(key));
  const aKeys = new Set(a.map(key));
  const aOnly = a.filter((c) => !bKeys.has(key(c)));
  const bOnly = b.filter((c) => !aKeys.has(key(c)));
  const common = a.filter((c) => bKeys.has(key(c)));

  const layer: OverlayLayer = {
    id: "ab-diff",
    visible: true,
    draw: (ctx, scale) => {
      const r = 3 / Math.sqrt(scale);
      ctx.fillStyle = COMMON_COLOR;
      for (const c of common) {
        ctx.beginPath();
        ctx.arc(c.x, c.y, r * 0.7, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.lineWidth = 1.6 / Math.sqrt(scale);
      ctx.strokeStyle = A_COLOR;
      for (const c of aOnly) {
        ctx.beginPath();
        ctx.arc(c.x, c.y, r * 1.6, 0, Math.PI * 2);
        ctx.stroke();
      }
      ctx.strokeStyle = B_COLOR;
      for (const c of bOnly) {
        ctx.beginPath();
        ctx.arc(c.x, c.y, r * 1.6, 0, Math.PI * 2);
        ctx.stroke();
      }
    },
  };

  const hits: HitPoint<OverlayHit>[] = [
    ...aOnly.map((c) => ({
      x: c.x,
      y: c.y,
      data: { c, where: "A only" as const, color: A_COLOR },
    })),
    ...bOnly.map((c) => ({
      x: c.x,
      y: c.y,
      data: { c, where: "B only" as const, color: B_COLOR },
    })),
    ...common.map((c) => ({
      x: c.x,
      y: c.y,
      data: { c, where: "common" as const, color: COMMON_COLOR },
    })),
  ];

  return {
    layer,
    hits,
    aOnly: aOnly.length,
    bOnly: bOnly.length,
    commonCount: common.length,
  };
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
      <Badge
        tone={cb - ca > 0 ? "normal" : cb - ca < 0 ? "defect" : "neutral"}
        className="font-mono"
      >
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
  const sel = (
    name: string,
    value: string,
    options: string[],
    onChange: (v: string) => void,
  ) => (
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
      {sel("engine", slot.engine, ["pipeline", "grid"], (v) =>
        onSlot({ ...slot, engine: v as EngineReq }),
      )}
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
      <span
        aria-hidden
        className="inline-block size-2.5 rounded-full border-2"
        style={{ borderColor: color }}
      />
      {text}
    </div>
  );
}
