// Typed wrapper over the raw @vitavision/calib-targets WASM API.

import init, {
  detect_corners as _detect_corners,
  detect_chessboard as _detect_chessboard,
  detect_chessboard_best as _detect_chessboard_best,
  detect_charuco as _detect_charuco,
  detect_charuco_best as _detect_charuco_best,
  detect_marker_board as _detect_marker_board,
  detect_marker_board_best as _detect_marker_board_best,
  detect_puzzleboard as _detect_puzzleboard,
  detect_puzzleboard_best as _detect_puzzleboard_best,
  rgba_to_gray as _rgba_to_gray,
  render_chessboard_png as _render_chessboard_png,
  render_charuco_png as _render_charuco_png,
  render_marker_board_png as _render_marker_board_png,
  render_puzzleboard_png as _render_puzzleboard_png,
  default_chess_config as _default_chess_config,
  default_chessboard_params as _default_chessboard_params,
  default_charuco_params as _default_charuco_params,
  default_marker_board_params as _default_marker_board_params,
  default_puzzleboard_params as _default_puzzleboard_params,
  chessboard_sweep_default as _chessboard_sweep_default,
  charuco_sweep_for_board as _charuco_sweep_for_board,
  puzzleboard_sweep_for_board as _puzzleboard_sweep_for_board,
  list_aruco_dictionaries as _list_aruco_dictionaries,
} from "@vitavision/calib-targets";

import type {
  ChessConfig,
  ChessboardParams,
  CharucoParams,
  MarkerBoardParams,
  PuzzleBoardParams,
  Corner,
  ChessboardDetectionResult,
  CharucoDetectionResult,
  MarkerBoardDetectionResult,
  PuzzleBoardDetectionResult,
} from "@vitavision/calib-targets";

let initialized = false;

