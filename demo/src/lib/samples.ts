import type { MarkerBoardSpec, PuzzleBoardSpec } from "@vitavision/calib-targets";

import type { DetectMode, TargetParams } from "../worker/protocol";
import type { BoardSync } from "./boards";

export interface Sample {
  id: string;
  label: string;
  url: string;
  /** The detector that matches the sample. */
  mode: DetectMode;
  /** Board geometry to seed when it differs from the detector's default. */
  markerboard?: Pick<MarkerBoardSpec, "rows" | "cols" | "circles">;
  puzzleboard?: Pick<PuzzleBoardSpec, "rows" | "cols">;
}

// The two oblique PuzzleBoards are public photo-realistic synthetic renders (20x20) from the
// canonical 501x501 master maps; they decode uniquely against it. The marker board is the
// 22x22 board of `testdata/marker_detect_config.json`; without its size and circle layout the
// labels would come out relative to a 6x8 placeholder, and negative.
export const SAMPLES: readonly Sample[] = [
  { id: "chessboard", label: "Chessboard", url: "./samples/chessboard.png", mode: "chessboard" },
  { id: "charuco", label: "ChArUco", url: "./samples/charuco.png", mode: "charuco" },
  {
    id: "marker",
    label: "Marker",
    url: "./samples/markerboard.png",
    mode: "markerboard",
    markerboard: {
      rows: 22,
      cols: 22,
      circles: [
        { cell: { i: 11, j: 11 }, polarity: "white" },
        { cell: { i: 12, j: 11 }, polarity: "black" },
        { cell: { i: 12, j: 12 }, polarity: "white" },
      ],
    },
  },
  { id: "puzzleboard", label: "PuzzleBoard", url: "./samples/puzzleboard.png", mode: "puzzleboard" },
  {
    id: "puzzleboard-oblique",
    label: "PuzzleBoard (oblique)",
    url: "./samples/oblique.png",
    mode: "puzzleboard",
    puzzleboard: { rows: 20, cols: 20 },
  },
  {
    id: "puzzleboard-foreshortened",
    label: "PuzzleBoard (foreshortened)",
    url: "./samples/foreshortened.png",
    mode: "puzzleboard",
    puzzleboard: { rows: 20, cols: 20 },
  },
];

/** The board a sample asks the detector for, on top of the board the parameters hold now. */
export function sampleBoard(sample: Sample, params: TargetParams | null): BoardSync | undefined {
  if (params === null) return undefined;
  if (sample.markerboard) {
    return { mode: "markerboard", board: { ...params.markerboard.board, ...sample.markerboard } };
  }
  if (sample.puzzleboard) {
    return { mode: "puzzleboard", board: { ...params.puzzleboard.board, ...sample.puzzleboard } };
  }
  return undefined;
}
