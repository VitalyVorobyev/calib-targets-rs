import { DensityProvider, ErrorBox, Skeleton, Tabs } from "@vitavision/ui";
import { lazy, Suspense } from "react";

import { DetectPanel } from "./detect/DetectPanel";
import { useAppState, useDispatch } from "./state/AppContext";
import type { Tab } from "./state/types";

// Only opened on demand: keeps the first load to what the Detect tab shows.
const GeneratePanel = lazy(() =>
  import("./generate/GeneratePanel").then((module) => ({ default: module.GeneratePanel })),
);

const TABS = [
  { id: "detect", label: "Detect" },
  { id: "generate", label: "Generate" },
] satisfies { id: Tab; label: string }[];

export function Inspector() {
  const { tab, bootError } = useAppState();
  const dispatch = useDispatch();
  return (
    <DensityProvider value="compact">
      <div className="flex min-h-full flex-col">
        <div className="sticky top-0 z-10 border-b border-line bg-surface px-3 pt-2">
          <Tabs
            items={TABS}
            active={tab}
            onSelect={(id) => dispatch({ type: "setTab", tab: id })}
            label="Inspector"
            idPrefix="inspector"
          />
        </div>
        <div
          role="tabpanel"
          id={`inspector-panel-${tab}`}
          aria-labelledby={`inspector-tab-${tab}`}
          className="flex-1 p-3"
        >
          {bootError && <ErrorBox className="mb-3">{`The WebAssembly module did not load: ${bootError}`}</ErrorBox>}
          {tab === "detect" ? (
            <DetectPanel />
          ) : (
            <Suspense fallback={<Skeleton className="h-32 w-full" />}>
              <GeneratePanel />
            </Suspense>
          )}
        </div>
      </div>
    </DensityProvider>
  );
}
