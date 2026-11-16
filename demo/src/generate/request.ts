// The Generate form as a renderer request, and the names that go with it.

import type { GenerateForm, PrintableKind } from "../state/types";
import type { PrintRenderRequest, RenderRequest, ScreenRenderRequest } from "../worker/protocol";

export function renderRequest(form: GenerateForm): RenderRequest {
  return form.medium === "screen" ? screenRequest(form) : printRequest(form);
}

function screenRequest(form: GenerateForm): ScreenRenderRequest {
  const { rows, cols, squareMm, screenWidth, screenHeight, screenPitchMm, screenMargin } = form;
  const screen = {
    width_px: screenWidth ?? 0,
    height_px: screenHeight ?? 0,
    pixel_pitch_mm: screenPitchMm ?? 0,
    margin_px: screenMargin,
  };
  switch (form.kind) {
    case "chessboard":
      return {
        kind: "screen",
        screen,
        target: { kind: "chessboard", inner_rows: rows, inner_cols: cols, square_size_mm: squareMm },
      };
    case "charuco":
      return {
        kind: "screen",
        screen,
        target: {
          kind: "charuco",
          rows,
          cols,
          square_size_mm: squareMm,
          marker_size_rel: form.markerRel,
          dictionary: form.dictionary,
        },
      };
    case "puzzleboard":
      return { kind: "screen", screen, target: { kind: "puzzle_board", rows, cols, square_size_mm: squareMm } };
    default:
      throw new Error("Selected target is unavailable in screen mode.");
  }
}

function printRequest(form: GenerateForm): PrintRenderRequest {
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
  const suffix = form.medium === "screen" ? "-screen" : "";
  return `${form.kind.replace("_", "-")}-${size}${suffix}`;
}
