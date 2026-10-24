// The messages between the app and the worker that owns the WASM module.
//
// Every call is `{ id, method, args }` and is answered by `{ id, ok, value }` or
// `{ id, ok: false, error }`. Types only: this file is imported on both sides and must not
// pull the WASM package into the main bundle.

import type {
  CharucoDetection,
  CharucoParams,
  ChessConfig,
  ChessboardDetection,
  ChessboardParams,
  Corner,
  GeneratedTargetBundle,
  MarkerBoardDetection,
  MarkerBoardDiagnostics,
  MarkerBoardParams,
  PrintableTargetSpec,
  PuzzleBoardDetection,
  PuzzleBoardDiagnostics,
  PuzzleBoardParams,
} from "@vitavision/calib-targets";

/** What to detect. The ids are the `@vitavision/overlays` detection kinds. */
export type DetectMode = "corners" | "chessboard" | "charuco" | "markerboard" | "puzzleboard";

/** The detector parameters of every target mode, which the app edits as plain objects. */
export interface TargetParams {
  chessboard: ChessboardParams;
  charuco: CharucoParams;
  markerboard: MarkerBoardParams;
  puzzleboard: PuzzleBoardParams;
}

/** Everything the app seeds its forms with, read once from the WASM module. */
export interface Defaults {
  chess: ChessConfig;
  params: TargetParams;
  dictionaries: string[];
  /** `[circumference_squares, start_row]` of every PuzzlePole that closes seamlessly. */
  poleperiods: [number, number][];
}

/** What to run: the mode with its own parameters, the ChESS front-end, and whether to sweep. */
export type DetectInput = {
  chess: ChessConfig;
  /** Run the built-in multi-config sweep and keep the best result. */
  sweep: boolean;
} & (
  | { mode: "corners" }
  | { mode: "chessboard"; params: ChessboardParams }
  | { mode: "charuco"; params: CharucoParams }
  | { mode: "markerboard"; params: MarkerBoardParams }
  | { mode: "puzzleboard"; params: PuzzleBoardParams }
);

/** One detector run's request. The image itself was sent with `prepare`. */
export type DetectRequest = DetectInput & { imageKey: number };

/**
 * A detector's answer, tagged with its kind. `detection` is `null` for a miss; `note` carries the
 * detector's own message then (the ChArUco and PuzzleBoard detectors throw on a miss).
 * `diagnostics` is what only a `diagnose_*` call returns: the marker-board circles and the
 * PuzzleBoard edge bits. A sweep has none.
 */
export type DetectPayload =
  | { kind: "corners"; corners: Corner[] }
  | { kind: "chessboard"; detection: ChessboardDetection | null }
  | { kind: "charuco"; detection: CharucoDetection | null; note?: string }
  | {
      kind: "markerboard";
      detection: MarkerBoardDetection | null;
      diagnostics: MarkerBoardDiagnostics | null;
      note?: string;
    }
  | {
      kind: "puzzleboard";
      detection: PuzzleBoardDetection | null;
      diagnostics: PuzzleBoardDiagnostics | null;
      note?: string;
    };

export interface DetectOutcome {
  payload: DetectPayload;
  /** Wall time of the WASM call, in the worker. */
  timeMs: number;
}

/** A printable target: the renderer's bundle plus the document it was built from. */
export interface RenderedTarget {
  bundle: GeneratedTargetBundle;
  /** The `target` of the printable document the bundle was rendered from. */
  target: PrintableTargetSpec;
}

/** The parameters `render` takes, one variant per printable kind. */
export type RenderRequest =
  | { kind: "chessboard"; innerRows: number; innerCols: number; squareMm: number; dpi: number }
  | {
      kind: "charuco";
      rows: number;
      cols: number;
      squareMm: number;
      markerRel: number;
      dictionary: string;
      dpi: number;
    }
  | { kind: "marker_board"; innerRows: number; innerCols: number; squareMm: number; dpi: number }
  | { kind: "puzzleboard"; rows: number; cols: number; squareMm: number; dpi: number }
  | { kind: "puzzlepole"; circumference: number; axial: number; squareMm: number; dpi: number };

/** The worker's methods: argument tuple and result of each. */
export interface WorkerMethods {
  defaults: { args: []; result: Defaults };
  /** Convert and keep an RGBA image (transferred) as the gray image later runs read. */
  prepare: { args: [{ key: number; rgba: ArrayBuffer; width: number; height: number }]; result: null };
  detect: { args: [DetectRequest]; result: DetectOutcome };
  render: { args: [RenderRequest]; result: RenderedTarget };
}

export type WorkerMethod = keyof WorkerMethods;

export interface WorkerRequest<M extends WorkerMethod = WorkerMethod> {
  id: number;
  method: M;
  args: WorkerMethods[M]["args"];
}

export type WorkerResponse = { id: number; ok: true; value: unknown } | { id: number; ok: false; error: string };
