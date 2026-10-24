import { Section, Skeleton } from "@vitavision/ui";
import { lazy, Suspense } from "react";

import { ResultsPanel } from "../results/ResultsPanel";
import { ImageSection } from "./ImageSection";
import { RunBar } from "./RunBar";
import { TargetSection } from "./TargetSection";

// The settings forms and the JSON Schemas behind them are the bulk of the bundle, and nobody
// needs them until a section is opened.
const SettingsSection = lazy(() => import("./SettingsSection").then((module) => ({ default: module.SettingsSection })));

export function DetectPanel() {
  return (
    <div className="flex flex-col gap-4">
      <Section title="Image">
        <ImageSection />
      </Section>
      <Section title="Target">
        <TargetSection />
      </Section>
      <RunBar />
      <Section title="Result">
        <ResultsPanel />
      </Section>
      <Section title="Detector settings">
        <Suspense fallback={<Skeleton className="h-16 w-full" />}>
          <SettingsSection />
        </Suspense>
      </Section>
    </div>
  );
}
