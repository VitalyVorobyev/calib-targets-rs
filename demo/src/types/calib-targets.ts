// Re-export of every object-shape type that crosses the WASM boundary.
//
// The shapes themselves live alongside the function signatures in
// `@vitavision/calib-targets`; this module exists only to keep imports
// short for the demo and to declare the demo-internal `DetectionMode`
// enum.

export type {
  AxisEstimate,
  CharucoCorner,
  CharucoBoardSpec,
  CharucoParams,
  ChessConfig,
  ChessRing,
  ChessboardDetection,
  ChessboardParams,
  CharucoDetection,
  CircleCandidate,
  CircleMatch,
  CircleMatchParams,
  CirclePolarity,
  CircleScoreParams,
  Corner,
  Coord,
  GridAlignment,
  GridTransform,
  LabeledCorner,
  MarkerBoardCorner,
  MarkerBoardDetection,
  MarkerBoardSpec,
  MarkerBoardParams,
  MarkerCircleSpec,
  MarkerDetection,
  MarkerLayout,
  Point2,
  PuzzleBoardCorner,
  PuzzleBoardDecodeConfig,
  PuzzleBoardDecodeInfo,
  PuzzleBoardDetection,
  PuzzleBoardObservedEdge,
  PuzzleBoardParams,
  PuzzleBoardScoringMode,
  PuzzleBoardSearchMode,
  PuzzleBoardSpec,
  RefinerConfig,
  ScanDecodeConfig,
  TargetDetection,
  TargetKind,
  UpscaleConfig,
} from "@vitavision/calib-targets";

/** Detection mode selected from the demo UI. */
export type DetectionMode =
  | "corners"
  | "chessboard"
  | "charuco"
  | "marker_board"
  | "puzzleboard";

/** Synthetic-target kinds the demo can render in the browser. */
export type SyntheticTargetKind =
  | "chessboard"
  | "charuco"
  | "marker_board"
  | "puzzleboard";
