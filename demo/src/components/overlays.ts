// Overlay-layer builders: turn WASM detection results into draw callbacks
// for CanvasViewport. Colors follow the bench CLI's overlay conventions
// (crates/calib-targets-bench/src/overlay.rs) so the browser and CLI render
// identically.

import type { OverlayLayer } from "./CanvasViewport";

export const OVERLAY_COLORS = {
  corner: "rgb(230, 30, 30)",
  edge: "rgb(100, 180, 255)",
  origin: "rgb(255, 220, 30)",
  far: "rgb(30, 200, 60)",
} as const;

/** A corner as used by the overlay — image-pixel coordinates plus grid label. */
export interface OverlayCorner {
  /** Image x in pixels. */
  x: number;
  /** Image y in pixels. */
  y: number;
  i: number;
  j: number;
  id?: number | null;
  score?: number;
}

function gridKey(i: number, j: number): string {
  return `${i},${j}`;
}

/** Marker radius in *image* pixels, scaled at draw time. */
const R = 3.5;

/** Light-blue segments between cardinal (i, j) neighbours. */
export function edgesLayer(
  corners: OverlayCorner[],
  visible: boolean,
): OverlayLayer {
  const byGrid = new Map<string, OverlayCorner>();
  for (const c of corners) byGrid.set(gridKey(c.i, c.j), c);
  return {
    id: "edges",
    visible,
    draw: (ctx, scale) => {
      ctx.strokeStyle = OVERLAY_COLORS.edge;
      ctx.lineWidth = Math.max(1 / scale, 0.4);
      ctx.beginPath();
      for (const c of corners) {
        for (const [di, dj] of [[1, 0], [0, 1]] as const) {
          const n = byGrid.get(gridKey(c.i + di, c.j + dj));
          if (n) {
            ctx.moveTo(c.x, c.y);
            ctx.lineTo(n.x, n.y);
          }
        }
      }
      ctx.stroke();
    },
  };
}

/** Filled red dots on every labelled corner. */
export function cornersLayer(
  corners: OverlayCorner[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "corners",
    visible,
    draw: (ctx, scale) => {
      ctx.fillStyle = OVERLAY_COLORS.corner;
      const r = R / Math.sqrt(scale);
      for (const c of corners) {
        ctx.beginPath();
        ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
        ctx.fill();
      }
    },
  };
}

/** Yellow ring on origin (min i, min j), green ring on far (max i, max j). */
export function ringsLayer(
  corners: OverlayCorner[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "rings",
    visible,
    draw: (ctx, scale) => {
      if (!corners.length) return;
      let minI = Infinity, minJ = Infinity, maxI = -Infinity, maxJ = -Infinity;
      for (const c of corners) {
        minI = Math.min(minI, c.i);
        minJ = Math.min(minJ, c.j);
        maxI = Math.max(maxI, c.i);
        maxJ = Math.max(maxJ, c.j);
      }
      const origin = corners.find((c) => c.i === minI && c.j === minJ);
      const far = corners.find((c) => c.i === maxI && c.j === maxJ);
      const r = 7 / Math.sqrt(scale);
      ctx.lineWidth = 2 / Math.sqrt(scale);
      if (origin) {
        ctx.strokeStyle = OVERLAY_COLORS.origin;
        ctx.beginPath();
        ctx.arc(origin.x, origin.y, r, 0, Math.PI * 2);
        ctx.stroke();
      }
      if (far) {
        ctx.strokeStyle = OVERLAY_COLORS.far;
        ctx.beginPath();
        ctx.arc(far.x, far.y, r, 0, Math.PI * 2);
        ctx.stroke();
      }
    },
  };
}

/** (i,j) / id text labels, readable only when zoomed in. */
export function idsLayer(
  corners: OverlayCorner[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "ids",
    visible,
    draw: (ctx, scale) => {
      if (scale < 2) return;
      ctx.font = `${10 / scale}px ui-monospace, monospace`;
      ctx.fillStyle = "rgba(230, 233, 238, 0.9)";
      for (const c of corners) {
        const text = c.id != null ? `#${c.id}` : `${c.i},${c.j}`;
        ctx.fillText(text, c.x + 5 / scale, c.y - 4 / scale);
      }
    },
  };
}

/** Raw ChESS corners — colored by axis-0 angle hue. */
export function rawCornersLayer(
  rawCorners: Array<{ position: [number, number]; axes: Array<{ angle: number }>; strength: number }>,
  visible: boolean,
): OverlayLayer {
  return {
    id: "raw-corners",
    visible,
    draw: (ctx, scale) => {
      for (const c of rawCorners) {
        const [cx, cy] = c.position;
        const hue = ((c.axes[0]!.angle / Math.PI) * 360 + 360) % 360;
        const r = Math.max(2 / scale, Math.min(5 / scale, c.strength / 200 / scale));
        ctx.fillStyle = `hsl(${hue}, 100%, 55%)`;
        ctx.globalAlpha = 0.8;
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    },
  };
}
