// A detect response's corners, as what `TargetOverlay` draws, and the facts a hover or the
// corner inspector reads back out of them.
//
// Coordinates need no transform. The server reports them in the fed-image frame (post-crop,
// post-upscale: what the detector saw), which is also the frame of the PNG that
// `/api/image/{label}` serves, and calib-targets puts the centre of pixel `i` at `i`, the
// convention the stage uses too.

import { fromCalibTargets, type TargetDetection } from "@vitavision/overlays";

import type { BaselineCorner, PositionCovariance } from "../api/types";

/** The id `fromCalibTargets` gives the corner at `index` of the result. */
const CORNER_ID = /^corner-(\d+)$/;

/**
 * The corners as a `TargetDetection`.
 *
 * Every family is mapped as a chessboard: the wire carries the labelled corners (a ChArUco or
 * PuzzleBoard result adds a marker count or a decode summary, not geometry), and all the board
 * kinds draw the same lattice. A corner that has a detector id is labelled `#id`, otherwise
 * the overlay's own `i,j`.
 */
export function targetDetectionOf(corners: readonly BaselineCorner[]): TargetDetection {
  const mapped = fromCalibTargets({
    kind: "chessboard",
    detection: {
      corners: corners.map((c) => ({
        position: [c.x, c.y] as const,
        grid: { u: c.i, v: c.j },
        score: c.score,
        position_covariance: c.position_covariance,
      })),
    },
  });
  if (!corners.some((c) => c.id != null)) return mapped;
  return {
    ...mapped,
    corners: mapped.corners?.map((corner, index) => {
      const id = corners[index]?.id;
      return id == null ? corner : { ...corner, label: `#${id}` };
    }),
  };
}

/** The corner a `TargetOverlay` hit id names, or null for an id that is not one of the corners. */
export function cornerOfId(id: string, corners: readonly BaselineCorner[]): BaselineCorner | null {
  const match = CORNER_ID.exec(id);
  if (match === null) return null;
  return corners[Number(match[1])] ?? null;
}

/** The overlay id of the corner at `index`. */
export function idOfCorner(index: number): string {
  return `corner-${index}`;
}

/** Largest 1-sigma semi-axis in px: the square root of the covariance's larger eigenvalue. */
export function sigmaMax({ xx, xy, yy }: PositionCovariance): number {
  const mean = (xx + yy) / 2;
  const radius = Math.hypot((xx - yy) / 2, xy);
  return Math.sqrt(Math.max(mean + radius, 0));
}

/** The corner's σ_max, or null when the server reported no covariance for it. */
export function sigmaOf(corner: BaselineCorner): number | null {
  const cov = corner.position_covariance;
  return cov == null ? null : sigmaMax(cov);
}

/** Whether any corner carries a covariance, i.e. whether the ellipse layer has anything to draw. */
export function hasCovariance(corners: readonly BaselineCorner[]): boolean {
  return corners.some((c) => c.position_covariance != null);
}

/** One line about a corner, for the stage readout. */
export function describeCorner(c: BaselineCorner): string {
  const sigma = sigmaOf(c);
  return [
    `(i, j) = (${c.i}, ${c.j})`,
    c.id == null ? null : `id ${c.id}`,
    `x ${c.x.toFixed(2)} · y ${c.y.toFixed(2)}`,
    `score ${c.score.toFixed(1)}`,
    sigma === null ? null : `σ max ${sigma.toFixed(3)} px`,
  ]
    .filter((part) => part !== null)
    .join(" · ");
}
