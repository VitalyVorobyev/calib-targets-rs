// The worker that owns the WASM module. Detection and rendering block for as long as the image
// takes, so they run here and the page stays responsive; cancelling a run terminates the worker.

import type { ChessConfig, GeneratedTargetBundle, PrintableTargetSpec } from "@vitavision/calib-targets";

import type {
  DetectOutcome,
  DetectPayload,
  DetectRequest,
  Defaults,
  RenderedTarget,
  RenderRequest,
  WorkerMethods,
  WorkerRequest,
  WorkerResponse,
} from "./protocol";
import * as wasm from "./wasm";

interface GrayImage {
  key: number;
  width: number;
  height: number;
  gray: Uint8Array;
}

let image: GrayImage | null = null;
const ready = wasm.load();

function defaults(): Defaults {
  return {
    chess: wasm.defaultChessConfig(),
    params: {
      chessboard: wasm.defaultChessboardParams(),
      charuco: wasm.defaultCharucoParams(5, 7, 0.75, "DICT_4X4_50"),
      markerboard: wasm.defaultMarkerBoardParams(),
      puzzleboard: wasm.defaultPuzzleBoardParams(10, 10),
    },
    dictionaries: wasm.listArucoDictionaries(),
    poleperiods: wasm.puzzlepolePeriods(),
  };
}

function prepare({ key, rgba, width, height }: WorkerMethods["prepare"]["args"][0]): null {
  image = { key, width, height, gray: wasm.rgbaToGray(new Uint8Array(rgba), width, height) };
  return null;
}

/**
 * A sweep preset carries its own ChESS front-end per config (a sweep may mix them), so the
 * user's ChESS form cannot replace it. What does not change which corners are found, the
 * uncertainty settings, still follows the form.
 */
function withUncertainty<T extends { chess: ChessConfig }>(configs: T[], chess: ChessConfig): T[] {
  return configs.map((config) => ({
    ...config,
    chess: { ...config.chess, uncertainty: chess.uncertainty },
  }));
}

/** A thrown detector message as a miss: the ChArUco and PuzzleBoard detectors throw on one. */
function attempt<T>(run: () => T): { value: T | null; note?: string } {
  try {
    return { value: run() };
  } catch (error) {
    return { value: null, note: error instanceof Error ? error.message : String(error) };
  }
}

const noted = (note: string | undefined) => (note === undefined ? {} : { note });

function runDetector(request: DetectRequest, { width: w, height: h, gray }: GrayImage): DetectPayload {
  const { chess, sweep } = request;
  switch (request.mode) {
    case "corners":
      return { kind: "corners", corners: wasm.detectCorners(w, h, gray, chess) };
    case "chessboard": {
      const detection = sweep
        ? wasm.detectChessboardBest(w, h, gray, chess, wasm.chessboardSweep())
        : wasm.detectChessboard(w, h, gray, chess, request.params);
      return { kind: "chessboard", detection };
    }
    case "charuco": {
      const { params } = request;
      const { value, note } = attempt(() => {
        if (!sweep) return wasm.detectCharuco(w, h, gray, chess, params);
        const { rows, cols, marker_size_rel, dictionary } = params.board;
        const configs = withUncertainty(wasm.charucoSweep(rows, cols, marker_size_rel, dictionary), chess);
        return wasm.detectCharucoBest(w, h, gray, configs);
      });
      return { kind: "charuco", detection: value, ...noted(note) };
    }
    case "markerboard": {
      const { params } = request;
      if (sweep) {
        const configs = withUncertainty(wasm.markerBoardSweep(params.board), chess);
        return { kind: "markerboard", detection: wasm.detectMarkerBoardBest(w, h, gray, configs), diagnostics: null };
      }
      const out = wasm.diagnoseMarkerBoard(w, h, gray, chess, params);
      return { kind: "markerboard", detection: out.result ?? null, diagnostics: out.diagnostics ?? null };
    }
    case "puzzleboard": {
      const { params } = request;
      if (sweep) {
        const { value, note } = attempt(() =>
          wasm.detectPuzzleBoardBest(
            w,
            h,
            gray,
            withUncertainty(wasm.puzzleboardSweep(params.board.rows, params.board.cols), chess),
          ),
        );
        return { kind: "puzzleboard", detection: value, diagnostics: null, ...noted(note) };
      }
      const out = wasm.diagnosePuzzleBoard(w, h, gray, chess, params);
      return { kind: "puzzleboard", detection: out.result ?? null, diagnostics: out.diagnostics ?? null };
    }
  }
}

function detect(request: DetectRequest): DetectOutcome {
  if (image === null || image.key !== request.imageKey) throw new Error("The image was not sent to the worker.");
  const start = performance.now();
  const payload = runDetector(request, image);
  return { payload, timeMs: performance.now() - start };
}

function renderBundle(request: RenderRequest): GeneratedTargetBundle {
  switch (request.kind) {
    case "chessboard":
      return wasm.renderChessboard(request.innerRows, request.innerCols, request.squareMm, request.dpi);
    case "charuco":
      return wasm.renderCharuco(
        request.rows,
        request.cols,
        request.squareMm,
        request.markerRel,
        request.dictionary,
        request.dpi,
      );
    case "marker_board":
      return wasm.renderMarkerBoard(request.innerRows, request.innerCols, request.squareMm, request.dpi);
    case "puzzleboard":
      return wasm.renderPuzzleBoard(request.rows, request.cols, request.squareMm, request.dpi);
    case "puzzlepole":
      return wasm.renderPuzzlePole(request.circumference, request.axial, request.squareMm, request.dpi);
  }
}

function render(request: RenderRequest): RenderedTarget {
  const bundle = renderBundle(request);
  const document = JSON.parse(bundle.json_text) as { target: PrintableTargetSpec };
  return { bundle, target: document.target };
}

function handle(request: WorkerRequest): unknown {
  switch (request.method) {
    case "defaults":
      return defaults();
    case "prepare":
      return prepare(...(request.args as WorkerMethods["prepare"]["args"]));
    case "detect":
      return detect(...(request.args as WorkerMethods["detect"]["args"]));
    case "render":
      return render(...(request.args as WorkerMethods["render"]["args"]));
  }
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  void ready.then(
    () => {
      const transfer: Transferable[] = [];
      let response: WorkerResponse;
      try {
        const value = handle(request);
        if (request.method === "render") transfer.push((value as RenderedTarget).bundle.png_bytes.buffer);
        response = { id: request.id, ok: true, value };
      } catch (error) {
        response = { id: request.id, ok: false, error: error instanceof Error ? error.message : String(error) };
      }
      self.postMessage(response, transfer);
    },
    (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      self.postMessage({ id: request.id, ok: false, error: `The WASM module failed to load: ${message}` });
    },
  );
};
