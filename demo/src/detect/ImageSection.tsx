import { Button, Field } from "@vitavision/ui";
import { FileDrop } from "@vitavision/workbench";

import { SAMPLES } from "../lib/samples";
import { useActions, useAppState } from "../state/AppContext";

export function ImageSection() {
  const { image, imageStatus } = useAppState();
  const { loadFile, loadSample } = useActions();
  return (
    <div className="flex flex-col gap-2">
      {/* The window takes drops; this is the keyboard route to the same picker. */}
      <FileDrop
        overlay
        accept="image/*"
        multiple={false}
        buttonLabel="Upload image…"
        overlayMessage="Drop an image to detect on it"
        onFiles={([file]) => file && loadFile(file)}
      />
      <Field label="Sample images" as="group">
        <div className="flex flex-wrap gap-1.5">
          {SAMPLES.map((sample) => (
            <Button
              key={sample.id}
              size="sm"
              variant="secondary"
              aria-current={image?.name === sample.label ? "true" : undefined}
              className={image?.name === sample.label ? "ring-signal" : undefined}
              onClick={() => loadSample(sample)}
            >
              {sample.label}
            </Button>
          ))}
        </div>
      </Field>
      <p className="font-mono text-xs text-fg-muted" aria-live="polite">
        {imageStatus.status === "loading"
          ? `Opening ${imageStatus.name}…`
          : image
            ? `${image.name} · ${image.width}×${image.height} px`
            : "No image yet"}
      </p>
    </div>
  );
}
