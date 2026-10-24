// The studio's own layers over the image, besides the detection itself (`TargetOverlay`):
// the origin/far rings, the baseline diff, the topological diagnosis, and the A/B diff of the
// compare view. All are stage2d `PointSet` / `PolylineSet` layers, listed in the stage's layers
// menu by `StageLayersMenu`.
//
// Colour: the detection's marks keep the stage's overlay roles (feature, model, structure). A
// mark that is a verdict (a corner the diagnosis dropped, one the baseline lost) is painted in
// the design system's verdict colours. The bench CLI's PNG overlays keep their own palette
// (crates/calib-targets-bench/src/overlay.rs); the studio no longer mirrors it.

import { PointSet, PolylineSet, type PointSetItem, type StageLayer } from "@vitavision/stage2d";
import { useMemo } from "react";

import type { BaselineCorner, BaselineDiff, TopologicalDiagnosisWire } from "../api/types";

// --- layer visibility ---------------------------------------------------------

export type LayerId = "detection" | "edges" | "labels" | "rings" | "diff" | "ellipses" | "labelled" | "dropped";

export type Visibility = Record<LayerId, boolean>;

/** Everything on, except what the menu shows as off by default. */
export const DEFAULT_VISIBILITY: Visibility = {
  detection: true,
  edges: true,
  labels: false,
  rings: true,
  diff: true,
  ellipses: true,
  labelled: true,
  dropped: true,
};

// Role colours, as the stage paints them (stage2d/styles.css), and the verdict colours.
const FEATURE = "var(--stage-feature)";
const MODEL = "var(--stage-model)";
const STRUCTURE = "var(--stage-structure)";
const NORMAL = "var(--normal)";
const DEFECT = "var(--defect)";
const WARN = "var(--warn)";

/** What the detect view has to show, so the menu can say what is missing instead of hiding it. */
export interface DetectAvailability {
  baseline: boolean;
  covariance: boolean;
}

export function detectMenu(visible: Visibility, available: DetectAvailability): StageLayer[] {
  return [
    { id: "detection", label: "Corners", swatch: FEATURE, visible: visible.detection },
    { id: "edges", label: "Grid edges", swatch: STRUCTURE, visible: visible.edges },
    { id: "labels", label: "(i, j) labels", visible: visible.labels },
    { id: "rings", label: "Origin / far", swatch: MODEL, visible: visible.rings },
    {
      id: "diff",
      label: available.baseline ? "Baseline diff" : "Baseline diff (no baseline)",
      swatch: DEFECT,
      visible: visible.diff,
      disabled: !available.baseline,
    },
    {
      id: "ellipses",
      label: available.covariance ? "Uncertainty ellipses" : "Uncertainty ellipses (no covariance)",
      swatch: MODEL,
      visible: visible.ellipses,
      disabled: !available.covariance,
    },
  ];
}

export function diagnoseMenu(visible: Visibility): StageLayer[] {
  return [
    { id: "labelled", label: "Labelled", swatch: NORMAL, visible: visible.labelled },
    { id: "dropped", label: "Dropped", swatch: DEFECT, visible: visible.dropped },
  ];
}

// --- origin / far rings -------------------------------------------------------

/** The corner at the label-box minimum `(min i, min j)` and the one at the maximum, if present. */
function extremes(corners: readonly BaselineCorner[]) {
  if (corners.length === 0) return null;
  let minI = Infinity;
  let minJ = Infinity;
  let maxI = -Infinity;
  let maxJ = -Infinity;
  for (const c of corners) {
    minI = Math.min(minI, c.i);
    minJ = Math.min(minJ, c.j);
    maxI = Math.max(maxI, c.i);
    maxJ = Math.max(maxJ, c.j);
  }
  return {
    origin: corners.find((c) => c.i === minI && c.j === minJ),
    far: corners.find((c) => c.i === maxI && c.j === maxJ),
  };
}

/**
 * A ring on `(min i, min j)` and a square on `(max i, max j)`. The two are told apart by shape
 * and by their label, not by hue.
 */
