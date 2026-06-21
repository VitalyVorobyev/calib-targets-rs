// Overlay-layer builders: turn detection / baseline data into draw callbacks
// for CanvasViewport. Colors follow the bench CLI's overlay conventions
// (crates/calib-targets-bench/src/overlay.rs) so PNG overlays and studio
// canvases read identically.

import type { BaselineCorner, BaselineDiff } from "../api/types";
import type { OverlayLayer } from "./CanvasViewport";

export const OVERLAY_COLORS = {
  corner: "rgb(230, 30, 30)",
  edge: "rgb(100, 180, 255)",
  origin: "rgb(255, 220, 30)",
  far: "rgb(30, 200, 60)",
  missing: "rgb(248, 81, 73)",
  extra: "rgb(63, 185, 80)",
  wrongPos: "rgb(210, 153, 34)",
} as const;

/** Marker radius in *screen* pixels (divided by scale when drawing). */
const R = 3.5;

function gridKey(i: number, j: number): string {
  return `${i},${j}`;
}

/** Light-blue segments between cardinal (i, j) neighbours. */
export function edgesLayer(
  corners: BaselineCorner[],
  visible: boolean,
): OverlayLayer {
  const byGrid = new Map<string, BaselineCorner>();
  for (const c of corners) byGrid.set(gridKey(c.i, c.j), c);
  return {
    id: "edges",
    visible,
    draw: (ctx, scale) => {
      ctx.strokeStyle = OVERLAY_COLORS.edge;
      ctx.lineWidth = Math.max(1 / scale, 0.4);
      ctx.beginPath();
      for (const c of corners) {
        for (const [di, dj] of [
          [1, 0],
          [0, 1],
        ] as const) {
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
  corners: BaselineCorner[],
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

/** Yellow ring on (min_i, min_j), green ring on (max_i, max_j). */
export function ringsLayer(
  corners: BaselineCorner[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "rings",
    visible,
    draw: (ctx, scale) => {
      if (!corners.length) return;
      let minI = Infinity,
        minJ = Infinity,
        maxI = -Infinity,
        maxJ = -Infinity;
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
  corners: BaselineCorner[],
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

/**
 * Baseline-diff highlights: hollow red ring where a baseline corner went
 * missing (drawn at the baseline position), green ring on extra run corners,
 * amber arrow from baseline to run position on wrong-position pairs.
 */
export function baselineDiffLayer(
  diff: BaselineDiff | null,
  baselineCorners: BaselineCorner[] | null,
  runCorners: BaselineCorner[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "baseline-diff",
    visible,
    draw: (ctx, scale) => {
      if (!diff) return;
      const r = 6 / Math.sqrt(scale);
      ctx.lineWidth = 1.8 / Math.sqrt(scale);

      if (baselineCorners) {
        const byGrid = new Map<string, BaselineCorner>();
        for (const c of baselineCorners) byGrid.set(gridKey(c.i, c.j), c);
        ctx.strokeStyle = OVERLAY_COLORS.missing;
        for (const [i, j] of diff.missing_labels) {
          const c = byGrid.get(gridKey(i, j));
          if (!c) continue;
          ctx.beginPath();
          ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
          ctx.stroke();
          ctx.beginPath();
          ctx.moveTo(c.x - r, c.y - r);
          ctx.lineTo(c.x + r, c.y + r);
          ctx.stroke();
        }
      }

      const runByGrid = new Map<string, BaselineCorner>();
      for (const c of runCorners) runByGrid.set(gridKey(c.i, c.j), c);
      ctx.strokeStyle = OVERLAY_COLORS.extra;
      for (const [i, j] of diff.extra_labels) {
        const c = runByGrid.get(gridKey(i, j));
        if (!c) continue;
        ctx.beginPath();
        ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
        ctx.stroke();
      }

      ctx.strokeStyle = OVERLAY_COLORS.wrongPos;
      for (const wp of diff.wrong_position) {
        ctx.beginPath();
        ctx.moveTo(wp.baseline[0], wp.baseline[1]);
        ctx.lineTo(wp.run[0], wp.run[1]);
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(wp.run[0], wp.run[1], r * 0.7, 0, Math.PI * 2);
        ctx.stroke();
      }
    },
  };
}
