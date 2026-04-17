// TypeScript interfaces mirroring the Rust serde structs.
// These provide type safety on top of the raw JsValue-based WASM API.

// ---------------------------------------------------------------------------
// Input config types
// ---------------------------------------------------------------------------

export interface ChessConfig {
  detector_mode: "canonical" | "broad";
  descriptor_mode: "follow_detector" | "canonical" | "broad";
  threshold_mode: "relative" | "absolute";
  threshold_value: number;
  nms_radius: number;
  min_cluster_size: number;
  refiner: RefinerConfig;
  pyramid_levels: number;
  pyramid_min_size: number;
  refinement_radius: number;
  merge_radius: number;
}

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

export interface GridGraphParams {
  min_spacing_pix: number;
  max_spacing_pix: number;
  k_neighbors: number;
  orientation_tolerance_deg: number;
}

export interface OrientationClusteringParams {
  num_bins: number;
  max_iters: number;
  peak_min_separation_deg: number;
  outlier_threshold_deg: number;
  min_peak_weight_fraction: number;
  use_weights: boolean;
}

export interface ChessboardParams {
  min_corner_strength: number;
  min_corners: number;
  expected_rows?: number | null;
  expected_cols?: number | null;
  completeness_threshold: number;
  use_orientation_clustering: boolean;
  orientation_clustering_params: OrientationClusteringParams;
  graph: GridGraphParams;
}

export interface CharucoBoardSpec {
  rows: number;
  cols: number;
  cell_size?: number | null;
  marker_size_rel: number;
  dictionary: string;
  marker_layout: "opencv_charuco" | "bottom_left";
}

export interface ScanDecodeConfig {
  marker_size_rel: number;
  inset_frac: number;
  border_bits: number;
  min_border_score: number;
  dedup_by_id: boolean;
}

export interface CharucoDetectorParams {
  charuco: CharucoBoardSpec;
  px_per_square: number;
  chessboard: ChessboardParams;
  scan: ScanDecodeConfig;
  max_hamming: number;
  min_marker_inliers: number;
  corner_validation_threshold_rel: number;
}

export interface MarkerCircleSpec {
  i: number;
  j: number;
  polarity: "white" | "black";
}

export interface MarkerBoardLayout {
  rows: number;
  cols: number;
  cell_size: number;
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

export interface PuzzleBoardDecodeConfig {
  min_window: number;
  min_bit_confidence: number;
  max_bit_error_rate: number;
  search_all_components: boolean;
  sample_radius_rel: number;
  search_mode: PuzzleBoardSearchMode;
}

export interface PuzzleBoardParams {
  px_per_square: number;
  chessboard: ChessboardParams;
  board: PuzzleBoardSpec;
  decode: PuzzleBoardDecodeConfig;
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/** nalgebra::Point2<f32> serializes as [x, y] via serde */
export type Point2 = [number, number];

export interface Corner {
  position: Point2;
  orientation: number;
  orientation_cluster: number | null;
  strength: number;
}

export interface GridCoords {
  i: number;
  j: number;
}

export interface LabeledCorner {
  position: Point2;
  grid: GridCoords | null;
  id: number | null;
  target_position: Point2 | null;
  score: number;
}

export interface TargetDetection {
  kind: "chessboard" | "charuco" | "checkerboard_marker" | "puzzle_board";
  corners: LabeledCorner[];
}

export interface ChessboardDetectionResult {
  detection: TargetDetection;
  inliers: number[];
  orientations: [number, number] | null;
}

export interface GridTransform {
  a: number;
  b: number;
  c: number;
  d: number;
}

export interface GridAlignment {
  transform: GridTransform;
  translation: [number, number];
}

export interface MarkerDetection {
  id: number;
  gc: { i: number; j: number };
  rotation: number;
  hamming: number;
  score: number;
  border_score: number;
}

export interface CharucoDetectionResult {
  detection: TargetDetection;
  markers: MarkerDetection[];
  alignment: GridAlignment;
}

export interface MarkerBoardDetectionResult {
  detection: TargetDetection;
  inliers: number[];
  alignment: GridAlignment | null;
  alignment_inliers: number;
}

export interface ObservedEdge {
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
}

export interface PuzzleBoardDetectionResult {
  detection: TargetDetection;
  alignment: GridAlignment;
  decode: PuzzleBoardDecodeInfo;
  observed_edges: ObservedEdge[];
}

// ---------------------------------------------------------------------------
// Detection mode enum
// ---------------------------------------------------------------------------

export type DetectionMode =
  | "corners"
  | "chessboard"
  | "charuco"
  | "marker_board"
  | "puzzleboard";