export function RingsLayer({
  corners,
  onHover,
}: {
  corners: readonly BaselineCorner[];
  onHover: (text: string | null) => void;
}) {
  const items = useMemo<PointSetItem[]>(() => {
    const ends = extremes(corners);
    const out: PointSetItem[] = [];
    if (ends?.origin) {
      const { origin } = ends;
      out.push({ id: "origin", x: origin.x, y: origin.y, kind: "hollow", role: "model", label: "origin" });
    }
    if (ends?.far) {
      out.push({ id: "far", x: ends.far.x, y: ends.far.y, kind: "square", role: "model", label: "far" });
    }
    return out;
  }, [corners]);
  return (
    <PointSet
      items={items}
      label="Origin and far corners"
      onHoverChange={(id) =>
        onHover(
          id === "origin" ? "origin: corner at (min i, min j)" : id === "far" ? "far: corner at (max i, max j)" : null,
        )
      }
    />
  );
}

// --- baseline diff ------------------------------------------------------------

function gridKey(i: number, j: number): string {
  return `${i},${j}`;
}

/**
 * The run against its pinned baseline: a cross where a baseline corner went missing (at the
 * baseline position), a ring on every extra run corner, and a segment from baseline to run
 * position, with a ring at the run end, where a corner moved.
 */
export function BaselineDiffLayers({
  diff,
  baselineCorners,
  runCorners,
  onHover,
}: {
  diff: BaselineDiff;
  baselineCorners: readonly BaselineCorner[] | null;
  runCorners: readonly BaselineCorner[];
  onHover: (text: string | null) => void;
}) {
  const missing = useMemo<PointSetItem[]>(() => {
    const byGrid = new Map<string, BaselineCorner>();
    for (const c of baselineCorners ?? []) byGrid.set(gridKey(c.i, c.j), c);
    const out: PointSetItem[] = [];
    for (const [i, j] of diff.missing_labels) {
      const c = byGrid.get(gridKey(i, j));
      if (c) out.push({ id: `missing ${i},${j}`, x: c.x, y: c.y, kind: "cross", color: DEFECT });
    }
    return out;
  }, [diff, baselineCorners]);

  const extra = useMemo<PointSetItem[]>(() => {
    const byGrid = new Map<string, BaselineCorner>();
    for (const c of runCorners) byGrid.set(gridKey(c.i, c.j), c);
    const out: PointSetItem[] = [];
    for (const [i, j] of diff.extra_labels) {
      const c = byGrid.get(gridKey(i, j));
      if (c) out.push({ id: `extra ${i},${j}`, x: c.x, y: c.y, kind: "hollow", color: NORMAL });
    }
    return out;
  }, [diff, runCorners]);

  const moved = useMemo(
    () =>
      diff.wrong_position.map((wp) => ({
        id: `moved ${wp.i},${wp.j} by ${wp.drift_px.toFixed(2)} px`,
        points: [wp.baseline[0], wp.baseline[1], wp.run[0], wp.run[1]],
        stroke: WARN,
      })),
    [diff],
  );
  const movedEnds = useMemo<PointSetItem[]>(
    () =>
      diff.wrong_position.map((wp) => ({
        id: `moved ${wp.i},${wp.j} by ${wp.drift_px.toFixed(2)} px`,
        x: wp.run[0],
        y: wp.run[1],
        kind: "hollow",
        color: WARN,
      })),
    [diff],
  );

  const hover = (id: string | number | null) => onHover(id === null ? null : `baseline diff: ${id}`);
  return (
    <>
      <PolylineSet items={moved} label="Corners moved against the baseline" onHoverChange={hover} />
      <PointSet items={movedEnds} label="Moved corners" onHoverChange={hover} />
      <PointSet items={missing} label="Corners missing against the baseline" onHoverChange={hover} />
      <PointSet items={extra} label="Extra corners against the baseline" onHoverChange={hover} />
    </>
  );
}

