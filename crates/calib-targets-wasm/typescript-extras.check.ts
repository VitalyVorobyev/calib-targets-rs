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
  CharucoCorner,
  ChessboardCorner,
  ChessConfig,
  Corner,
  LabeledCorner,
  MarkerBoardCorner,
  PositionCovariance,
  PuzzleBoardCorner,
  ChessRefiner,
  DetectionStrategy,
  ForstnerConfig,
  MarkerBoardSpec,
  MarkerCircleSpec,
  PageSpec,
  PrintableMarkerCircleSpec,
  PrintableTargetDocument,
  RefinerConfig,
  RenderOptions,
  SaddlePointConfig,
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
    inner_square_rel: 0.4,
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

// `inner_square_rel` is optional; a document authored without it (above)
// must remain constructible, and one that names it must too.
export const chessboardDocWithInnerSquare: PrintableTargetDocument = {
  target: {
    kind: "chessboard",
    inner_rows: 6,
    inner_cols: 9,
    square_size_mm: 25.0,
    inner_square_rel: 0.4,
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
    inner_square_rel: 0.4,
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

export const poleDoc: PrintableTargetDocument = {
  target: {
    kind: "puzzlepole",
    circumference_squares: 18,
    start_row: 7,
    axial_squares: 8,
    square_size_mm: 13.0,
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

// The workspace default ChESS config, key for key as `default_chess_config()`
// returns it (`noise_sigma: null` = estimate it per image).
export const defaultChessConfig: ChessConfig = {
  strategy: { chess: { ring: "canonical", refiner: { center_of_mass: { radius: 2 } } } },
  threshold: 15.0,
  detection: { nms_radius: 2, min_cluster_size: 2 },
  multiscale: "single_scale",
  upscale: "disabled",
  orientation_method: "ring_fit",
  merge_radius: 3.0,
  noise_sigma: null,
  uncertainty: "propagated",
};

// The uncertainty knobs are optional, so a config written before they existed
// still type-checks; it also exercises the payload-carrying variants.
export const legacyChessConfig: ChessConfig = {
  strategy: { chess: { ring: "broad", refiner: { forstner: forstnerTuning() } } },
  threshold: 8.0,
  detection: { nms_radius: 3, min_cluster_size: 2 },
  multiscale: { pyramid: { levels: 3, min_size: 128, refinement_radius: 3 } },
  upscale: { fixed: 2 },
  orientation_method: null,
  merge_radius: 3.0,
};

export const quietChessConfig: ChessConfig = {
  ...defaultChessConfig,
  noise_sigma: 2.0,
  uncertainty: "off",
};

const saddle: SaddlePointConfig = {
  radius: 2,
  det_margin: 1e-3,
  max_offset: 1.5,
  min_abs_det: 1e-4,
};

function forstnerTuning(): ForstnerConfig {
  return { radius: 2, min_trace: 25.0, min_det: 1e-3, max_condition_number: 50.0, max_offset: 1.5 };
}

// Every refiner variant is a single-key tagged object; the deprecated
// `RefinerConfig` name accepts the same values.
export const refiners: ChessRefiner[] = [
  { center_of_mass: { radius: 2 } },
  { forstner: forstnerTuning() },
  { saddle_point: saddle },
];
export const legacyRefinerName: RefinerConfig = { saddle_point: saddle };

// The Radon strategy, with both final-position refiners.
export const radonStrategies: DetectionStrategy[] = [
  {
    radon: {
      ray_radius: 4,
      image_upsample: 2,
      response_blur_radius: 1,
      peak_fit: "gaussian",
      refiner: "peak_fit",
    },
  },
  {
    radon: {
      ray_radius: 4,
      image_upsample: 2,
      response_blur_radius: 1,
      peak_fit: "parabolic",
      refiner: { saddle_point: saddle },
    },
  },
];

// Corner shapes carry a per-corner position covariance (px²), `null` when the
// detector measured none. A hand-built input corner may omit it.
const cov: PositionCovariance = { xx: 0.01, xy: -0.002, yy: 0.012 };
const axes: Corner["axes"] = [
  { angle: 0.1, sigma: 0.01 },
  { angle: 1.67, sigma: 0.01 },
];
export const measuredCorner: Corner = {
  position: [10.5, 20.25],
  position_covariance: cov,
  axes,
  strength: 120.0,
};
export const handBuiltCorner: Corner = { position: [1, 2], axes, strength: 30.0 };
export const labeledCorner: LabeledCorner = {
  position: [10.5, 20.25],
  position_covariance: null,
  grid: { u: 1, v: 2 },
  id: null,
  target_position: null,
  score: 0.9,
};
export const chessboardCorner: ChessboardCorner = {
  position: [10.5, 20.25],
  position_covariance: cov,
  grid: { u: 0, v: 0 },
  input_index: 3,
  score: 0.9,
};
export const charucoCorner: CharucoCorner = {
  position: [10.5, 20.25],
  position_covariance: cov,
  grid: { u: 1, v: 1 },
  id: 0,
  target_position: [20, 20],
  score: 0.9,
};
export const markerBoardCorner: MarkerBoardCorner = {
  position: [10.5, 20.25],
  position_covariance: null,
  grid: { u: 1, v: 1 },
  id: 4,
  target_position: null,
  score: 0.9,
};
export const puzzleBoardCorner: PuzzleBoardCorner = {
  position: [10.5, 20.25],
  position_covariance: cov,
  grid: { u: 18, v: 219 },
  id: 109737,
  target_position: [90, 1095],
  score: 0.9,
};
