// Which parts of a detection the layer menu can toggle, and the detection with the hidden ones
// left out.

import type { TargetDetection } from "@vitavision/overlays";
import type { StageLayer } from "@vitavision/stage2d";

import type { LayerId } from "../state/types";

type Visibility = Record<LayerId, boolean>;

/** What the detection has to show, so the menu can say what is missing instead of hiding it. */
export interface LayerAvailability {
  lattice: boolean;
  markers: boolean;
  circles: boolean;
  bits: boolean;
  covariance: boolean;
}

export function availabilityOf(detection: TargetDetection, covariance: boolean): LayerAvailability {
  return {
    lattice: detection.kind !== "corners" && (detection.corners?.length ?? 0) > 0,
    markers: (detection.markers?.length ?? 0) > 0,
    circles: (detection.circles?.length ?? 0) > 0,
    bits: (detection.edgeBits?.length ?? 0) > 0,
    covariance,
  };
}

/** The detection without the parts the menu hid. Corners stay: edges and labels are overlay flags. */
export function visiblePartsOf(detection: TargetDetection, visible: Visibility): TargetDetection {
  const out: TargetDetection = { kind: detection.kind };
  if (detection.corners) out.corners = detection.corners;
  if (detection.markers && visible.markers) out.markers = detection.markers;
  if (detection.circles && visible.circles) out.circles = detection.circles;
  if (detection.edgeBits && visible.bits) out.edgeBits = detection.edgeBits;
  return out;
}

// Overlay role colours, as the stage paints them (stage2d/styles.css).
const FEATURE = "var(--stage-feature)";
const MODEL = "var(--stage-model)";
const STRUCTURE = "var(--stage-structure)";

export function layerMenu(visible: Visibility, available: LayerAvailability): StageLayer[] {
  return [
    { id: "overlay", label: "Detection", swatch: FEATURE, visible: visible.overlay },
    { id: "edges", label: "Grid edges", swatch: STRUCTURE, visible: visible.edges, disabled: !available.lattice },
    { id: "labels", label: "Labels", visible: visible.labels },
    { id: "markers", label: "ArUco markers", swatch: MODEL, visible: visible.markers, disabled: !available.markers },
    { id: "circles", label: "Marker circles", swatch: FEATURE, visible: visible.circles, disabled: !available.circles },
    { id: "bits", label: "Edge bits", swatch: MODEL, visible: visible.bits, disabled: !available.bits },
    {
      id: "ellipses",
      label: available.covariance ? "Uncertainty ellipses" : "Uncertainty ellipses (no covariance)",
      swatch: MODEL,
      visible: visible.ellipses,
      disabled: !available.covariance,
    },
  ];
}
