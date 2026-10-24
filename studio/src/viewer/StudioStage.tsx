// The image, with whatever layers the screen puts over it, and the stage's own controls: the
// layers menu, the zoom toolbar, and a readout of the pixel under the pointer and of the item
// under it. Mounted with `key={label}` so a new image opens fitted.

import {
  ImageLayer,
  ImageStage,
  StageLayersMenu,
  StageReadout,
  StageToolbar,
  type StageLayer,
  type StageView,
} from "@vitavision/stage2d";
import { Progress, cn } from "@vitavision/ui";
import { useCallback, useState, type ReactNode } from "react";

import { imageUrl } from "../api/client";
import type { ImageSize } from "../hooks/useImageSize";

/** Where the pointer's hover text comes from: one entry per layer, so a layer that loses the
 *  pointer cannot blank the text of the layer that just gained it. */
export function useHoverText() {
  const [byLayer, setByLayer] = useState<Record<string, string | null>>({});
  const reporter = useCallback(
    (layer: string) => (text: string | null) =>
      setByLayer((prev) => (prev[layer] === text ? prev : { ...prev, [layer]: text })),
    [],
  );
  const text = Object.values(byLayer).find((t) => t != null) ?? null;
  return { text, reporter };
}

/** A note over the image, on the same halo as the stage's own readout. */
export function StageNote({ className, children }: { className?: string; children: string }) {
  return (
    <span className={cn("rounded-control bg-stage-halo px-1.5 py-0.5 text-[11px] text-stage-label", className)}>
      {children}
    </span>
  );
}

export interface StudioStageProps {
  label: string;
  size: ImageSize;
  layers: readonly StageLayer[];
  onLayer: (id: string, visible: boolean) => void;
  /** The text for the readout where the pointer is over an item. */
  hover: string | null;
  /** A shared view, so two stages pan and zoom together (compare). Otherwise the stage keeps its own. */
  view?: StageView | null;
  onView?: (view: StageView) => void;
  onBackgroundClick?: () => void;
  banner?: ReactNode;
  busy?: boolean;
  /** The stage layers: `TargetOverlay` and the studio's own sets. */
  children: ReactNode;
}

export function StudioStage({
  label,
  size,
  layers,
  onLayer,
  hover,
  view: sharedView,
  onView: onSharedView,
  onBackgroundClick,
  banner,
  busy,
  children,
}: StudioStageProps) {
  const [ownView, setOwnView] = useState<StageView | null>(null);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const shared = onSharedView !== undefined;
  return (
    <div className="relative h-full w-full">
      <ImageStage
        image={size}
        view={shared ? (sharedView ?? null) : ownView}
        onView={shared ? onSharedView : setOwnView}
        initialView="fit"
        fit={{ padding: 16 }}
        label={`Image canvas, ${label}`}
        className="bg-canvas"
        onHover={setCursor}
        {...(onBackgroundClick ? { onBackgroundClick } : {})}
        toolbar={
          <StageToolbar>
            <StageLayersMenu layers={layers} onVisibleChange={onLayer} />
          </StageToolbar>
        }
        readout={<StageReadout cursor={cursor} {...(hover === null ? {} : { extra: hover })} />}
        banner={banner}
      >
        <ImageLayer src={imageUrl(label)} alt={label} />
        {children}
      </ImageStage>
      {busy && <Progress aria-label="Detecting" className="absolute inset-x-0 top-0 rounded-none" />}
    </div>
  );
}