export async function initialize(): Promise<void> {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

export function isReady(): boolean {
  return initialized;
}

// ---------------------------------------------------------------------------
// Image utilities
// ---------------------------------------------------------------------------

export function rgbaToGray(
  rgba: Uint8Array,
  width: number,
  height: number,
): Uint8Array {
  return _rgba_to_gray(rgba, width, height);
}

// ---------------------------------------------------------------------------
// Synthetic target generation
// ---------------------------------------------------------------------------

export function renderChessboardPng(
  innerRows: number,
  innerCols: number,
  squareSizeMm: number,
  dpi: number,
): Uint8Array {
  return _render_chessboard_png(innerRows, innerCols, squareSizeMm, dpi);
}

export function renderCharucoPng(
  rows: number,
  cols: number,
  squareSizeMm: number,
  markerSizeRel: number,
  dictionaryName: string,
  dpi: number,
): Uint8Array {
  return _render_charuco_png(
    rows,
    cols,
    squareSizeMm,
    markerSizeRel,
    dictionaryName,
    dpi,
  );
}

export function renderMarkerBoardPng(
  innerRows: number,
  innerCols: number,
  squareSizeMm: number,
  dpi: number,
): Uint8Array {
  return _render_marker_board_png(innerRows, innerCols, squareSizeMm, dpi);
}

/** Synthesise a PuzzleBoard target PNG entirely in WASM. */
export function renderPuzzleBoardPng(
  rows: number,
  cols: number,
  squareSizeMm: number,
  dpi: number,
): Uint8Array {
  return _render_puzzleboard_png(rows, cols, squareSizeMm, dpi);
}

// ---------------------------------------------------------------------------
// Default config / sweep getters
// ---------------------------------------------------------------------------

export function defaultChessConfig(): ChessConfig {
  return _default_chess_config() as ChessConfig;
}

export function defaultChessboardParams(): ChessboardParams {
  return _default_chessboard_params() as ChessboardParams;
}

export function defaultCharucoParams(
  rows: number,
  cols: number,
  markerSizeRel: number,
  dictionaryName: string,
): CharucoParams {
  return _default_charuco_params(
    rows,
    cols,
    markerSizeRel,
    dictionaryName,
  ) as CharucoParams;
}

export function defaultMarkerBoardParams(): MarkerBoardParams {
  return _default_marker_board_params() as MarkerBoardParams;
}

export function defaultPuzzleBoardParams(
  rows: number,
  cols: number,
): PuzzleBoardParams {
  return _default_puzzleboard_params(rows, cols) as PuzzleBoardParams;
}

export function chessboardSweepDefault(): ChessboardParams[] {
  return _chessboard_sweep_default() as ChessboardParams[];
}

export function charucoSweepForBoard(
  rows: number,
  cols: number,
  markerSizeRel: number,
  dictionaryName: string,
): CharucoParams[] {
  return _charuco_sweep_for_board(
    rows,
    cols,
    markerSizeRel,
    dictionaryName,
  ) as CharucoParams[];
}

export function puzzleboardSweepForBoard(
  rows: number,
  cols: number,
): PuzzleBoardParams[] {
  return _puzzleboard_sweep_for_board(rows, cols) as PuzzleBoardParams[];
}

export function listArucoDictionaries(): string[] {
  return _list_aruco_dictionaries() as string[];
}

// ---------------------------------------------------------------------------
// Detection (single config)
// ---------------------------------------------------------------------------

export function detectCorners(
  gray: Uint8Array,
  width: number,
  height: number,
  chessCfg: ChessConfig,
): Corner[] {
  return _detect_corners(width, height, gray, chessCfg) as Corner[];
}

export function detectChessboard(
  gray: Uint8Array,
  width: number,
  height: number,
  chessCfg: ChessConfig,
  params: ChessboardParams,
): ChessboardDetectionResult | null {
  return _detect_chessboard(
    width,
    height,
    gray,
    chessCfg,
    params,
  ) as ChessboardDetectionResult | null;
}

export function detectCharuco(
  gray: Uint8Array,
  width: number,
  height: number,
  chessCfg: ChessConfig,
  params: CharucoParams,
): CharucoDetectionResult {
  return _detect_charuco(
    width,
    height,
    gray,
    chessCfg,
    params,
  ) as CharucoDetectionResult;
}

export function detectMarkerBoard(
  gray: Uint8Array,
  width: number,
  height: number,
  chessCfg: ChessConfig,
  params: MarkerBoardParams,
): MarkerBoardDetectionResult | null {
  return _detect_marker_board(
    width,
    height,
    gray,
    chessCfg,
    params,
  ) as MarkerBoardDetectionResult | null;
}

export function detectPuzzleBoard(
  gray: Uint8Array,
  width: number,
  height: number,
  chessCfg: ChessConfig,
  params: PuzzleBoardParams,
): PuzzleBoardDetectionResult {
  return _detect_puzzleboard(
    width,
    height,
    gray,
    chessCfg,
    params,
  ) as PuzzleBoardDetectionResult;
}

// ---------------------------------------------------------------------------
// Detection (best-of sweeps)
// ---------------------------------------------------------------------------

export function detectChessboardBest(
  gray: Uint8Array,
  width: number,
  height: number,
  configs: ChessboardParams[],
): ChessboardDetectionResult | null {
  return _detect_chessboard_best(
    width,
    height,
    gray,
    configs,
  ) as ChessboardDetectionResult | null;
}

export function detectCharucoBest(
  gray: Uint8Array,
  width: number,
  height: number,
  configs: CharucoParams[],
): CharucoDetectionResult {
  return _detect_charuco_best(
    width,
    height,
    gray,
    configs,
  ) as CharucoDetectionResult;
}

export function detectMarkerBoardBest(
  gray: Uint8Array,
  width: number,
  height: number,
  configs: MarkerBoardParams[],
): MarkerBoardDetectionResult | null {
  return _detect_marker_board_best(
    width,
    height,
    gray,
    configs,
  ) as MarkerBoardDetectionResult | null;
}

export function detectPuzzleBoardBest(
  gray: Uint8Array,
  width: number,
  height: number,
  configs: PuzzleBoardParams[],
): PuzzleBoardDetectionResult {
  return _detect_puzzleboard_best(
    width,
    height,
    gray,
    configs,
  ) as PuzzleBoardDetectionResult;
}
