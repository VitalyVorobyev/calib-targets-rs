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
  BlurDecline,
  BlurField,
  BlurFitQuality,
  BlurGradient,
  BlurKernel,
  BoardPhotometricOptions,
  BoardPhotometricRefinement,
  CharucoBoardSpec,
  CharucoCorner,
  ChessboardCorner,
  ChessboardDetection,
  ChessConfig,
  Corner,
  CornerBlur,
  CornerPhotometricFit,
  DetectionQuality,
  DisplaySurfaceModel,
  LabeledCorner,
  MarkerBoardCorner,
  PhotometricFitOptions,
  PositionCovariance,
  PuzzleBoardCorner,
  PuzzleBoardDecodeInfo,
  PuzzleBoardDetection,
  PuzzleBoardDiagnostics,
  PuzzlePoleCorner,
  PuzzlePoleDecodeConfig,
  PuzzlePoleDetection,
  PuzzlePoleParams,
  PuzzlePoleSpec,
  RadiometricModel,
  RefinedDetection,
  DetectionWithDiagnostics,
  Point3,
  DetectionStrategy,
  ImageRefiner,
  MarkerBoardSpec,
  MarkerCircleSpec,
  PageSpec,
  PrintableMarkerCircleSpec,
  PrintableTargetDocument,
  ScreenTargetBundle,
  ScreenTargetMetadata,
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
// returns it (no image refiner — a refiner opted into keeps the center of mass
// for the corners it rejects — and no position covariance).
export const defaultChessConfig: ChessConfig = {
  strategy: {
    chess: {
      ring: "canonical",
      center_of_mass: { radius: 2 },
      on_refiner_reject: "keep_center_of_mass",
    },
  },
  threshold: 15.0,
  detection: { nms_radius: 2, min_cluster_size: 2 },
  multiscale: "single_scale",
  upscale: "disabled",
  orientation_method: "ring_fit",
  merge_radius: 3.0,
  refiner: null,
  uncertainty: "off",
};

// `refiner` and `uncertainty` are optional, so a config that leaves them out
// still type-checks; it also exercises the payload-carrying variants.
export const minimalChessConfig: ChessConfig = {
  strategy: { chess: { ring: "broad", center_of_mass: { radius: 3 } } },
  threshold: 8.0,
  detection: { nms_radius: 3, min_cluster_size: 2 },
  multiscale: { pyramid: { levels: 3, min_size: 128, refinement_radius: 3 } },
  upscale: { fixed: 2 },
  orientation_method: null,
  merge_radius: 3.0,
};

const saddle: SaddlePointConfig = {
  radius: 2,
  det_margin: 1e-3,
  max_offset: 1.5,
  min_abs_det: 1e-4,
};

// The image refiner is a single-key tagged object.
export const saddleRefiner: ImageRefiner = { saddle_point: saddle };

// Every uncertainty variant: off, and propagated with an estimated or a known
// noise level.
export const covarianceConfigs: ChessConfig[] = [
  { ...defaultChessConfig, refiner: saddleRefiner, uncertainty: "off" },
  { ...defaultChessConfig, uncertainty: { propagated: { noise: "estimate" } } },
  { ...defaultChessConfig, uncertainty: { propagated: { noise: { known: 2.0 } } } },
];

// The Radon strategy, with both peak-fit modes.
export const radonStrategies: DetectionStrategy[] = [
  {
    radon: {
      ray_radius: 4,
      image_upsample: 2,
      response_blur_radius: 1,
      peak_fit: "gaussian",
    },
  },
  {
    radon: {
      ray_radius: 4,
      image_upsample: 2,
      response_blur_radius: 1,
      peak_fit: "parabolic",
    },
  },
];

