// Construction check for the hand-written declarations in
// `typescript-extras.d.ts`.
//
// Type-checking the `.d.ts` on its own proves only that it *parses*. It cannot
// see a declaration that is unsatisfiable, and TypeScript merges two
// same-named `export interface` declarations in one module silently — which is
// how `MarkerCircleSpec` was declared twice, once for the printable model and
// once for the detector, leaving a merged interface that required the fields of
// both and could therefore never be constructed. Both marker-board types were
// unusable for an npm consumer and every existing check stayed green.
//
// So this file *builds a value* of each exported shape. It is checked in CI
// next to the declarations; it ships nowhere.

import type {
  CharucoBoardSpec,
  MarkerBoardSpec,
  MarkerCircleSpec,
  PageSpec,
  PrintableMarkerCircleSpec,
  PrintableTargetDocument,
  RenderOptions,
} from "./typescript-extras";

// The printable document the WASM `render_target_bundle_json` accepts, in the
// shape its rustdoc and the `testdata/printable/*.json` fixtures use.
export const charucoDoc: PrintableTargetDocument = {
  schema_version: 1,
  target: {
    kind: "charuco",
    rows: 5,
    cols: 7,
    square_size_mm: 20.0,
    marker_size_rel: 0.75,
    dictionary: "DICT_4X4_50",
    marker_layout: "opencv_charuco",
    border_bits: 2,
  },
  page: {
    size: { kind: "letter" },
    orientation: "landscape",
    margin_mm: 15.0,
  },
  render: { debug_annotations: false, png_dpi: 300 },
};

// `page` and `render` are optional, and a custom page size is expressible.
export const chessboardDoc: PrintableTargetDocument = {
  target: {
    kind: "chessboard",
    inner_rows: 6,
    inner_cols: 9,
    square_size_mm: 25.0,
  },
};

export const customPage: PageSpec = {
  size: { kind: "custom", width_mm: 300.0, height_mm: 200.0 },
  orientation: "portrait",
  margin_mm: 0.0,
};

export const render: RenderOptions = { debug_annotations: true, png_dpi: 600 };

// The marker board is the shape that regressed: the printable circles are
// `{ i, j, polarity }` and there are exactly three of them.
export const markerBoardDoc: PrintableTargetDocument = {
  target: {
    kind: "marker_board",
    inner_rows: 6,
    inner_cols: 8,
    square_size_mm: 20.0,
    circles: [
      { i: 1, j: 1, polarity: "black" },
      { i: 6, j: 1, polarity: "black" },
      { i: 1, j: 4, polarity: "white" },
    ],
    circle_diameter_rel: 0.5,
  },
};

export const puzzleDoc: PrintableTargetDocument = {
  target: {
    kind: "puzzle_board",
    rows: 10,
    cols: 10,
    square_size_mm: 10.0,
    origin_row: 333,
    origin_col: 333,
  },
};

export const printableCircle: PrintableMarkerCircleSpec = {
  i: 0,
  j: 0,
  polarity: "white",
};

// The detector-side spec keeps the `MarkerCircleSpec` name and its `cell`
// shape; the two must stay constructible side by side.
export const detectorCircle: MarkerCircleSpec = {
  cell: { i: 0, j: 0 },
  polarity: "white",
};

export const detectorBoard: MarkerBoardSpec = {
  rows: 7,
  cols: 9,
  cell_size: 20.0,
  circles: [
    { cell: { i: 1, j: 1 }, polarity: "black" },
    { cell: { i: 6, j: 1 }, polarity: "black" },
    { cell: { i: 1, j: 4 }, polarity: "white" },
  ],
};

// The detector board carries `border_bits` too, and `marker_layout` has a
// serde default, so neither is required to name a board.
export const detectorCharucoBoard: CharucoBoardSpec = {
  rows: 5,
  cols: 7,
  cell_size: 20.0,
  marker_size_rel: 0.75,
  dictionary: "DICT_4X4_50",
  border_bits: 2,
};
