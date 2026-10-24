// The WASM module's functions with the result types the app relies on. wasm-bindgen types every
// config and result as `any`; the shapes are declared in `typescript-extras.d.ts` (appended to
// the package's declarations), and this is the one place they are asserted.

import init, * as raw from "@vitavision/calib-targets";
import type {
  CharucoDetection,
  CharucoParams,
  ChessConfig,
  ChessboardDetection,
  ChessboardParams,
  Corner,
  DetectionWithDiagnostics,
  GeneratedTargetBundle,
  MarkerBoardDetection,
  MarkerBoardDiagnostics,
  MarkerBoardParams,
  MarkerBoardSpec,
  PuzzleBoardDetection,
  PuzzleBoardDiagnostics,
  PuzzleBoardParams,
} from "@vitavision/calib-targets";

type Gray = Uint8Array;

export const load = (): Promise<unknown> => init();

export const rgbaToGray = (rgba: Uint8Array, width: number, height: number): Gray =>
  raw.rgba_to_gray(rgba, width, height);

// --- defaults and presets -------------------------------------------------------------------

export const defaultChessConfig = (): ChessConfig => raw.default_chess_config() as ChessConfig;
export const defaultChessboardParams = (): ChessboardParams => raw.default_chessboard_params() as ChessboardParams;
export const defaultMarkerBoardParams = (): MarkerBoardParams => raw.default_marker_board_params() as MarkerBoardParams;
export const defaultPuzzleBoardParams = (rows: number, cols: number): PuzzleBoardParams =>
  raw.default_puzzleboard_params(rows, cols) as PuzzleBoardParams;
export const defaultCharucoParams = (
  rows: number,
  cols: number,
  markerRel: number,
  dictionary: string,
): CharucoParams => raw.default_charuco_params(rows, cols, markerRel, dictionary) as CharucoParams;
export const listArucoDictionaries = (): string[] => raw.list_aruco_dictionaries() as string[];
export const puzzlepolePeriods = (): [number, number][] => raw.puzzlepole_periods() as [number, number][];

export const chessboardSweep = (): ChessboardParams[] => raw.chessboard_sweep_default() as ChessboardParams[];
export const charucoSweep = (rows: number, cols: number, markerRel: number, dictionary: string): CharucoParams[] =>
  raw.charuco_sweep_for_board(rows, cols, markerRel, dictionary) as CharucoParams[];
export const markerBoardSweep = (board: MarkerBoardSpec): MarkerBoardParams[] =>
  raw.marker_board_sweep_for_board(board) as MarkerBoardParams[];
export const puzzleboardSweep = (rows: number, cols: number): PuzzleBoardParams[] =>
  raw.puzzleboard_sweep_for_board(rows, cols) as PuzzleBoardParams[];

// --- detection ------------------------------------------------------------------------------
// A detector that finds nothing returns `undefined` or `null`, depending on the entry point.

export const detectCorners = (w: number, h: number, gray: Gray, chess: ChessConfig): Corner[] =>
  (raw.detect_corners(w, h, gray, chess) as Corner[] | null | undefined) ?? [];

export const detectChessboard = (
  w: number,
  h: number,
  gray: Gray,
  chess: ChessConfig,
  params: ChessboardParams,
): ChessboardDetection | null =>
  (raw.detect_chessboard(w, h, gray, chess, params) as ChessboardDetection | null | undefined) ?? null;
export const detectChessboardBest = (
  w: number,
  h: number,
  gray: Gray,
  chess: ChessConfig,
  configs: ChessboardParams[],
): ChessboardDetection | null =>
  (raw.detect_chessboard_best(w, h, gray, chess, configs) as ChessboardDetection | null | undefined) ?? null;

/** Throws on a miss. */
export const detectCharuco = (
  w: number,
  h: number,
  gray: Gray,
  chess: ChessConfig,
  params: CharucoParams,
): CharucoDetection => raw.detect_charuco(w, h, gray, chess, params) as CharucoDetection;
/** Throws when every config misses. */
export const detectCharucoBest = (w: number, h: number, gray: Gray, configs: CharucoParams[]): CharucoDetection =>
  raw.detect_charuco_best(w, h, gray, configs) as CharucoDetection;

export const detectMarkerBoardBest = (
  w: number,
  h: number,
  gray: Gray,
  configs: MarkerBoardParams[],
): MarkerBoardDetection | null =>
  (raw.detect_marker_board_best(w, h, gray, configs) as MarkerBoardDetection | null | undefined) ?? null;
export const diagnoseMarkerBoard = (
  w: number,
  h: number,
  gray: Gray,
  chess: ChessConfig,
  params: MarkerBoardParams,
): DetectionWithDiagnostics<MarkerBoardDetection, MarkerBoardDiagnostics> =>
  raw.diagnose_marker_board(w, h, gray, chess, params) as DetectionWithDiagnostics<
    MarkerBoardDetection,
    MarkerBoardDiagnostics
  >;

/** Throws when every config misses. */
export const detectPuzzleBoardBest = (
  w: number,
  h: number,
  gray: Gray,
  configs: PuzzleBoardParams[],
): PuzzleBoardDetection => raw.detect_puzzleboard_best(w, h, gray, configs) as PuzzleBoardDetection;
export const diagnosePuzzleBoard = (
  w: number,
  h: number,
  gray: Gray,
  chess: ChessConfig,
  params: PuzzleBoardParams,
): DetectionWithDiagnostics<PuzzleBoardDetection, PuzzleBoardDiagnostics> =>
  raw.diagnose_puzzleboard(w, h, gray, chess, params) as DetectionWithDiagnostics<
    PuzzleBoardDetection,
    PuzzleBoardDiagnostics
  >;

// --- printable targets ----------------------------------------------------------------------

export const renderChessboard = (
  innerRows: number,
  innerCols: number,
  squareMm: number,
  dpi: number,
): GeneratedTargetBundle => raw.render_chessboard_bundle(innerRows, innerCols, squareMm, dpi) as GeneratedTargetBundle;
export const renderCharuco = (
  rows: number,
  cols: number,
  squareMm: number,
  markerRel: number,
  dictionary: string,
  dpi: number,
): GeneratedTargetBundle =>
  raw.render_charuco_bundle(rows, cols, squareMm, markerRel, dictionary, dpi) as GeneratedTargetBundle;
export const renderMarkerBoard = (
  innerRows: number,
  innerCols: number,
  squareMm: number,
  dpi: number,
): GeneratedTargetBundle =>
  raw.render_marker_board_bundle(innerRows, innerCols, squareMm, dpi) as GeneratedTargetBundle;
export const renderPuzzleBoard = (rows: number, cols: number, squareMm: number, dpi: number): GeneratedTargetBundle =>
  raw.render_puzzleboard_bundle(rows, cols, squareMm, dpi) as GeneratedTargetBundle;
export const renderPuzzlePole = (
  circumference: number,
  axial: number,
  squareMm: number,
  dpi: number,
): GeneratedTargetBundle => raw.render_puzzlepole_bundle(circumference, axial, squareMm, dpi) as GeneratedTargetBundle;
