import { Button, Callout, ErrorBox, Field, Select } from "@vitavision/ui";

import type { GeneratedTargetBundle } from "@vitavision/calib-targets";
import { downloadBlob } from "../lib/download";
import { useActions, useAppState, useDispatch } from "../state/AppContext";
import type { PrintableKind } from "../state/types";
import { GeometryForm } from "./GeometryForm";
import { KIND_LABEL } from "./request";

const KINDS = (Object.keys(KIND_LABEL) as PrintableKind[]).map((value) => ({ value, label: KIND_LABEL[value] }));

/** The renderer's bundle, one download per format. */
const FORMATS: {
  label: string;
  extension: string;
  type: string;
  content: (bundle: GeneratedTargetBundle) => BlobPart;
}[] = [
  { label: "PNG", extension: "png", type: "image/png", content: (bundle) => bundle.png_bytes.slice() },
  { label: "SVG", extension: "svg", type: "image/svg+xml", content: (bundle) => bundle.svg_text },
  { label: "JSON", extension: "json", type: "application/json", content: (bundle) => bundle.json_text },
  { label: "DXF", extension: "dxf", type: "image/vnd.dxf", content: (bundle) => bundle.dxf_text },
];

export function GeneratePanel() {
  const { gen, genStatus, generated } = useAppState();
  const dispatch = useDispatch();
  const { generate } = useActions();
  const busy = genStatus.status === "busy";

  return (
    <div className="flex flex-col gap-4">
      <p className="text-xs text-fg-muted">
        Render a printable target in WebAssembly, load it as the image, and point the detector at the same board.
      </p>
      <Field label="Target">
        <Select
          value={gen.kind}
          onValueChange={(kind) => dispatch({ type: "setKind", kind: kind as PrintableKind })}
          options={KINDS}
          aria-label="Target to generate"
        />
      </Field>
      <GeometryForm />
      {gen.kind === "puzzlepole" && (
        <Callout tone="info">
          There is no PuzzlePole detector yet. The strip loads as an ordinary PuzzleBoard, which the decoder reads as a
          window of the master pattern.
        </Callout>
      )}
      <Button variant="primary" loading={busy} disabled={busy} onClick={generate}>
        {busy ? "Generating…" : "Generate & load"}
      </Button>
      {genStatus.status === "error" && <ErrorBox>{genStatus.message}</ErrorBox>}

      <Field
        label="Download the last target"
        as="group"
        description={generated ? generated.label : "Generate one first."}
      >
        <div className="flex flex-wrap gap-1.5">
          {FORMATS.map(({ label, extension, type, content }) => (
            <Button
              key={extension}
              size="sm"
              variant="secondary"
              disabled={generated === null}
              onClick={() =>
                generated &&
                downloadBlob(new Blob([content(generated.rendered.bundle)], { type }), `${generated.stem}.${extension}`)
              }
            >
              {label}
            </Button>
          ))}
        </div>
      </Field>
    </div>
  );
}
