import type { DetectInput } from "../worker/protocol";
import type { AppState } from "./types";

/** The detector run the current settings describe; null until the defaults have loaded. */
export function inputOf({
  mode,
  chess,
  params,
  sweep,
}: Pick<AppState, "mode" | "chess" | "params" | "sweep">): DetectInput | null {
  if (chess === null || params === null) return null;
  switch (mode) {
    case "corners":
      return { mode, chess, sweep };
    case "chessboard":
      return { mode, chess, sweep, params: params.chessboard };
    case "charuco":
      return { mode, chess, sweep, params: params.charuco };
    case "markerboard":
      return { mode, chess, sweep, params: params.markerboard };
    case "puzzleboard":
      return { mode, chess, sweep, params: params.puzzleboard };
  }
}
