import { Button, Callout, ErrorBox, Field, Select } from "@vitavision/ui";

import { downloadBlob } from "../lib/download";
import { useActions, useAppState, useDispatch } from "../state/AppContext";
import type { GenerateForm, PrintableKind } from "../state/types";
import type { RenderedTarget } from "../worker/protocol";
import { GeometryForm } from "./GeometryForm";
import { KIND_LABEL } from "./request";

const KINDS = (Object.keys(KIND_LABEL) as PrintableKind[]).map((value) => ({ value, label: KIND_LABEL[value] }));
const SCREEN_KINDS = KINDS.filter(({ value }) => value !== "marker_board" && value !== "puzzlepole");
const MEDIUMS = [
  { value: "print", label: "Print" },
  { value: "screen", label: "Screen" },
];

const FORMATS = [
  {
    label: "PNG",
    extension: "png",
    type: "image/png",
    content: (result: RenderedTarget) => result.bundle.png_bytes.slice(),
  },
  {
    label: "SVG",
    extension: "svg",
    type: "image/svg+xml",
    content: (result: RenderedTarget) => (result.medium === "print" ? result.bundle.svg_text : ""),
  },
  {
    label: "JSON",
    extension: "json",
    type: "application/json",
    content: (result: RenderedTarget) => result.bundle.json_text,
  },
  {
    label: "DXF",
    extension: "dxf",
    type: "image/vnd.dxf",
    content: (result: RenderedTarget) => (result.medium === "print" ? result.bundle.dxf_text : ""),
  },
];
const SCREEN_FORMATS = FORMATS.filter(({ extension }) => extension === "png" || extension === "json");

const millimeters = new Intl.NumberFormat(undefined, { maximumSignificantDigits: 8 });
const difference = new Intl.NumberFormat(undefined, { maximumSignificantDigits: 8, signDisplay: "exceptZero" });

export function GeneratePanel() {
  const { gen, genStatus, generated } = useAppState();
  const dispatch = useDispatch();
  const { generate } = useActions();
  const busy = genStatus.status === "busy";
  const screen = gen.medium === "screen";
  const missingScreenGeometry =
    screen && (gen.screenWidth === null || gen.screenHeight === null || gen.screenPitchMm === null);
  const lastOutputIsScreen = (generated?.rendered.medium ?? gen.medium) === "screen";
  const metadata = generated?.rendered.medium === "screen" ? generated.rendered.metadata : null;

  return (
    <div className="flex flex-col gap-4">
      <Field label="Output">
        <Select
          value={gen.medium}
          onValueChange={(medium) => dispatch({ type: "setGenerateMedium", medium: medium as GenerateForm["medium"] })}
          options={MEDIUMS}
          aria-label="Output medium"
        />
      </Field>
      <Field label="Target">
        <Select
          value={gen.kind}
          onValueChange={(kind) => dispatch({ type: "setKind", kind: kind as PrintableKind })}
          options={screen ? SCREEN_KINDS : KINDS}
          aria-label="Target to generate"
        />
      </Field>
      <GeometryForm />
      {screen && (
        <p className="text-xs text-fg-muted">
          Display the downloaded PNG at one image pixel per screen pixel, with zoom and fitting disabled.
        </p>
      )}
      {gen.kind === "puzzlepole" && (
        <Callout tone="info">
          There is no PuzzlePole detector yet. The strip loads as an ordinary PuzzleBoard, which the decoder reads as a
          window of the master pattern.
        </Callout>
      )}
      <Button variant="primary" loading={busy} disabled={busy || missingScreenGeometry} onClick={generate}>
        {busy ? "Generating…" : "Generate & load"}
      </Button>
      {genStatus.status === "error" && <ErrorBox>{genStatus.message}</ErrorBox>}

      <Field
        label="Download the last target"
        as="group"
        description={generated ? generated.label : "Generate one first."}
      >
        {metadata && (
          <div className="mb-2 text-xs text-fg-muted" role="status">
            <div>
              {metadata.screen.width_px} × {metadata.screen.height_px} px
            </div>
            <div>
              Square size {millimeters.format(metadata.target.square_size_mm)} mm ({metadata.square_size_px} px).
              Requested {millimeters.format(metadata.requested_square_size_mm)} mm (
              {difference.format(metadata.target.square_size_mm - metadata.requested_square_size_mm)} mm difference).
            </div>
            {metadata.target.kind === "charuco" && (
              <div>Marker side {millimeters.format(metadata.target.marker_size_rel * 100)}% of square.</div>
            )}
          </div>
        )}
        <div className="flex flex-wrap gap-1.5">
          {(lastOutputIsScreen ? SCREEN_FORMATS : FORMATS).map(({ label, extension, type, content }) => (
            <Button
              key={extension}
              size="sm"
              variant="secondary"
              disabled={generated === null}
              onClick={() =>
                generated &&
                downloadBlob(new Blob([content(generated.rendered)], { type }), `${generated.stem}.${extension}`)
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
