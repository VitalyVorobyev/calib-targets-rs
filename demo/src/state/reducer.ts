import { applyBoard } from "../lib/boards";
import { inputOf } from "./input";
import type { Action, AppState, GenerateForm, PrintableKind } from "./types";

/** The Generate form's starting size for each kind, in that kind's own unit (see `GenerateForm`). */
export const GENERATE_DEFAULTS: Record<PrintableKind, Pick<GenerateForm, "rows" | "cols">> = {
  chessboard: { rows: 7, cols: 10 },
  charuco: { rows: 5, cols: 7 },
  marker_board: { rows: 6, cols: 8 },
  puzzleboard: { rows: 10, cols: 10 },
  puzzlepole: { rows: 0, cols: 0 },
};

export const INITIAL_STATE: AppState = {
  tab: "detect",
  defaults: null,
  bootError: null,
  mode: "chessboard",
  chess: null,
  params: null,
  sweep: false,
  auto: false,
  image: null,
  imageStatus: { status: "idle" },
  rev: 0,
  run: { status: "idle" },
  layers: { overlay: true, edges: true, labels: true, markers: true, circles: true, bits: true, ellipses: false },
  ellipseScale: 50,
  selectedId: null,
  gen: {
    kind: "chessboard",
    ...GENERATE_DEFAULTS.chessboard,
    dictionary: "DICT_4X4_50",
    markerRel: 0.75,
    squareMm: 12,
    dpi: 300,
    circumference: 12,
    axial: 10,
  },
  generated: null,
  genStatus: { status: "idle" },
};

/** Every change a detector run depends on goes through here: the old result is then stale. */
function changed(state: AppState, patch: Partial<AppState>): AppState {
  return { ...state, ...patch, rev: state.rev + 1 };
}

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "booted": {
      const { defaults } = action;
      // The first pole period is a valid starting point; every period closes seamlessly.
      const firstPole = defaults.poleperiods[0]?.[0] ?? state.gen.circumference;
      return {
        ...state,
        defaults,
        bootError: null,
        chess: defaults.chess,
        params: defaults.params,
        gen: { ...state.gen, circumference: firstPole },
      };
    }
    case "bootFailed":
      return { ...state, bootError: action.message };
    case "setTab":
      return { ...state, tab: action.tab };
    case "setMode":
      return changed(state, { mode: action.mode, selectedId: null });
    case "setChess":
      return changed(state, { chess: action.chess });
    case "setParams":
      return state.params === null
        ? state
        : changed(state, { params: { ...state.params, [action.mode]: action.params } });
    case "setBoard":
      return state.params === null ? state : changed(state, { params: applyBoard(state.params, action.sync) });
    case "setSweep":
      return changed(state, { sweep: action.sweep });
    case "setAuto":
      return { ...state, auto: action.auto };
    case "resetSettings":
      return state.defaults === null
        ? state
        : changed(state, { chess: state.defaults.chess, params: state.defaults.params });
    case "imageLoading":
      return { ...state, imageStatus: { status: "loading", name: action.name } };
    case "imageFailed":
      return { ...state, imageStatus: { status: "error", message: action.message } };
    case "imageLoaded": {
      const params = state.params !== null && action.sync ? applyBoard(state.params, action.sync) : state.params;
      return changed(state, {
        image: action.image,
        imageStatus: { status: "idle" },
        params,
        mode: action.mode ?? state.mode,
        run: { status: "idle" },
        selectedId: null,
        tab: "detect",
      });
    }
    case "runStarted":
      return { ...state, run: { status: "running", rev: state.rev }, selectedId: null };
    case "runFinished": {
      const input = inputOf(state);
      if (state.run.status !== "running" || state.run.rev !== action.rev || input === null) return state;
      return { ...state, run: { status: "done", rev: action.rev, outcome: action.outcome, input } };
    }
    case "runFailed":
      if (state.run.status !== "running" || state.run.rev !== action.rev) return state;
      return { ...state, run: { status: "error", rev: action.rev, message: action.message } };
    case "runCancelled":
      return state.run.status === "running" ? { ...state, run: { status: "idle" } } : state;
    case "setLayer":
      return { ...state, layers: { ...state.layers, [action.layer]: action.visible } };
    case "setEllipseScale":
      return { ...state, ellipseScale: action.scale };
    case "select":
      return { ...state, selectedId: action.id };
    case "setGenerate":
      return { ...state, gen: { ...state.gen, ...action.patch } };
    case "setKind": {
      const sizes = action.kind === "puzzlepole" ? {} : GENERATE_DEFAULTS[action.kind];
      return { ...state, gen: { ...state.gen, kind: action.kind, ...sizes } };
    }
    case "generated":
      return { ...state, generated: action.generated, genStatus: { status: "idle" } };
    case "genStatus":
      return { ...state, genStatus: action.status };
  }
}
