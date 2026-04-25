// TypeScript object-shape declarations for `@vitavision/calib-targets`.
//
// The auto-generated `calib_targets_wasm.d.ts` (above) types only the
// function signatures. This block describes the JSON-serialised shapes
// of every parameter and result type that crosses the WASM boundary.
//
// Names and field layouts mirror the Rust serde structs verbatim; renaming
// any Rust field requires an update here. Public enums on the Rust side
// are `#[non_exhaustive]`, so consumer match statements should default
// to a `_:` arm.

// ---------------------------------------------------------------------------
// Geometry primitives
// ---------------------------------------------------------------------------

/** `nalgebra::Point2<f32>` serialises as a 2-tuple. */
export type Point2 = [number, number];

export interface GridCoords {
  i: number;
  j: number;
}

export interface GridTransform {
  a: number;
  b: number;
  c: number;
  d: number;
}

export interface GridAlignment {
  transform: GridTransform;
  /** Integer translation `[tx, ty]` in grid units. */
  translation: [number, number];
}

// ---------------------------------------------------------------------------
// Corner outputs
// ---------------------------------------------------------------------------

export interface AxisEstimate {
  angle: number;
  sigma: number;
}

export interface Corner {
  position: Point2;
  /** Two orthogonal grid axes (`axes[1] − axes[0] ≈ π/2`). */
  axes: [AxisEstimate, AxisEstimate];
  contrast: number;
  fit_rms: number;
  strength: number;
}

export type TargetKind =
  | "chessboard"
  | "charuco"
  | "checkerboard_marker"
  | "puzzle_board";

export interface LabeledCorner {
  position: Point2;
  /** Integer `(i, j)` grid label, rebased so the bounding-box minimum is `(0, 0)`. */
  grid: GridCoords | null;
  /** Logical target ID (ChArUco marker-referenced corner, PuzzleBoard master ID). */
  id: number | null;
  /** Physical position in mm on the printed board, when known. */
  target_position: Point2 | null;
  /** Detector-specific quality score (higher is better). */
  score: number;
}

export interface TargetDetection {
  kind: TargetKind;
  corners: LabeledCorner[];
}

// ---------------------------------------------------------------------------
// Detector-specific result types
// ---------------------------------------------------------------------------

export interface ChessboardDetectionResult {
  /** Two orthogonal grid-axis angles in radians, `axes[1] − axes[0] ≈ π/2`. */
  grid_directions: [number, number];
  cell_size: number;
  target: TargetDetection;
  /** Indices into the input `corners` slice, in the same order as `target.corners`. */
  strong_indices: number[];
}

export interface MarkerDetection {
  id: number;
  gc: GridCoords;
  rotation: number;
  hamming: number;
  score: number;
  border_score: number;
  code: number;
  inverted: boolean;
  corners_rect: [Point2, Point2, Point2, Point2];
  corners_img: [Point2, Point2, Point2, Point2] | null;
}

export interface CharucoDetectionResult {
  detection: TargetDetection;
  markers: MarkerDetection[];
  alignment: GridAlignment;
  raw_marker_count: number;
  raw_marker_wrong_id_count: number;
}

export interface CircleCandidate {
  center: Point2;
  score: number;
  polarity: "white" | "black";
}

export interface CircleMatch {
  cell: GridCoords;
  candidate_index: number;
  polarity: "white" | "black";
}

export interface MarkerBoardDetectionResult {
  detection: TargetDetection;
  inliers: number[];
  circle_candidates: CircleCandidate[];
  circle_matches: CircleMatch[];
  alignment: GridAlignment | null;
  alignment_inliers: number;
}

export interface PuzzleBoardObservedEdge {
  row: number;
  col: number;
  orientation: "horizontal" | "vertical";
  bit: 0 | 1;
  confidence: number;
}

export interface PuzzleBoardDecodeInfo {
  edges_observed: number;
  edges_matched: number;
  mean_confidence: number;
  bit_error_rate: number;
  master_origin_row: number;
  master_origin_col: number;
  /** Winning hypothesis score (soft-mode log-likelihood, hard-mode confidence sum). */
  score_best?: number;
  score_runner_up?: number;
  score_margin?: number;
  runner_up_origin_row?: number;
  runner_up_origin_col?: number;
  runner_up_transform?: GridTransform;
  scoring_mode?: PuzzleBoardScoringMode;
}

export interface PuzzleBoardDetectionResult {
  detection: TargetDetection;
  alignment: GridAlignment;
  decode: PuzzleBoardDecodeInfo;
  observed_edges: PuzzleBoardObservedEdge[];
}

// ---------------------------------------------------------------------------
// Parameters: ChESS corners
// ---------------------------------------------------------------------------

export type ThresholdMode = "relative" | "absolute";
export type DetectorMode = "canonical" | "broad";
export type DescriptorMode = "follow_detector" | "canonical" | "broad";

export interface RefinerConfig {
  kind: "center_of_mass" | "forstner" | "saddle_point";
  center_of_mass: { radius: number };
  forstner: {
    radius: number;
    min_trace: number;
    min_det: number;
    max_condition_number: number;
    max_offset: number;
  };
  saddle_point: {
    radius: number;
    det_margin: number;
    max_offset: number;
    min_abs_det: number;
  };
}

