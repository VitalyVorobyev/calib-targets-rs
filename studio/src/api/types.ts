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

export interface AxisEstimateWire {
  angle: number;
  sigma: number;
}

/** Externally-tagged serde enum: unit variants are strings, others objects. */
export type CornerStageWire =
  | "Raw"
  | "Strong"
  | { NoCluster: { max_d_deg: number } }
  | { Clustered: { label: string } }
  | { AttachmentAmbiguous: { at: [number, number] } }
  | { AttachmentFailedInvariants: { at: [number, number]; reason: string } }
  | {
      Labeled: { at: [number, number]; local_h_residual_px: number | null };
    }
  | { LabeledThenBlacklisted: { at: [number, number]; reason: string } };

export type StageName =
  | "Raw"
  | "Strong"
  | "NoCluster"
  | "Clustered"
  | "AttachmentAmbiguous"
  | "AttachmentFailedInvariants"
  | "Labeled"
  | "LabeledThenBlacklisted"
  | "Other";

export function stageName(stage: CornerStageWire): StageName {
  if (typeof stage === "string") {
    return stage === "Raw" || stage === "Strong" ? stage : "Other";
  }
  const key = Object.keys(stage)[0] as StageName;
  return key ?? "Other";
}

/** Human-readable detail for a corner's stage (reason / cell / residual). */
export function stageDetail(stage: CornerStageWire): string | null {
  if (typeof stage === "string") return null;
  if ("NoCluster" in stage)
    return `max_d ${stage.NoCluster.max_d_deg.toFixed(1)}°`;
  if ("Clustered" in stage) return stage.Clustered.label;
  if ("AttachmentAmbiguous" in stage)
    return `at (${stage.AttachmentAmbiguous.at.join(", ")})`;
  if ("AttachmentFailedInvariants" in stage)
    return `at (${stage.AttachmentFailedInvariants.at.join(", ")}): ${stage.AttachmentFailedInvariants.reason}`;
  if ("Labeled" in stage) {
    const r = stage.Labeled.local_h_residual_px;
    return `at (${stage.Labeled.at.join(", ")})${r != null ? ` · res ${r.toFixed(2)} px` : ""}`;
  }
  if ("LabeledThenBlacklisted" in stage)
    return `was (${stage.LabeledThenBlacklisted.at.join(", ")}): ${stage.LabeledThenBlacklisted.reason}`;
  return null;
}

export interface CornerAugWire {
  input_index: number;
  position: [number, number];
  axes: [AxisEstimateWire, AxisEstimateWire];
  strength: number;
  contrast: number;
  fit_rms: number;
  stage: CornerStageWire;
  label: string | null;
}

export interface ExtensionTraceWire {
  h_trusted: boolean;
  h_residual_median_px: number | null;
  h_residual_max_px: number | null;
  iterations: number;
  attached: number;
  rejected_no_candidate: number;
  rejected_ambiguous: number;
  rejected_label: number;
  rejected_policy: number;
  rejected_edge: number;
}

export interface IterationTraceWire {
  iter: number;
  labelled_count: number;
  new_blacklist: number[];
  converged: boolean;
  extension?: ExtensionTraceWire | null;
  rescue?: ExtensionTraceWire | null;
  extension2?: ExtensionTraceWire | null;
  rescue2?: ExtensionTraceWire | null;
  bfs_extend?: Record<string, unknown> | null;
  geometry_check?: Record<string, unknown> | null;
  refit?: Record<string, unknown> | null;
}

export interface DebugFrameWire {
  schema: number;
  input_count: number;
  grid_directions: [number, number] | null;
  cell_size: number | null;
  seed: number[] | null;
  iterations: IterationTraceWire[];
  boosters: Record<string, unknown> | null;
  detection: { corners: unknown[] } | null;
  corners: CornerAugWire[];
}

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

export type DiagnoseAlgorithm = "topological" | "seed_and_grow";

export interface DiagnoseRequest {
  label: string;
  algorithm?: DiagnoseAlgorithm;
  params?: DetectorParamsOverride;
  orientation_method?: OrientationMethodReq;
}

export type DiagnoseResponse =
  | { kind: "topological"; diagnosis: TopologicalDiagnosisWire }
  | {
      kind: "seed_and_grow";
      frame: DebugFrameWire;
      stage_counts: Record<string, number>;
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
  params?: DetectorParamsOverride;
  engine?: EngineReq;
  orientation_method?: OrientationMethodReq;
}
