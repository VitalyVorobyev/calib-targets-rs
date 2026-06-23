// Interactive image viewport: scroll-wheel zoom around cursor, click-drag pan,
// double-click-to-fit, pixel-crisp magnification, layered overlays drawn in
// image coordinates, nearest-corner hit-test tooltip, rAF render loop.
//
// Ported directly from studio/src/components/CanvasViewport.tsx — same API,
// same interaction model, no backend dependency.

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

export interface ViewTransform {
  scale: number;
  tx: number;
  ty: number;
}

/** One toggleable overlay; `draw` receives a ctx already in image coords. */
export interface OverlayLayer {
  id: string;
  visible: boolean;
  draw: (ctx: CanvasRenderingContext2D, scale: number) => void;
}

/** A hover-testable point in image coordinates with arbitrary payload. */
export interface HitPoint<T> {
  x: number;
  y: number;
  data: T;
}

interface Props<T> {
  /** Decoded base image as an ImageBitmap. */
  image: ImageBitmap | null;
  /** Overlay layers drawn back-to-front. */
  layers: OverlayLayer[];
  /** Points eligible for hover; nearest within 8/scale px wins. */
  hitPoints?: HitPoint<T>[];
  /** Tooltip renderer for the hovered point. */
  renderTooltip?: (data: T) => ReactNode;
}

const MIN_SCALE = 0.05;
const MAX_SCALE = 32;

