// What a detector run returned, read for display: the overlay detection, the corner rows, and
// the per-item facts a selection shows. Pure functions over `DetectPayload`.

import { fromCalibTargets } from "@vitavision/overlays";
import type { TargetDetection } from "@vitavision/overlays";

import type { DetectPayload } from "../worker/protocol";

/** A position covariance in px^2: the fields `position_covariance` carries when the build has it. */
export interface Covariance {
  xx: number;
  xy: number;
  yy: number;
}

/** The covariance a corner carries, if this build of the WASM package reports one. */
function covarianceOf(corner: object): Covariance | null {
  const cov = (corner as { position_covariance?: Covariance | null }).position_covariance;
  return cov && Number.isFinite(cov.xx + cov.xy + cov.yy) ? cov : null;
}

/** Largest 1-sigma semi-axis in px: the square root of the covariance's larger eigenvalue. */
export function sigmaMax({ xx, xy, yy }: Covariance): number {
  const mean = (xx + yy) / 2;
  const larger = mean + Math.hypot((xx - yy) / 2, xy);
  return larger > 0 ? Math.sqrt(larger) : 0;
}

/** The overlay's detection for a payload. */
export function detectionOf(payload: DetectPayload): TargetDetection {
  switch (payload.kind) {
    case "corners":
      return fromCalibTargets({ kind: "corners", corners: payload.corners });
    case "chessboard":
      return fromCalibTargets({ kind: "chessboard", detection: payload.detection });
    case "charuco":
      return fromCalibTargets({ kind: "charuco", detection: payload.detection });
    case "markerboard":
      return fromCalibTargets({ kind: "markerboard", detection: payload.detection, diagnostics: payload.diagnostics });
    case "puzzleboard":
      return fromCalibTargets({ kind: "puzzleboard", detection: payload.detection, diagnostics: payload.diagnostics });
  }
}

/** One corner, flattened for the table and the selection card. `id` is the overlay's `corner-<n>`. */
export interface CornerRow {
  id: string;
  index: number;
  x: number;
  y: number;
  /** Grid label (u, v); null for a raw corner. */
  grid: { i: number; j: number } | null;
  /** The target's own id (ChArUco corner id, PuzzleBoard master id); null where the detector has none. */
  targetId: number | null;
  score: number;
  /** Largest 1-sigma semi-axis, px; null when the result carries no covariance. */
  sigma: number | null;
}

export function cornerRows(payload: DetectPayload): CornerRow[] {
  if (payload.kind === "corners") {
    return payload.corners.map((corner, index) => {
      const cov = covarianceOf(corner);
      return {
        id: `corner-${index}`,
        index,
        x: corner.position[0],
        y: corner.position[1],
        grid: null,
        targetId: null,
        score: corner.strength,
        sigma: cov ? sigmaMax(cov) : null,
      };
    });
  }
  const corners = payload.detection?.corners ?? [];
  return corners.map((corner, index) => {
    const cov = covarianceOf(corner);
    const targetId = "id" in corner ? corner.id : null;
    return {
      id: `corner-${index}`,
      index,
      x: corner.position[0],
      y: corner.position[1],
      grid: corner.grid ? { i: corner.grid.u, j: corner.grid.v } : null,
      targetId,
      score: corner.score,
      sigma: cov ? sigmaMax(cov) : null,
    };
  });
}

/** Whether any corner of the payload carries a covariance, i.e. whether ellipses can be drawn. */
export function hasCovariance(payload: DetectPayload): boolean {
  const corners = payload.kind === "corners" ? payload.corners : (payload.detection?.corners ?? []);
  return corners.some((corner) => covarianceOf(corner) !== null);
}

/** Facts about the found target as label/value pairs, for the results panel. */
export function summaryFacts(payload: DetectPayload): [label: string, value: string][] {
  switch (payload.kind) {
    case "puzzleboard": {
      const decode = payload.detection?.decode;
      if (!decode) return [];
      return [
        ["bit error rate", `${(decode.bit_error_rate * 100).toFixed(1)} %`],
        ["edges matched", `${decode.edges_matched} / ${decode.edges_observed}`],
        ["master origin", `(${decode.master_origin_row}, ${decode.master_origin_col})`],
      ];
    }
    case "markerboard": {
      const { detection, diagnostics } = payload;
      if (!detection) return [];
      const facts: [string, string][] = [["board alignment", detection.alignment ? "found" : "none"]];
      // The three circles break the board's 4-fold symmetry: all must agree on one frame.
      if (diagnostics) facts.push(["circles matched", `${diagnostics.alignment_inliers} / 3`]);
      return facts;
    }
    case "chessboard":
      return payload.detection?.cell_size != null
        ? [["cell size", `${payload.detection.cell_size.toFixed(1)} px`]]
        : [];
    default:
      return [];
  }
}

/** The facts about the item a selection or hover names, e.g. `corner-3` or `marker-0`. */
export function itemFacts(payload: DetectPayload, rows: readonly CornerRow[], id: string): [string, string][] | null {
  const [part, text] = id.split("-");
  const index = Number(text);
  if (part === "corner") {
    const row = rows[index];
    if (!row) return null;
    const facts: [string, string][] = [];
    if (row.grid) facts.push(["(i, j)", `(${row.grid.i}, ${row.grid.j})`]);
    if (row.targetId !== null) facts.push(["id", String(row.targetId)]);
    facts.push(["score", row.score.toFixed(2)], ["x, y", `${row.x.toFixed(2)}, ${row.y.toFixed(2)} px`]);
    if (row.sigma !== null) facts.push(["σ max", `${row.sigma.toFixed(3)} px`]);
    return facts;
  }
  if (part === "marker" && payload.kind === "charuco") {
    const marker = payload.detection?.markers[index];
    if (!marker) return null;
    return [
      ["marker id", String(marker.id)],
      ["cell", `(${marker.gc.u}, ${marker.gc.v})`],
      ["hamming", String(marker.hamming)],
      ["score", marker.score.toFixed(2)],
    ];
  }
  if (part === "circle" && payload.kind === "markerboard") {
    const match = payload.diagnostics?.circle_matches[index];
    if (!match) return null;
    return [
      ["circle", match.expected.polarity],
      ["cell", `(${match.expected.cell.i}, ${match.expected.cell.j})`],
    ];
  }
  return null;
}

/** A short one-line form of `itemFacts` for the stage readout. */
export function itemLabel(payload: DetectPayload, rows: readonly CornerRow[], id: string): string | null {
  const facts = itemFacts(payload, rows, id);
  return facts ? facts.map(([label, value]) => `${label} ${value}`).join(" · ") : null;
}
