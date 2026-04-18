// Typed wrapper over the raw calib-targets-wasm WASM API.

import init, {
  detect_corners as _detect_corners,
  detect_chessboard as _detect_chessboard,
  detect_charuco as _detect_charuco,
  detect_marker_board as _detect_marker_board,
  detect_puzzleboard as _detect_puzzleboard,
  rgba_to_gray as _rgba_to_gray,
  render_puzzleboard_png as _render_puzzleboard_png,
  default_chess_config as _default_chess_config,
  default_chessboard_params as _default_chessboard_params,
  default_puzzleboard_params as _default_puzzleboard_params,
} from "calib-targets-wasm";

import type {
  ChessConfig,
  ChessboardParams,
  CharucoDetectorParams,
  MarkerBoardParams,
  PuzzleBoardParams,
  Corner,
  ChessboardDetectionResult,
  CharucoDetectionResult,
  MarkerBoardDetectionResult,
  PuzzleBoardDetectionResult,
} from "../types/calib-targets";

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

export function rgbaToGray(
  rgba: Uint8Array,
  width: number,
  height: number,
): Uint8Array {
  return _rgba_to_gray(rgba, width, height);
}

/**
 * Synthesise a PuzzleBoard target PNG entirely in WASM.
 *
 * Returns PNG bytes suitable for decoding via `createImageBitmap` and feeding
 * back into the detection pipeline to complete a generate → detect roundtrip.
 */
export function renderPuzzleBoardPng(
  rows: number,
  cols: number,
  squareSizeMm: number,
  dpi: number,
): Uint8Array {
  return _render_puzzleboard_png(rows, cols, squareSizeMm, dpi);
}

export function defaultChessConfig(): ChessConfig {
  return _default_chess_config() as ChessConfig;
}

export function defaultChessboardParams(): ChessboardParams {
  return _default_chessboard_params() as ChessboardParams;
}

export function defaultPuzzleBoardParams(rows: number, cols: number): PuzzleBoardParams {
  return _default_puzzleboard_params(rows, cols) as PuzzleBoardParams;
}

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
  params: CharucoDetectorParams,
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
