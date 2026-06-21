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
// Printable-target generation
// ---------------------------------------------------------------------------

/**
 * Full output of `render_*_bundle` — mirrors Rust
 * `calib_targets_print::GeneratedTargetBundle`.
 *
 * `png_bytes` is materialised as a `Uint8Array` so binary data crosses
 * the WASM boundary as a typed array (single-buffer copy) rather than as
 * a generic JS array of integers.
 *
 * The Rust struct is `#[non_exhaustive]`; consumers should default any
 * future-format `switch` and avoid exhaustive destructuring.
 */
export interface GeneratedTargetBundle {
  /** The target description serialized as JSON. */
  json_text: string;
  /** The target rendered as an SVG document. */
  svg_text: string;
  /** The target rendered as PNG image bytes. */
  png_bytes: Uint8Array;
  /**
   * The target rendered as a DXF document — chrome-on-glass
   * photolithography handoff (`AC1015` ASCII, `$INSUNITS = 4` mm,
   * Y-up cartesian, single `PATTERN` layer carrying `Fill::Black`
   * regions only).
   */
  dxf_text: string;
}

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

/** A single labelled chessboard corner (Rust `ChessboardCorner`). */
export interface ChessboardCorner {
  /** Sub-pixel image position. */
  position: Point2;
  /** Grid label `(i, j)` — always present for a chessboard corner. */
  grid: GridCoords;
  /** Index into the input `corners` slice that produced this corner. */
  input_index: number;
  /** Corner score (higher is better). */
  score: number;
}