// Corner shapes carry a per-corner position covariance (px²), `null` when the
// detector measured none. A hand-built input corner may omit it.
const cov: PositionCovariance = { xx: 0.01, xy: -0.002, yy: 0.012 };
const axes: Corner["axes"] = [
  { angle: 0.1, fit_std_error: 0.01 },
  { angle: 1.67, fit_std_error: 0.01 },
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

// Every detection result carries its localization quality; unavailable
// statistics are `null`, counts are numbers.
export const quality: DetectionQuality = {
  noise_sigma: 1.4,
  covariance_coverage: 1.0,
  median_sigma_px: 0.12,
  p95_sigma_px: 0.2,
  local_residual_count: 40,
  local_residual_rms_px: 0.25,
  variance_factor_count: 40,
  variance_factor: 1.3,
};
export const unmeasured: DetectionQuality = {
  noise_sigma: null,
  covariance_coverage: 0,
  median_sigma_px: null,
  p95_sigma_px: null,
  local_residual_count: 0,
  local_residual_rms_px: null,
  variance_factor_count: 0,
  variance_factor: null,
};
export const chessboardDetection: ChessboardDetection = {
  corners: [chessboardCorner],
  cell_size: 20.0,
  quality,
};


export const screenTargetMetadata: ScreenTargetMetadata = {
  screen: { width_px: 1200, height_px: 1600, pixel_pitch_mm: 0.085, margin_px: 20 },
  board_origin_px: [100, 400],
  requested_square_size_mm: 12,
  square_size_px: 125,
  target: { kind: "chessboard", inner_rows: 5, inner_cols: 7, square_size_mm: 10.625 },
};

export const screenTargetBundle: ScreenTargetBundle = {
  png_bytes: new Uint8Array(),
  json_text: JSON.stringify(screenTargetMetadata),
};

export const point3Sample: Point3 = [1.0, 2.0, 3.0];

export const puzzlePoleSpec: PuzzlePoleSpec = {
  circumference_squares: 24,
  start_row: 0,
  axial_start_col: 0,
  axial_squares: 12,
  cell_size_mm: 20.0,
};

export const puzzlePoleDecode: PuzzlePoleDecodeConfig = {
  min_circumference_span: 5,
  min_axial_span: 8,
  min_bit_confidence: 0.1,
  max_bit_error_rate: 0.15,
  search_all_components: true,
  sample_radius_rel: 0.2,
  scoring_mode: { kind: "soft_log_likelihood" },
  symmetry_mode: { kind: "rotations" },
};

export const puzzlePoleParams: PuzzlePoleParams = {
  pole: puzzlePoleSpec,
  decode: puzzlePoleDecode,
};

export const puzzlePoleCorner: PuzzlePoleCorner = {
  position: [10.5, 20.25],
  position_covariance: cov,
  grid: { u: 3, v: 4 },
  id: 42,
  surface_position: [60.0, 80.0],
  object_position: point3Sample,
  score: 12.5,
};

const puzzlePoleDecodeInfo: PuzzleBoardDecodeInfo = {
  edges_observed: 40,
  edges_matched: 39,
  mean_confidence: 0.9,
  bit_error_rate: 0.025,
  logical_bits: 16,
  logical_bit_error_rate: 0.0,
  dot_dissent_rate: 0.0,
  master_origin_row: 0,
  master_origin_col: 0,
};

export const puzzlePoleDetection: PuzzlePoleDetection = {
  corners: [puzzlePoleCorner],
  alignment: {
    lattice: "square",
    matrix: [
      [1, 0],
      [0, 1],
    ],
    translation: [0, 0],
  },
  decode: puzzlePoleDecodeInfo,
  spec: puzzlePoleSpec,
  quality,
};

export const puzzlePoleDiagnoseResult: DetectionWithDiagnostics<
  PuzzlePoleDetection,
  PuzzleBoardDiagnostics
> = {
  result: puzzlePoleDetection,
  diagnostics: {
    observed_edges: [],
    decode: {
      score_margin: 12.0,
    },
  },
};

export const radiometricModels: RadiometricModel[] = [
  "linear",
  "srgb",
  { fixed_gamma: { gamma: 2.2 } },
  { free_gamma: { initial_gamma: 2.2 } },
];

export const einkDisplaySurface: DisplaySurfaceModel = {
  display_pixels_per_cell: 12.0,
  mtf_sigma_display_px: 0.5,
  gray_levels: 16,
  rasterize_curved_features: true,
};

export const blurGradientSample: BlurGradient = {
  d_sigma_dx: [0.01, -0.002, 0.005],
  d_sigma_dy: [-0.003, 0.004, 0.01],
};

export const photometricFitOpts: PhotometricFitOptions = {
  radiometric: radiometricModels[1],
  fit_ink_bleed: false,
  use_varpro: true,
  display: einkDisplaySurface,
  known_blur_gradient: blurGradientSample,
};

export const boardPhotometricOpts: BoardPhotometricOptions = {
  fit: photometricFitOpts,
  puzzle_dot_diameter_rel: 1.0 / 3.0,
  two_pass_scatter_correction: true,
};

const sampleBlurKernel: BlurKernel = { xx: 4.0, xy: -0.1, yy: 4.2 };

const sampleBlurQuality: BlurFitQuality = {
  window_cells: 1.5,
  pixels: 256,
  contrast: 150.0,
  residual_rms: 1.2,
  residual_autocorrelation: 0.02,
};

const sampleCornerBlur: CornerBlur = {
  sigma_px: 2.02,
  sigma_std_error_px: 0.03,
  kernel: sampleBlurKernel,
  kernel_covariance: [
    [0.01, 0.0, 0.0],
    [0.0, 0.01, 0.0],
    [0.0, 0.0, 0.01],
  ],
  fit: sampleBlurQuality,
};

const sampleCornerFit: CornerPhotometricFit = {
  position: [120.5, 84.25],
  position_covariance: cov,
  axes: [
    { angle: 0.0, fit_std_error: 0.002 },
    { angle: Math.PI / 2, fit_std_error: 0.002 },
  ],
  blur: sampleCornerBlur,
  blur_decline: null,
  display_blur: { xx: 0.05, xy: 0.0, yy: 0.05 },
  fit: sampleBlurQuality,
  gamma: null,
  ink_bleed_cells: null,
  scatter_correction_px: [0.002, -0.001],
};

const sampleDecline: BlurDecline = "SharperThanPixel";

const sampleBlurField: BlurField = {
  center: [180.0, 180.0],
  scale: 150.0,
  degree: 2,
  coeffs: [
    [4.0, 0.01, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [4.1, 0.0, 0.01, 0.0, 0.0, 0.0],
  ],
  sample_count: 16,
};

export const sampleRefinement: BoardPhotometricRefinement = {
  corners: [{ Ok: sampleCornerFit }, { Err: sampleDecline }],
  blur_field: sampleBlurField,
};

const samplePuzzleCorner: PuzzleBoardCorner = {
  position: [120.5, 84.25],
  position_covariance: cov,
  grid: { u: 2, v: 3 },
  id: 7,
  target_position: [6.4, 9.6],
  score: 42.0,
};

const samplePuzzleDetection: PuzzleBoardDetection = {
  corners: [samplePuzzleCorner],
  alignment: {
    lattice: "square",
    matrix: [
      [1, 0],
      [0, 1],
    ],
    translation: [0, 0],
  },
  decode: puzzlePoleDecodeInfo,
  quality,
};

export const sampleRefinedPuzzleBoard: RefinedDetection<PuzzleBoardDetection> = {
  detection: samplePuzzleDetection,
  refinement: sampleRefinement,
};
