// TypeScript mirrors of the studio server's wire types.
// Rust sources of truth:
//   crates/calib-targets-studio/src/routes/*.rs
//   crates/calib-targets-bench/src/{baseline,diff,report,diagnose}.rs
//   crates/calib-targets-chessboard/src/pipeline/types.rs (DebugFrame)

// --- dataset ---------------------------------------------------------------

export interface SnapInfo {
  label: string;
  index: number;
  width: number | null;
  height: number | null;
}

export interface StitchedInfo {
  count: number;
  snap_width: number;
  snap_height: number;
}

export interface ImageInfo {
  path: string;
  kind: "public" | "private";
  note: string;
  upscale: number;
  stitched: StitchedInfo | null;
  available: boolean;
  snaps: SnapInfo[];
}

export interface DatasetResponse {
  images: ImageInfo[];
}

// --- detection -------------------------------------------------------------

export interface BaselineCorner {
  i: number;
  j: number;
  x: number;
  y: number;
  id?: number;
  score: number;
}

export interface BaselineImage {
  labelled_count: number;
  cell_size_px: number;
  corners: BaselineCorner[];
}

export interface WrongPosition {
  i: number;
  j: number;
  baseline: [number, number];
  run: [number, number];
  drift_px: number;
}

export interface DuplicatePosition {
  position: [number, number];
  labels: [number, number][];
}

export interface BaselineDiff {
  missing_labels: [number, number][];
  extra_labels: [number, number][];
  wrong_position: WrongPosition[];
  wrong_id: [number, number][];
  shift: [number, number] | null;
  inconsistent_shift: boolean;
  duplicate_run_positions: DuplicatePosition[];
}

export type EngineReq = "pipeline" | "grid";
export type OrientationMethodReq = "ring_fit" | "disk_fit";
export type GraphBuildAlgorithm = "topological" | "seed_and_grow";
export type OrientationSource = "chess_axes" | "neighbour_edges";

/** Partial DetectorParams override (top-level-key merge over defaults). */
export interface DetectorParamsOverride {
  graph_build_algorithm?: GraphBuildAlgorithm;
  orientation_source?: OrientationSource;
  min_labeled_corners?: number;
  max_components?: number;
  min_corner_strength?: number;
  advanced?: Record<string, unknown>;
}

export interface DetectRequest {
  label: string;
  engine?: EngineReq;
  params?: DetectorParamsOverride;
  orientation_method?: OrientationMethodReq;
  compare_baseline?: boolean;
}

export interface BaselineBlock {
  exists: boolean;
  diff?: BaselineDiff;
}

export interface DetectResponse {
  elapsed_ms: number;
  image: { width: number; height: number };
  detection: BaselineImage | null;
  baseline?: BaselineBlock;
}

export interface ApiErrorBody {
  error: string;
}