// --- topological diagnosis ------------------------------------------------------

const deg = (rad: number) => ((rad * 180) / Math.PI).toFixed(1);

/** The text for a diagnosed corner: its class and the axis uncertainty the prefilter read. */
export function describeDiagnosed(c: TopologicalDiagnosisWire["corners"][number]): string {
  return `${c.labelled ? "labelled" : "dropped"} · axis σ (${deg(c.sigma0)}°, ${deg(c.sigma1)}°) · x ${c.x.toFixed(2)} · y ${c.y.toFixed(2)}`;
}

/** Every input corner of the diagnosis: labelled in the normal colour, dropped in the defect one. */
export function DiagnoseLayers({
  diagnosis,
  showLabelled,
  showDropped,
  onHover,
}: {
  diagnosis: TopologicalDiagnosisWire;
  showLabelled: boolean;
  showDropped: boolean;
  onHover: (text: string | null) => void;
}) {
  const { labelled, dropped } = useMemo(() => {
    const kept: PointSetItem[] = [];
    const lost: PointSetItem[] = [];
    diagnosis.corners.forEach((c, k) => {
      (c.labelled ? kept : lost).push({ id: k, x: c.x, y: c.y, kind: "dot", color: c.labelled ? NORMAL : DEFECT });
    });
    return { labelled: kept, dropped: lost };
  }, [diagnosis]);
  const hover = (id: string | number | null) => {
    const c = id === null ? undefined : diagnosis.corners[Number(id)];
    onHover(c === undefined ? null : describeDiagnosed(c));
  };
  return (
    <>
      {showDropped && <PointSet items={dropped} label="Dropped corners" labels={false} onHoverChange={hover} />}
      {showLabelled && <PointSet items={labelled} label="Labelled corners" labels={false} onHoverChange={hover} />}
    </>
  );
}

// --- compare: A/B diff overlay ----------------------------------------------------

/** What the A/B diff draws each class in: A and B take two roles, the corners both found the third. */
export const AB_COLORS = { a: FEATURE, b: MODEL, common: STRUCTURE } as const;

function positionKey(c: BaselineCorner): string {
  return `${c.x.toFixed(2)},${c.y.toFixed(2)}`;
}

/** Corners found by one run only, and by both, matched by position (to 0.01 px). */
export function abSplit(a: readonly BaselineCorner[], b: readonly BaselineCorner[]) {
  const bKeys = new Set(b.map(positionKey));
  const aKeys = new Set(a.map(positionKey));
  return {
    aOnly: a.filter((c) => !bKeys.has(positionKey(c))),
    bOnly: b.filter((c) => !aKeys.has(positionKey(c))),
    common: a.filter((c) => bKeys.has(positionKey(c))),
  };
}

function abItems(corners: readonly BaselineCorner[], where: string, kind: string, color: string): PointSetItem[] {
  return corners.map((c) => ({
    id: `${where} ${c.i},${c.j} @ ${c.x.toFixed(1)},${c.y.toFixed(1)}`,
    x: c.x,
    y: c.y,
    kind,
    color,
  }));
}

export function AbDiffLayers({
  split,
  onHover,
}: {
  split: ReturnType<typeof abSplit>;
  onHover: (text: string | null) => void;
}) {
  const common = useMemo(() => abItems(split.common, "common", "dot", AB_COLORS.common), [split.common]);
  const aOnly = useMemo(() => abItems(split.aOnly, "A only", "hollow", AB_COLORS.a), [split.aOnly]);
  const bOnly = useMemo(() => abItems(split.bOnly, "B only", "hollow", AB_COLORS.b), [split.bOnly]);
  const hover = (id: string | number | null) => onHover(id === null ? null : String(id));
  return (
    <>
      <PointSet items={common} label="Corners both runs found" onHoverChange={hover} />
      <PointSet items={aOnly} label="Corners only run A found" onHoverChange={hover} />
      <PointSet items={bOnly} label="Corners only run B found" onHoverChange={hover} />
    </>
  );
}