export type UpscaleConfig =
  | { mode: "disabled" }
  | { mode: "fixed"; factor: number }
  | { mode: "adaptive"; min_corners: number };

export interface ChessConfig {
  detector_mode: DetectorMode;
  descriptor_mode: DescriptorMode;
  threshold_mode: ThresholdMode;
  threshold_value: number;
  nms_radius: number;
  min_cluster_size: number;
  refiner: RefinerConfig;
  pyramid_levels: number;
  pyramid_min_size: number;
  refinement_radius: number;
  merge_radius: number;
  upscale: UpscaleConfig;
}

// ---------------------------------------------------------------------------
// Parameters: chessboard detector (flat 30-ish field shape).
// ---------------------------------------------------------------------------

export interface ChessboardParams {
  min_corner_strength: number;
  max_fit_rms_ratio: number;
  num_bins: number;
  max_iters_2means: number;
  cluster_tol_deg: number;
  peak_min_separation_deg: number;
  min_peak_weight_fraction: number;
  cell_size_hint?: number;
  seed_edge_tol: number;
  seed_axis_tol_deg: number;
  seed_close_tol: number;
  attach_search_rel: number;
  attach_axis_tol_deg: number;
  attach_ambiguity_factor: number;
  step_tol: number;
  edge_axis_tol_deg: number;
  line_tol_rel: number;
  projective_line_tol_rel: number;
  line_min_members: number;
  local_h_tol_rel: number;
  max_validation_iters: number;
  enable_line_extrapolation: boolean;
  enable_gap_fill: boolean;
  enable_component_merge: boolean;
  enable_weak_cluster_rescue: boolean;
  weak_cluster_tol_deg: number;
  component_merge_min_boundary_pairs: number;
  max_booster_iters: number;
  min_labeled_corners: number;
  max_components: number;
}

// ---------------------------------------------------------------------------
// Parameters: ChArUco
// ---------------------------------------------------------------------------

export type MarkerLayout = "opencv_charuco" | "bottom_left";

export interface CharucoBoardSpec {
  rows: number;
  cols: number;
  cell_size: number;
  marker_size_rel: number;
  /** Built-in dictionary name; see `list_aruco_dictionaries()`. */
  dictionary: string;
  marker_layout: MarkerLayout;
}

export interface ScanDecodeConfig {
  border_bits: number;
  inset_frac: number;
  marker_size_rel: number;
  min_border_score: number;
  dedup_by_id: boolean;
  multi_threshold: boolean;
}

export interface CharucoParams {
  px_per_square: number;
  chessboard: ChessboardParams;
  board: CharucoBoardSpec;
  scan: ScanDecodeConfig;
  max_hamming: number;
  min_marker_inliers: number;
  min_secondary_marker_inliers: number;
  grid_smoothness_threshold_rel: number;
  corner_validation_threshold_rel: number;
  use_board_level_matcher: boolean;
  bit_likelihood_slope: number;
  per_bit_floor: number;
  alignment_min_margin: number;
  cell_weight_border_threshold: number;
}

// ---------------------------------------------------------------------------
// Parameters: marker board
// ---------------------------------------------------------------------------

export type CirclePolarity = "white" | "black";

export interface MarkerCircleSpec {
  cell: GridCoords;
  polarity: CirclePolarity;
}

export interface MarkerBoardLayout {
  rows: number;
  cols: number;
  circles: MarkerCircleSpec[];
}

export interface CircleScoreParams {
  patch_size: number;
  diameter_frac: number;
  ring_thickness_frac: number;
  ring_radius_mul: number;
  min_contrast: number;
  samples: number;
  center_search_px: number;
}

export interface CircleMatchParams {
  max_candidates_per_polarity: number;
  min_offset_inliers: number;
}

export interface MarkerBoardParams {
  layout: MarkerBoardLayout;
  chessboard: ChessboardParams;
  circle_score: CircleScoreParams;
  match_params: CircleMatchParams;
}

// ---------------------------------------------------------------------------
// Parameters: PuzzleBoard
// ---------------------------------------------------------------------------

export interface PuzzleBoardSpec {
  rows: number;
  cols: number;
  cell_size: number;
  origin_row: number;
  origin_col: number;
}

export type PuzzleBoardSearchMode =
  | { kind: "full" }
  | { kind: "fixed_board" };

export type PuzzleBoardScoringMode =
  | { kind: "hard_weighted" }
  | { kind: "soft_log_likelihood" };

export interface PuzzleBoardDecodeConfig {
  min_window: number;
  min_bit_confidence: number;
  max_bit_error_rate: number;
  search_all_components: boolean;
  sample_radius_rel: number;
  search_mode: PuzzleBoardSearchMode;
  scoring_mode: PuzzleBoardScoringMode;
  bit_likelihood_slope: number;
  per_bit_floor: number;
  alignment_min_margin: number;
}

export interface PuzzleBoardParams {
  px_per_square: number;
  chessboard: ChessboardParams;
  board: PuzzleBoardSpec;
  decode: PuzzleBoardDecodeConfig;
}