export function CanvasViewport<T>({
  image,
  layers,
  hitPoints,
  renderTooltip,
}: Props<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const ownTransform = useRef<ViewTransform | null>(null);
  const rafId = useRef<number>(0);
  const dragging = useRef<{ startX: number; startY: number } | null>(null);
  const [hover, setHover] = useState<{ px: number; py: number; data: T } | null>(null);
  const [zoomPct, setZoomPct] = useState<number>(100);

  const getTransform = useCallback((): ViewTransform | null => {
    return ownTransform.current;
  }, []);

  const setTransform = useCallback((t: ViewTransform) => {
    ownTransform.current = t;
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const t = getTransform();
    if (!canvas || !container || !t) return;
    const dpr = window.devicePixelRatio || 1;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    if (canvas.width !== cw * dpr || canvas.height !== ch * dpr) {
      canvas.width = cw * dpr;
      canvas.height = ch * dpr;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr * t.scale, 0, 0, dpr * t.scale, dpr * t.tx, dpr * t.ty);
    if (image) {
      ctx.imageSmoothingEnabled = t.scale < 4;
      ctx.drawImage(image, 0, 0);
    }
    for (const layer of layers) {
      if (layer.visible) layer.draw(ctx, t.scale);
    }
    setZoomPct(Math.round(t.scale * 100));
  }, [image, layers, getTransform]);

  const requestRedraw = useCallback(() => {
    cancelAnimationFrame(rafId.current);
    rafId.current = requestAnimationFrame(draw);
  }, [draw]);

  const fit = useCallback(() => {
    const container = containerRef.current;
    if (!container || !image) return;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const scale = Math.min(cw / image.width, ch / image.height) * 0.98;
    setTransform({
      scale,
      tx: (cw - image.width * scale) / 2,
      ty: (ch - image.height * scale) / 2,
    });
    requestRedraw();
  }, [image, setTransform, requestRedraw]);

  // Fit on image change; redraw on layer change.
  useEffect(() => {
    if (!getTransform()) fit();
    else requestRedraw();
  }, [image, fit, getTransform, requestRedraw]);
  useEffect(() => requestRedraw(), [layers, requestRedraw]);

  // Redraw when the container resizes.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(() => requestRedraw());
    ro.observe(container);
    return () => ro.disconnect();
  }, [requestRedraw]);

  const clientToImage = useCallback(
    (clientX: number, clientY: number): { x: number; y: number } | null => {
      const container = containerRef.current;
      const t = getTransform();
      if (!container || !t) return null;
      const rect = container.getBoundingClientRect();
      return {
        x: (clientX - rect.left - t.tx) / t.scale,
        y: (clientY - rect.top - t.ty) / t.scale,
      };
    },
    [getTransform],
  );

  const onWheel = useCallback(
    (e: WheelEvent) => {
      e.preventDefault();
      const t = getTransform();
      const container = containerRef.current;
      if (!t || !container) return;
      const rect = container.getBoundingClientRect();
      const cx = e.clientX - rect.left;
      const cy = e.clientY - rect.top;
      const factor = Math.exp(-e.deltaY * 0.0015);
      const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, t.scale * factor));
      const k = scale / t.scale;
      setTransform({
        scale,
        tx: cx - (cx - t.tx) * k,
        ty: cy - (cy - t.ty) * k,
      });
      requestRedraw();
    },
    [getTransform, setTransform, requestRedraw],
  );

  // Non-passive wheel listener (preventDefault needs it).
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    container.addEventListener("wheel", onWheel, { passive: false });
    return () => container.removeEventListener("wheel", onWheel);
  }, [onWheel]);

  const hitIndex = useMemo(() => hitPoints ?? [], [hitPoints]);

  const onPointerMove = (e: React.PointerEvent) => {
    const t = getTransform();
    if (!t) return;
    if (dragging.current) {
      setTransform({
        scale: t.scale,
        tx: t.tx + (e.clientX - dragging.current.startX),
        ty: t.ty + (e.clientY - dragging.current.startY),
      });
      dragging.current = { startX: e.clientX, startY: e.clientY };
      requestRedraw();
      setHover(null);
      return;
    }
    if (!hitIndex.length || !renderTooltip) return;
    const pos = clientToImage(e.clientX, e.clientY);
    if (!pos) return;
    const radius = 8 / t.scale;
    let best: HitPoint<T> | null = null;
    let bestD = radius * radius;
    for (const p of hitIndex) {
      const dx = p.x - pos.x;
      const dy = p.y - pos.y;
      const d = dx * dx + dy * dy;
      if (d <= bestD) {
        bestD = d;
        best = p;
      }
    }
    if (best) {
      const rect = containerRef.current!.getBoundingClientRect();
      setHover({
        px: e.clientX - rect.left,
        py: e.clientY - rect.top,
        data: best.data,
      });
    } else {
      setHover(null);
    }
  };

  return (
    <div
      ref={containerRef}
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        overflow: "hidden",
        background: "var(--bg0)",
        cursor: dragging.current ? "grabbing" : "crosshair",
        touchAction: "none",
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        dragging.current = { startX: e.clientX, startY: e.clientY };
      }}
      onPointerUp={() => {
        dragging.current = null;
      }}
      onPointerLeave={() => {
        dragging.current = null;
        setHover(null);
      }}
      onPointerMove={onPointerMove}
      onDoubleClick={fit}
    >
      <canvas
        ref={canvasRef}
        style={{ width: "100%", height: "100%", display: "block" }}
      />

      {/* Empty-state hint */}
      {!image && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: "var(--s2)",
            color: "var(--text-faint)",
            pointerEvents: "none",
            userSelect: "none",
          }}
        >
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" opacity="0.4">
            <rect x="3" y="3" width="18" height="18" rx="2"/>
            <path d="M3 9h18M9 3v18M15 3v18M3 15h18"/>
          </svg>
          <span style={{ fontSize: 12 }}>Load an image or generate a synthetic target</span>
          <span style={{ fontSize: 11, color: "var(--text-faint)" }}>scroll to zoom · drag to pan · double-click to fit</span>
        </div>
      )}

      {/* Hover tooltip */}
      {hover && renderTooltip && (
        <div
          style={{
            position: "absolute",
            left: hover.px + 14,
            top: hover.py + 14,
            pointerEvents: "none",
            zIndex: 10,
            background: "color-mix(in srgb, var(--bg1) 92%, transparent)",
            border: "1px solid var(--border-strong)",
            borderRadius: "var(--radius)",
            padding: "6px 10px",
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            whiteSpace: "nowrap",
            boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
          }}
        >
          {renderTooltip(hover.data)}
        </div>
      )}

      {/* Zoom indicator */}
      {image && (
        <div
          style={{
            position: "absolute",
            right: 8,
            bottom: 8,
            padding: "2px 8px",
            borderRadius: "var(--radius-sm)",
            background: "color-mix(in srgb, var(--bg1) 85%, transparent)",
            border: "1px solid var(--border)",
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            color: "var(--text-muted)",
            pointerEvents: "none",
          }}
        >
          {zoomPct}%
        </div>
      )}
    </div>
  );
}
