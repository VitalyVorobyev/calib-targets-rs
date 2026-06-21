// A/B comparison: run two configs on the same snap, side-by-side with
// synchronised zoom/pan, or as a single position-matched diff overlay
// (A-only / B-only / common), plus a metric delta strip.

import { useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { api } from "../api/client";
import type {
  BaselineCorner,
  DetectorParamsOverride,
  DetectResponse,
  EngineReq,
  GraphBuildAlgorithm,
  OrientationMethodReq,
  OrientationSource,
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

const A_COLOR = "rgb(255, 120, 60)";
const B_COLOR = "rgb(80, 200, 255)";
const COMMON_COLOR = "rgba(170, 180, 190, 0.8)";

interface Slot {
  algorithm: GraphBuildAlgorithm;
  engine: EngineReq;
  orientationSource: OrientationSource;
  orientationMethod: OrientationMethodReq;
}

const DEFAULT_A: Slot = {
  algorithm: "topological",
  engine: "pipeline",
  orientationSource: "chess_axes",
  orientationMethod: "ring_fit",
};

const DEFAULT_B: Slot = { ...DEFAULT_A };

function slotParams(s: Slot): DetectorParamsOverride {
  return {
    graph_build_algorithm: s.algorithm,
    orientation_source: s.orientationSource,
  };
}

function useSlotDetect(label: string, slot: Slot) {
  const debounced = useDebounced(slot, 300);
  return useQuery({
    queryKey: ["detect-compare", label, debounced],
    placeholderData: (prev) => prev,
    queryFn: () =>
      api.detect({
        label,
        engine: debounced.engine,
        params: slotParams(debounced),
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
      <div style={{ padding: "var(--s5)", color: "var(--text-muted)" }}>
        Pick an image from the <Link to="/">dataset</Link> first, then hit
        “compare ⇄”.
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--s4)",
          padding: "var(--s3) var(--s4)",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg1)",
          flexWrap: "wrap",
        }}
      >
        <Link
          to={`/image/${label.split("/").map(encodeURIComponent).join("/")}`}
          style={{ fontSize: 12 }}
        >
          ← workspace
        </Link>
        <span className="mono" style={{ fontWeight: 600, fontSize: 12 }}>
          {label}
        </span>
        <div style={{ display: "flex", gap: 2 }}>
          {(["side", "overlay"] as const).map((m) => (
            <button
              key={m}
              className="btn"
              style={{
                padding: "3px 10px",
                fontSize: 11,
                background: mode === m ? "var(--bg3)" : "var(--bg2)",
                borderColor: mode === m ? "var(--accent)" : "var(--border)",
              }}
              onClick={() => setMode(m)}
            >
              {m === "side" ? "side by side" : "diff overlay"}
            </button>
          ))}
        </div>
        <DeltaStrip da={da.data} db={db.data} overlap={overlay.commonCount} />
      </header>

      <div style={{ display: "flex", gap: 1, flex: 1, minHeight: 0 }}>
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
          <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <CanvasViewport
                image={bitmap.data ?? null}
                layers={[overlay.layer]}
                hitPoints={overlay.hits}
                renderTooltip={(d) => (
                  <>
                    <div style={{ color: d.color }}>● {d.where}</div>
                    <div style={{ color: "var(--text-muted)" }}>
                      (i, j) = ({d.c.i}, {d.c.j}) · x {d.c.x.toFixed(1)} · y{" "}
                      {d.c.y.toFixed(1)}
                    </div>
                  </>
                )}
              />
            </div>
            <aside
              style={{
                width: 240,
                flexShrink: 0,
                borderLeft: "1px solid var(--border)",
                background: "var(--bg1)",
                padding: "var(--s4)",
                display: "flex",
                flexDirection: "column",
                gap: "var(--s3)",
              }}
            >
              <SlotControls title="A" color={A_COLOR} slot={a} onSlot={setA} />
              <SlotControls title="B" color={B_COLOR} slot={b} onSlot={setB} />
              <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
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
      <div style={{ color: "var(--text-muted)" }}>
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
  da?: DetectResponse;
  db?: DetectResponse;
  overlap: number;
}) {
  if (!da || !db) return <span className="chip">running…</span>;
  const ca = da.detection?.labelled_count ?? 0;
  const cb = db.detection?.labelled_count ?? 0;
  const dt = db.elapsed_ms - da.elapsed_ms;
  return (
    <div style={{ display: "flex", gap: "var(--s1)", flexWrap: "wrap" }}>
      <span className="chip" style={{ color: A_COLOR }}>
        A {ca} corners · {da.elapsed_ms.toFixed(1)} ms
      </span>
      <span className="chip" style={{ color: B_COLOR }}>
        B {cb} corners · {db.elapsed_ms.toFixed(1)} ms
      </span>
      <span className={`chip ${cb - ca > 0 ? "ok" : cb - ca < 0 ? "err" : ""}`}>
        Δ corners {cb - ca >= 0 ? "+" : ""}
        {cb - ca}
      </span>
      <span className="chip">
        Δ time {dt >= 0 ? "+" : ""}
        {dt.toFixed(1)} ms
      </span>
      <span className="chip">common {overlap}</span>
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
  detect?: DetectResponse;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        flex: 1,
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        borderRight: "1px solid var(--border)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--s2)",
          padding: "var(--s2) var(--s3)",
          background: "var(--bg1)",
          borderBottom: "1px solid var(--border)",
          flexWrap: "wrap",
        }}
      >
        <span style={{ color, fontWeight: 700 }}>{title}</span>
        <SlotControls inline slot={slot} onSlot={onSlot} />
        {detect && (
          <span className="chip">
            {detect.detection?.labelled_count ?? 0} ·{" "}
            {detect.elapsed_ms.toFixed(1)} ms
          </span>
        )}
      </div>
      <div style={{ flex: 1, minHeight: 0 }}>{children}</div>
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
    value: string,
    options: string[],
    onChange: (v: string) => void,
    disabled: string[] = [],
  ) => (
    <select
      className="select"
      value={value}
      style={{ fontSize: 11, padding: "2px 4px" }}
      onChange={(e) => onChange(e.target.value)}
    >
      {options.map((o) => (
        <option key={o} value={o} disabled={disabled.includes(o)}>
          {o}
        </option>
      ))}
    </select>
  );
  const body = (
    <>
      {sel(slot.algorithm, ["topological"], (v) =>
        onSlot({ ...slot, algorithm: v as GraphBuildAlgorithm }),
      )}
      {sel(slot.engine, ["pipeline", "grid"], (v) =>
        onSlot({ ...slot, engine: v as EngineReq }),
      )}
      {sel(
        slot.orientationSource,
        ["chess_axes", "neighbour_edges"],
        (v) => onSlot({ ...slot, orientationSource: v as OrientationSource }),
      )}
      {sel(slot.orientationMethod, ["ring_fit", "disk_fit"], (v) =>
        onSlot({ ...slot, orientationMethod: v as OrientationMethodReq }),
      )}
    </>
  );
  if (inline) {
    return <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>{body}</div>;
  }
  return (
    <div>
      <div className="label" style={{ color, marginBottom: "var(--s1)" }}>
        {title}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {body}
      </div>
    </div>
  );
}

function Legend({ color, text }: { color: string; text: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <span
        style={{
          width: 9,
          height: 9,
          borderRadius: "50%",
          border: `2px solid ${color}`,
          display: "inline-block",
        }}
      />
      {text}
    </div>
  );
}
