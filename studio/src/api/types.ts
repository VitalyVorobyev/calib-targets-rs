// TypeScript mirrors of the studio server's wire types.
// Rust sources of truth:
//   crates/calib-targets-studio/src/routes/*.rs
//   crates/calib-targets-bench/src/{baseline,diff,report,diagnose}.rs

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
  /** Dataset group name (directory-derived or explicit). */
  dataset: string;
  /** Baseline-free low-recall floor; null ⇒ flag only no-detection. */
  min_labelled: number | null;
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

/**
 * Partial DetectorParams override (top-level-key merge over defaults).
 *
 * Mirrors `calib_targets_chessboard::DetectorParams`, which is
 * `#[serde(deny_unknown_fields)]`: only these keys are accepted, and any
 * other (unknown or removed) key is rejected by the server's merge.
 * `advanced`, when present, must be the *complete*
 * `AdvancedTuning` block (every field required) — the UI seeds it from the
 * fully-materialised `/api/configs/_defaults` response.
 */
export interface DetectorParamsOverride {
  min_labeled_corners?: number;
  max_components?: number;
  min_corner_strength?: number;
  advanced?: Record<string, unknown>;
}

/** A built-in detector preset served by `GET /api/presets`. */
export interface Preset {
  name: string;
  description: string;
  params: DetectorParamsOverride;
}

/** One row of the saved-config listing (`GET /api/configs`). */
export interface ConfigSummary {
  name: string;
  modified_at: number;
  has_advanced: boolean;
}

// --- advanced-param schema (`GET /api/params/schema`) ----------------------
// Rust source of truth: crates/calib-targets-studio/src/routes/params_schema.rs

export type ParamKind = "bool" | "int" | "float";

/** One section of the advanced-param form, in display order. */
export interface ParamGroup {
  id: string;
  title: string;
}

/** Human metadata for one editable advanced knob, keyed by JSON pointer. */
export interface ParamField {
  /** RFC-6901 pointer into the materialised params (e.g. `/advanced/cluster_tol_deg`). */
  pointer: string;
  group: string;
  label: string;
  help: string;
  kind: ParamKind;
  /** Pointer of the bool flag that gates this knob (greyed when false). */
  gated_by?: string;
}

/** The advanced-param UI metadata catalogue. */
export interface ParamSchema {
  groups: ParamGroup[];
  fields: ParamField[];
}

export type DetectorReq = "chessboard" | "charuco" | "puzzleboard";

export interface BoardReq {
  rows: number;
  cols: number;
  cell_size?: number;
  marker_size_rel?: number;
  dictionary?: string;
  origin_row?: number;
  origin_col?: number;
}

export interface DetectRequest {
  label: string;
  detector?: DetectorReq;
  board?: BoardReq;
  engine?: EngineReq;
  params?: DetectorParamsOverride;
  orientation_method?: OrientationMethodReq;
  compare_baseline?: boolean;
  sweep?: boolean;
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
  /** Family extras: `{markers}` for charuco, `{decode}` for puzzleboard. */
  info?: {
    markers?: number;
    decode?: {
      edges_observed: number;
      edges_matched: number;
      mean_confidence: number;
      bit_error_rate: number;
      master_origin_row: number;
      master_origin_col: number;
    };
  };
}

export interface ApiErrorBody {
  error: string;
}

// --- diagnose ----------------------------------------------------------------

export interface TolSummaryWire {
  axis_align_tol_rad: number;
  max_axis_sigma_rad: number;
  cluster_axis_tol_rad: number;
  edge_length_max_rel: number;
}

export interface TopologicalDiagnosisWire {
  input_count: number;
  effective_tols: TolSummaryWire;
  prefilter: {
    survives_strength: number;
    survives_fit: number;
    survives_axis: number;
  };
  components: { labelled: number; bbox: [number, number, number, number] }[];
  labelled_indices: number[];
  corners: {
    x: number;
    y: number;
    sigma0: number;
    sigma1: number;
    labelled: boolean;
  }[];
}

export type DiagnoseAlgorithm = "topological";

export interface DiagnoseRequest {
  label: string;
  algorithm?: DiagnoseAlgorithm;
  params?: DetectorParamsOverride;
  orientation_method?: OrientationMethodReq;
}

export type DiagnoseResponse = {
  kind: "topological";
  diagnosis: TopologicalDiagnosisWire;
};

// --- dataset runs ------------------------------------------------------------

export type RunStatus = "running" | "done" | "failed";
export type DatasetReq = "public" | "private" | "all";

export interface PerImageReport {
  image: string;
  passed: boolean;
  has_baseline: boolean;
  elapsed_ms: number;
  labelled_count: number;
  diff_vs_baseline: BaselineDiff;
}

export interface RunSummary {
  images_total: number;
  images_passed: number;
  images_failed: number;
  p50_ms: number;
  p95_ms: number;
  max_ms: number;
}

export interface RunRecord {
  id: string;
  status: RunStatus;
  started_at: number;
  config_id: string;
  dataset: string;
  progress: { done: number; total: number; current: string | null };
  per_image: PerImageReport[];
  summary: RunSummary | null;
  error: string | null;
}

export interface RunRequest {
  dataset?: DatasetReq;
  /** Scope the run to a single dataset group (overrides the kind filter). */
  group?: string;
  params?: DetectorParamsOverride;
  engine?: EngineReq;
  orientation_method?: OrientationMethodReq;
}