/** Result of chessboard detection (Rust `ChessboardDetection`). */
export interface ChessboardDetectionResult {
  /** The labelled corners. */
  corners: ChessboardCorner[];
  /** Grid pitch in pixels; `null` only on results built without a seed. */
  cell_size: number | null;
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

export interface CharucoCorner {
  position: Point2;
  grid: GridCoords;
  id: number;
  target_position: Point2;
  score: number;
}

export interface CharucoDetectionResult {
  corners: CharucoCorner[];
  /** Markers consistent with `alignment` (inliers of the chosen hypothesis). */
  markers: MarkerDetection[];
  alignment: GridAlignment;
}

export interface MarkerBoardCorner {
  position: Point2;
  grid: GridCoords;
  id: number | null;
  target_position: Point2 | null;
  score: number;
}

export interface MarkerBoardDetectionResult {
  corners: MarkerBoardCorner[];
  alignment: GridAlignment | null;
}

/**
 * Compact decode quality summary (Rust `PuzzleBoardDecodeInfo`).
 *
 * Winner-vs-runner-up scoring evidence and the raw per-edge observations
 * live in the `PuzzleBoardDiagnostics` payload returned by
 * `detect_puzzleboard_with_diagnostics`.
 */
export interface PuzzleBoardDecodeInfo {
  /** Total observed edges that contributed to the decode. */
  edges_observed: number;
  /** Observed edges whose bit matched the master after alignment. */
  edges_matched: number;
  /** Mean confidence across contributing edges. */
  mean_confidence: number;
  /** Hamming error rate across all observed bits after alignment. */
  bit_error_rate: number;
  /** Absolute master-board origin of local `(0, 0)`. */
  master_origin_row: number;
  /** Absolute master-board origin of local `(0, 0)`. */
  master_origin_col: number;
}

export interface PuzzleBoardCorner {
  position: Point2;
  grid: GridCoords;
  id: number;
  target_position: Point2;
  score: number;
}

export interface PuzzleBoardDetectionResult {
  corners: PuzzleBoardCorner[];
  alignment: GridAlignment;
  decode: PuzzleBoardDecodeInfo;
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
// Parameters: chessboard detector
//
// The Rust `DetectorParams` carries a small stable core plus an opt-in,
// unstable `AdvancedTuning` sub-struct. `advanced` is `Option`-wrapped and
// serialized as a NESTED `"advanced"` object (it is NOT flattened). When
// omitted, detection runs on the default tuning. The four stable keys
// (`graph_build_algorithm`, `min_labeled_corners`, `max_components`,
// `min_corner_strength`) are covered by semver; the `AdvancedTuning` knobs
// are NOT and may change between minor versions.
// ---------------------------------------------------------------------------

/** Which grid-build algorithm to run (Rust `GraphBuildAlgorithm`). */
export type GraphBuildAlgorithm = "topological" | "seed_and_grow";

/** Global grid-direction centers for the topological pre-Delaunay gate. */
export interface AxisClusterCenters {
  /** First grid-axis direction (radians, `[0, π)`, `theta0 < theta1`). */
  theta0: number;
  /** Second grid-axis direction (radians, `[0, π)`, `theta0 < theta1`). */
  theta1: number;
}

/** Tuning knobs for the topological grid-build path (Rust `TopologicalParams`). */
export interface TopologicalParams {
  axis_align_tol_rad: number;
  max_axis_sigma_rad: number;
  opposing_edge_ratio_max: number;
  min_quads_per_component: number;
  axis_cluster_centers: AxisClusterCenters | null;
  cluster_axis_tol_rad: number;
  edge_length_min_rel: number;
  edge_length_max_rel: number;
}

/** Tuning knobs for the shared local-geometry component merger (Rust `LocalMergeParams`). */
export interface LocalMergeParams {
  position_tol_rel: number;
  cell_size_ratio_tol: number;
  min_overlap: number;
  max_components: number;
}

/**
 * Opt-in, **unstable** per-stage chessboard tuning knobs (Rust
 * `AdvancedTuning`). Nested under {@link ChessboardParams.advanced}. These
 * knobs are NOT covered by semver and may be renamed, retyped, or removed
 * between minor versions — leave them unset unless a specific input fails.
 */
export interface AdvancedTuning {
  topological: TopologicalParams;
  component_merge: LocalMergeParams;
  max_fit_rms_ratio: number;
  num_bins: number;
  max_iters_2means: number;
  cluster_tol_deg: number;
  cluster_sigma_k: number;
  peak_min_separation_deg: number;
  min_peak_weight_fraction: number;
  attach_search_rel: number;
  attach_axis_tol_deg: number;
  attach_ambiguity_factor: number;
  step_tol: number;
  edge_axis_tol_deg: number;
  line_min_members: number;
  validate_step_aware: boolean;
  geometry_check_line_tol_rel: number;
  geometry_check_local_h_tol_rel: number;
  enable_final_edge_shape_check: boolean;
  enable_weak_cluster_rescue: boolean;
  weak_cluster_tol_deg: number;
  max_booster_iters: number;
}

/**
 * Chessboard detector parameters — the serialized shape of the Rust
 * `DetectorParams`. The four stable keys below are the semver-covered core;
 * the optional `advanced` block holds the unstable per-stage tuning knobs and
 * is omitted from the wire format unless set.
 */
export interface ChessboardParams {
  // --- stable core ---
  graph_build_algorithm: GraphBuildAlgorithm;
  min_labeled_corners: number;
  max_components: number;
  min_corner_strength: number;
  // --- opt-in, unstable tuning (omitted when unset) ---
  advanced?: AdvancedTuning;
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

// ---------------------------------------------------------------------------
// Diagnostics channel
//
// Returned by the `detect_*_with_diagnostics` functions as a
// `{ result, diagnostics }` object. The `diagnostics` payloads mirror the
// Rust diagnostics structs' `serde_json` shape and carry a LOOSER stability
// promise than the result types above — fields may be added or restructured
// between minor releases.
// ---------------------------------------------------------------------------

/** `{ result, diagnostics }` wrapper returned by `detect_*_with_diagnostics`. */
export interface DetectionWithDiagnostics<TResult, TDiagnostics> {
  /** The typed detection result, or `null` when detection failed. */
  result: TResult | null;
  /**
   * The diagnostics payload. `null` only for marker boards on a failed
   * detection (that channel yields evidence only on success).
   */
  diagnostics: TDiagnostics | null;
}

// --- ChArUco diagnostics (Rust `CharucoDetectDiagnostics`) -----------------

/** Best marker match found for a single cell (Rust `CellBestMatch`). */
export interface CellBestMatch {
  marker_id: number;
  rotation: number;
  score: number;
}

/** One scored board-placement hypothesis (Rust `DiagHypothesis`). */
export interface DiagHypothesis {
  rotation: number;
  /** `[Δcol, Δrow]` translation on the grid. */
  translation: [number, number];
  score: number;
  contributing_cells: number;
}

/** Reason the board matcher rejected a frame (Rust `RejectReason`). */
export type RejectReason =
  | { kind: "no_cells" }
  | { kind: "empty_board" }
  | { kind: "translation_window_empty" }
  | { kind: "margin_below_gate"; margin: number; required: number }
  | { kind: "no_emitted_markers" };

/** Per-cell diagnostic record from the board matcher (Rust `CellDiag`). */
export interface CellDiag {
  gc: GridCoords;
  /** Four corners `[TL, TR, BR, BL]` in image pixels. */
  corners_img: [Point2, Point2, Point2, Point2];
  sampled: boolean;
  otsu: number;
  border_black: number;
  weight: number;
  mapped_bc?: [number, number];
  expected_id?: number;
  expected_score: number;
  best?: CellBestMatch;
  expected_bit_ll?: number[];
  interior_means: number[];
}

/** Board-level matcher diagnostics (Rust `BoardMatchDiagnostics`). */
export interface BoardMatchDiagnostics {
  cells: CellDiag[];
  chosen?: DiagHypothesis;
  runner_up?: DiagHypothesis;
  margin: number;
  total_hypotheses: number;
  rejection?: RejectReason;
  board_cols: number;
  board_rows: number;
  bits_per_side: number;
}

/** Which marker-matching branch produced a component (Rust `MatcherDiagKind`). */
export type MatcherDiagKind = "legacy" | "board_level";

/** Final outcome of detecting one chessboard component (Rust `ComponentOutcome`). */
export type ComponentOutcome =
  | {
      status: "ok";
      markers: number;
      charuco_corners: number;
      raw_marker_count: number;
      raw_marker_wrong_id_count: number;
    }
  | { status: "failed"; reason: string };

/** Per-component ChArUco diagnostics (Rust `ComponentDiagnostics`). */
export interface ComponentDiagnostics {
  index: number;
  chess_corner_count: number;
  candidate_cell_count: number;
  matcher: MatcherDiagKind;
  board?: BoardMatchDiagnostics;
  outcome: ComponentOutcome;
}

/** ChArUco detector diagnostics payload (Rust `CharucoDetectDiagnostics`). */
export interface CharucoDetectDiagnostics {
  components: ComponentDiagnostics[];
  raw_marker_count: number;
  raw_marker_wrong_id_count: number;
}

// --- Marker-board diagnostics (Rust `MarkerBoardDiagnostics`) --------------

/** Integer cell coordinate, top-left corner indices (Rust `CellCoords`). */
export interface CellCoords {
  i: number;
  j: number;
}

/** Integer detected-to-board cell translation (Rust `CellOffset`). */
export interface CellOffset {
  di: number;
  dj: number;
}

/** A scored circle hypothesis (Rust `CircleCandidate`). */
export interface CircleCandidate {
  /** Circle center in image pixel coordinates. */
  center_img: Point2;
  cell: CellCoords;
  polarity: CirclePolarity;
  score: number;
  contrast: number;
}

/** An expected-to-detected circle pairing (Rust `CircleMatch`). */
export interface CircleMatch {
  expected: MarkerCircleSpec;
  matched_index: number | null;
  distance_cells: number | null;
  offset_cells: CellOffset | null;
}

/** Marker-board detector diagnostics payload (Rust `MarkerBoardDiagnostics`). */
export interface MarkerBoardDiagnostics {
  /** Per-corner provenance back into the input ChESS-corner slice. */
  inliers: number[];
  /** Empty on the corners-only detection path. */
  circle_candidates: CircleCandidate[];
  /** Empty on the corners-only detection path. */
  circle_matches: CircleMatch[];
  alignment_inliers: number;
}

// --- PuzzleBoard diagnostics (Rust `PuzzleBoardDiagnostics`) ---------------

/** Edge orientation in the local board frame (Rust `EdgeOrientation`). */
export type EdgeOrientation = "horizontal" | "vertical";

/** A raw per-edge bit observation sampled before alignment (Rust `PuzzleBoardObservedEdge`). */
export interface PuzzleBoardObservedEdge {
  /** Board row coordinate of the edge. */
  row: number;
  /** Board column coordinate of the edge. */
  col: number;
  orientation: EdgeOrientation;
  /** Observed bit (`0` / `1`). */
  bit: number;
  /** Per-bit confidence in `[0, 1]`. */
  confidence: number;
}

/** Winner-vs-runner-up decode scoring evidence (Rust `PuzzleBoardDecodeDiagnostics`). */
export interface PuzzleBoardDecodeDiagnostics {
  score_best?: number;
  score_runner_up?: number;
  score_margin?: number;
  runner_up_origin_row?: number;
  runner_up_origin_col?: number;
  runner_up_transform?: GridTransform;
  scoring_mode?: PuzzleBoardScoringMode;
}

/** PuzzleBoard detector diagnostics payload (Rust `PuzzleBoardDiagnostics`). */
export interface PuzzleBoardDiagnostics {
  /** Unfiltered raw per-edge bit observations sampled before alignment. */
  observed_edges: PuzzleBoardObservedEdge[];
  decode: PuzzleBoardDecodeDiagnostics;
}
