// The Generate form as a renderer request, and the names that go with it.

import type { GenerateForm, PrintableKind } from "../state/types";
import type { RenderRequest } from "../worker/protocol";

export function renderRequest(form: GenerateForm): RenderRequest {
  const { squareMm, dpi } = form;
  switch (form.kind) {
    case "chessboard":
      return { kind: "chessboard", innerRows: form.rows, innerCols: form.cols, squareMm, dpi };
    case "charuco":
      return {
        kind: "charuco",
        rows: form.rows,
        cols: form.cols,
        squareMm,
        markerRel: form.markerRel,
        dictionary: form.dictionary,
        dpi,
      };
    case "marker_board":
      return { kind: "marker_board", innerRows: form.rows, innerCols: form.cols, squareMm, dpi };
    case "puzzleboard":
      return { kind: "puzzleboard", rows: form.rows, cols: form.cols, squareMm, dpi };
    case "puzzlepole":
      return { kind: "puzzlepole", circumference: form.circumference, axial: form.axial, squareMm, dpi };
  }
}

export const KIND_LABEL: Record<PrintableKind, string> = {
  chessboard: "Chessboard",
  charuco: "ChArUco",
  marker_board: "Marker board",
  puzzleboard: "PuzzleBoard",
  puzzlepole: "PuzzlePole",
};

/** A file-name stem for a rendered target: `charuco-5x7`. */
export function stemOf(form: GenerateForm): string {
  const size = form.kind === "puzzlepole" ? `${form.circumference}x${form.axial}` : `${form.rows}x${form.cols}`;
  return `${form.kind.replace("_", "-")}-${size}`;
}
