// The image, the detection over it, and the stage's own controls.

import { TargetOverlay, type TargetDetection, type TargetHit } from "@vitavision/overlays";
import {
  ImageLayer,
  ImageStage,
  StageLayersMenu,
  StageReadout,
  StageToolbar,
  type StageView,
} from "@vitavision/stage2d";
import { Progress, cn } from "@vitavision/ui";
import { useMemo, useState } from "react";

import type { ImageRecord } from "../lib/image";
import type { LayerId } from "../state/types";
import { availabilityOf, layerMenu, visiblePartsOf } from "./layers";

export interface ViewerStageProps {
  image: ImageRecord;
  detection: TargetDetection | null;
  covariance: boolean;
  layers: Record<LayerId, boolean>;
  onLayer: (layer: LayerId, visible: boolean) => void;
  ellipseScale: number;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** The text for the readout where the pointer is over an item. */
  describe: (id: string) => string | null;
  running: boolean;
  stale: boolean;
}

/** A note over the image, on the same halo as the stage's own readout. */
function StageNote({ className, children }: { className?: string; children: string }) {
  return (
    <span className={cn("rounded-control bg-stage-halo px-1.5 py-0.5 text-[11px] text-stage-label", className)}>
      {children}
    </span>
  );
}

const EMPTY: TargetDetection = { kind: "corners" };

/** Mounted with `key={image.key}`: a new image starts from a fresh, fitted view. */
export function ViewerStage({
  image,
  detection,
  covariance,
  layers,
  onLayer,
  ellipseScale,
  selectedId,
  onSelect,
  describe,
  running,
  stale,
}: ViewerStageProps) {
  const [view, setView] = useState<StageView | null>(null);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);

  const shown = useMemo(() => visiblePartsOf(detection ?? EMPTY, layers), [detection, layers]);
  const menu = useMemo(
    () => layerMenu(layers, availabilityOf(detection ?? EMPTY, covariance)),
    [detection, covariance, layers],
  );
  const selected = useMemo(() => (selectedId === null ? [] : [selectedId]), [selectedId]);
  const ellipses = layers.ellipses && covariance;
  const uncertainty = useMemo(() => (ellipses ? { scale: ellipseScale } : undefined), [ellipses, ellipseScale]);
  const onHoverChange = (hit: TargetHit | null) => setHovered(hit === null ? null : String(hit.id));

  return (
    <div className="relative h-full w-full">
      <ImageStage
        image={{ width: image.width, height: image.height }}
        view={view}
        onView={setView}
        initialView="fit"
        fit={{ padding: 16 }}
        label={`Image canvas, ${image.name}`}
        className="bg-canvas"
        onHover={setCursor}
        onBackgroundClick={() => onSelect(null)}
        toolbar={
          <StageToolbar>
            <StageLayersMenu layers={menu} onVisibleChange={(id, visible) => onLayer(id as LayerId, visible)} />
          </StageToolbar>
        }
        readout={<StageReadout cursor={cursor} extra={hovered === null ? undefined : describe(hovered)} />}
        banner={
          <div className="flex flex-wrap gap-1.5">
            {stale && <StageNote className="ring-1 ring-warn">Settings changed since this result</StageNote>}
            {ellipses && <StageNote>{`Uncertainty ellipses: 1σ × ${ellipseScale}`}</StageNote>}
          </div>
        }
      >
        <ImageLayer src={image.url} alt={image.name} />
        {detection !== null && layers.overlay && (
          <TargetOverlay
            detection={shown}
            showEdges={layers.edges}
            showLabels={layers.labels}
            selectedIds={selected}
            onHoverChange={onHoverChange}
            onItemPress={(hit) => onSelect(String(hit.id))}
            uncertainty={uncertainty}
            dimmed={stale}
          />
        )}
      </ImageStage>
      {running && <Progress aria-label="Detecting" className="absolute inset-x-0 top-0 rounded-none" />}
    </div>
  );
}
