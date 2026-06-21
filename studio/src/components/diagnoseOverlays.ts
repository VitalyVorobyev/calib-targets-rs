// Diagnose-mode overlay builders: per-stage corner markers (seed-and-grow
// DebugFrame), axis vectors, and the topological labelled/dropped split.
// Stage palette is locked to render_diagnose_overlay (bench overlay.rs).

import {
  stageName,
  type CornerAugWire,
  type StageName,
  type TopologicalDiagnosisWire,
} from "../api/types";
import type { OverlayLayer } from "./CanvasViewport";

export const STAGE_COLORS: Record<StageName, string> = {
  Raw: "#646464",
  Strong: "#64b4ff",
  NoCluster: "#2850c8",
  Clustered: "#00dcdc",
  AttachmentAmbiguous: "#ff00dc",
  AttachmentFailedInvariants: "#ff8c00",
  Labeled: "#e61e1e",
  LabeledThenBlacklisted: "#ffdc1e",
  Other: "#8b95a3",
};

export const TOPO_COLORS = {
  labelled: "rgb(50, 220, 80)",
  dropped: "rgb(220, 50, 50)",
} as const;

/** Stage-colored markers for every DebugFrame corner. Raw corners faint. */
export function stageMarkersLayer(
  corners: CornerAugWire[],
  visibleStages: Record<string, boolean>,
  visible: boolean,
): OverlayLayer {
  return {
    id: "stage-markers",
    visible,
    draw: (ctx, scale) => {
      const r = 3 / Math.sqrt(scale);
      for (const c of corners) {
        const name = stageName(c.stage);
        if (visibleStages[name] === false) continue;
        ctx.globalAlpha = name === "Raw" ? 0.45 : 1;
        ctx.fillStyle = STAGE_COLORS[name];
        ctx.beginPath();
        ctx.arc(c.position[0], c.position[1], r, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    },
  };
}

/**
 * Per-corner axis segments: axes[0] warm orange, axes[1] cool teal — the
 * canvas twin of `--draw-axes`. Raw corners are skipped (no useful axes).
 */
export function axesLayer(
  corners: CornerAugWire[],
  visible: boolean,
): OverlayLayer {
  return {
    id: "axes",
    visible,
    draw: (ctx, scale) => {
      const len = 11 / Math.sqrt(scale);
      ctx.lineWidth = 1.2 / Math.sqrt(scale);
      for (const c of corners) {
        if (stageName(c.stage) === "Raw") continue;
        const [x, y] = c.position;
        for (const [k, color] of [
          [0, "#ff9632"],
          [1, "#32c8c8"],
        ] as const) {
          const a = c.axes[k].angle;
          ctx.strokeStyle = color;
          ctx.beginPath();
          ctx.moveTo(x - Math.cos(a) * len, y - Math.sin(a) * len);
          ctx.lineTo(x + Math.cos(a) * len, y + Math.sin(a) * len);
          ctx.stroke();
        }
      }
    },
  };
}

/** Topological diagnosis: green = labelled, red = dropped. */
export function topoSplitLayer(
  diagnosis: TopologicalDiagnosisWire,
  visible: boolean,
): OverlayLayer {
  return {
    id: "topo-split",
    visible,
    draw: (ctx, scale) => {
      const r = 2.6 / Math.sqrt(scale);
      for (const c of diagnosis.corners) {
        ctx.fillStyle = c.labelled ? TOPO_COLORS.labelled : TOPO_COLORS.dropped;
        ctx.beginPath();
        ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
        ctx.fill();
      }
    },
  };
}
