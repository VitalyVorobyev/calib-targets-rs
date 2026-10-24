// Board geometry: the friendly fields, and the one place a printed target becomes detector
// parameters. The generate-then-detect round trip depends on every kind going through here.

import type {
  CharucoBoardSpec,
  MarkerBoardSpec,
  PrintableTargetSpec,
  PuzzleBoardSpec,
} from "@vitavision/calib-targets";

import type { DetectMode, TargetParams } from "../worker/protocol";

/** The board a detector is told to look for. `chessboard` and `corners` have none. */
export type BoardSync =
  | { mode: "chessboard" }
  | { mode: "charuco"; board: CharucoBoardSpec }
  | { mode: "markerboard"; board: MarkerBoardSpec }
  | { mode: "puzzleboard"; board: PuzzleBoardSpec };

/**
 * Put a board into the parameters of its detector.
 *
 * `CharucoParams.for_board` also derives the marker-decode settings from the board (the marker
 * size and the border ring), so a board edit has to carry them along or the decoder would sample
 * the old marker size.
 */
export function applyBoard(params: TargetParams, sync: BoardSync): TargetParams {
  switch (sync.mode) {
    case "charuco":
      return {
        ...params,
        charuco: {
          ...params.charuco,
          board: sync.board,
          scan: {
            ...params.charuco.scan,
            marker_size_rel: sync.board.marker_size_rel,
            border_bits: sync.board.border_bits ?? 1,
          },
        },
      };
    case "markerboard":
      return { ...params, markerboard: { ...params.markerboard, board: sync.board } };
    case "puzzleboard":
      return { ...params, puzzleboard: { ...params.puzzleboard, board: sync.board } };
    case "chessboard":
      return params;
  }
}

/** The detector board of a rendered printable target. */
export function boardOfTarget(target: PrintableTargetSpec): BoardSync {
  switch (target.kind) {
    case "chessboard":
      return { mode: "chessboard" };
    case "charuco": {
      const board: CharucoBoardSpec = {
        rows: target.rows,
        cols: target.cols,
        cell_size: target.square_size_mm,
        marker_size_rel: target.marker_size_rel,
        dictionary: target.dictionary,
      };
      if (target.marker_layout != null) board.marker_layout = target.marker_layout;
      if (target.border_bits != null) board.border_bits = target.border_bits;
      return { mode: "charuco", board };
    }
    case "marker_board": {
      const [a, b, c] = target.circles;
      const cell = (circle: typeof a) => ({ cell: { i: circle.i, j: circle.j }, polarity: circle.polarity });
      return {
        mode: "markerboard",
        board: {
          rows: target.inner_rows,
          cols: target.inner_cols,
          cell_size: target.square_size_mm,
          circles: [cell(a), cell(b), cell(c)],
          circle_diameter_rel: target.circle_diameter_rel,
        },
      };
    }
    case "puzzle_board":
      return {
        mode: "puzzleboard",
        board: {
          rows: target.rows,
          cols: target.cols,
          cell_size: target.square_size_mm,
          origin_row: target.origin_row ?? 0,
          origin_col: target.origin_col ?? 0,
        },
      };
    case "puzzlepole":
      // A pole strip is a window of the PuzzleBoard master pattern. The page runs along the axis
      // (x) and round the circumference (y), two pieces longer than it wraps for the trim and
      // the glue overlap. The decoder searches the whole master, so the board only has to size
      // the sampling; the origin is left at 0.
      return {
        mode: "puzzleboard",
        board: {
          rows: target.circumference_squares + 2,
          cols: target.axial_squares,
          cell_size: target.square_size_mm,
          origin_row: 0,
          origin_col: 0,
        },
      };
  }
}

export function isBoardMode(mode: DetectMode): mode is Exclude<DetectMode, "corners"> {
  return mode !== "corners";
}
