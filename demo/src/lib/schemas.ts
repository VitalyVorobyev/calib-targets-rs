// The shipped JSON Schemas of the configs (`@vitavision/calib-targets/schemas/*.json`) and what
// the forms add to them: grouping and what the friendly fields already own.

import type { JsonSchema, UiSchema } from "@vitavision/forms";
import charucoSchema from "@vitavision/calib-targets/schemas/charuco_params.json";
import chessSchema from "@vitavision/calib-targets/schemas/chess_config.json";
import chessboardSchema from "@vitavision/calib-targets/schemas/chessboard_params.json";
import markerSchema from "@vitavision/calib-targets/schemas/marker_board_params.json";
import puzzleSchema from "@vitavision/calib-targets/schemas/puzzleboard_params.json";

import type { DetectMode } from "../worker/protocol";

export const CHESS_SCHEMA = chessSchema;

export const CHESS_UI: UiSchema = {
  groups: [
    {
      id: "corners",
      title: "Corner detection",
      fields: ["threshold", "strategy", "refiner", "upscale", "orientation_method"],
    },
    {
      id: "uncertainty",
      title: "Position uncertainty",
      hint: "Per-corner covariance; off by default, and not reported while upscaling.",
      fields: ["uncertainty"],
    },
  ],
};

/**
 * The parameter schemas of the board detectors. `chess` is edited in its own section and `board`
 * in the geometry fields, so the forms hide both.
 */
export const PARAMS_SCHEMA: Record<Exclude<DetectMode, "corners">, JsonSchema> = {
  chessboard: chessboardSchema,
  charuco: charucoSchema,
  markerboard: markerSchema,
  puzzleboard: puzzleSchema,
};

const OWNED_ELSEWHERE: UiSchema["fields"] = {
  chess: { hidden: true },
  board: { hidden: true },
};

export const PARAMS_UI: Record<Exclude<DetectMode, "corners">, UiSchema> = {
  chessboard: {},
  charuco: { fields: OWNED_ELSEWHERE },
  markerboard: { fields: OWNED_ELSEWHERE },
  puzzleboard: { fields: OWNED_ELSEWHERE },
};
