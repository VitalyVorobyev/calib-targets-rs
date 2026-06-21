// Diagnose-mode overlay builders: topological labelled/dropped split.
// Stage palette is locked to render_diagnose_overlay (bench overlay.rs).

import type { TopologicalDiagnosisWire } from "../api/types";
import type { OverlayLayer } from "./CanvasViewport";

export const TOPO_COLORS = {
  labelled: "rgb(50, 220, 80)",
  dropped: "rgb(220, 50, 50)",
} as const;

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
