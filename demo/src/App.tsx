import { useState, useCallback, useEffect } from "react";
import { ImageUpload } from "./components/ImageUpload";
import { ImageCanvas } from "./components/ImageCanvas";
import { ConfigPanel } from "./components/ConfigPanel";
import { ResultsPanel } from "./components/ResultsPanel";
import { useDetector } from "./hooks/useDetector";
import {
  defaultChessConfig,
  defaultChessboardParams,
  defaultPuzzleBoardParams,
} from "./lib/detector";
import type { ImageData } from "./lib/image-utils";
import type {
  ChessConfig,
  ChessboardParams,
  CharucoDetectorParams,
  MarkerBoardParams,
  PuzzleBoardParams,
  DetectionMode,
} from "./types/calib-targets";

const DEFAULT_CHARUCO_PARAMS: CharucoDetectorParams = {
  charuco: {
    rows: 22,
    cols: 22,
    marker_size_rel: 0.75,
    dictionary: "DICT_4X4_1000",
    marker_layout: "opencv_charuco",
  },
  px_per_square: 60,
  chessboard: {
    min_corner_strength: 0.5,
    min_corners: 32,
    expected_rows: 21,
    expected_cols: 21,
    completeness_threshold: 0.05,
    use_orientation_clustering: true,
    orientation_clustering_params: {
      num_bins: 90,
      max_iters: 10,
      peak_min_separation_deg: 10,
      outlier_threshold_deg: 30,
      min_peak_weight_fraction: 0.05,
      use_weights: true,
    },
    graph: {
      min_spacing_pix: 5,
      max_spacing_pix: 50,
      k_neighbors: 8,
      orientation_tolerance_deg: 22.5,
    },
  },
  scan: {
    marker_size_rel: 0.75,
    inset_frac: 0.06,
    border_bits: 1,
    min_border_score: 0.85,
    dedup_by_id: true,
  },
  max_hamming: 2,
  min_marker_inliers: 8,
  corner_validation_threshold_rel: 0.3,
};

const DEFAULT_MARKER_PARAMS: MarkerBoardParams = {
  layout: {
    rows: 22,
    cols: 22,
    cell_size: 1.0,
    circles: [
      { i: 11, j: 11, polarity: "white" },
      { i: 12, j: 11, polarity: "black" },
      { i: 12, j: 12, polarity: "white" },
    ],
  },
  chessboard: {
    min_corner_strength: 0.0,
    min_corners: 16,
    expected_rows: 22,
    expected_cols: 22,
    completeness_threshold: 0.05,
    use_orientation_clustering: true,
    orientation_clustering_params: {
      num_bins: 90,
      max_iters: 10,
      peak_min_separation_deg: 10,
      outlier_threshold_deg: 30,
      min_peak_weight_fraction: 0.05,
      use_weights: true,
    },
    graph: {
      min_spacing_pix: 5,
      max_spacing_pix: 50,
      k_neighbors: 8,
      orientation_tolerance_deg: 22.5,
    },
  },
  circle_score: {
    patch_size: 64,
    diameter_frac: 0.5,
    ring_thickness_frac: 0.35,
    ring_radius_mul: 1.6,
    min_contrast: 10,
    samples: 48,
    center_search_px: 2,
  },
  match_params: {
    max_candidates_per_polarity: 6,
    min_offset_inliers: 1,
  },
};

export default function App() {
  const { ready, initError, loading, result, error, timeMs, detect } =
    useDetector();

  const [image, setImage] = useState<ImageData | null>(null);
  const [mode, setMode] = useState<DetectionMode>("chessboard");
  const [chessCfg, setChessCfg] = useState<ChessConfig | null>(null);
  const [cbParams, setCbParams] = useState<ChessboardParams | null>(null);
  const [charucoParams, setCharucoParams] =
    useState<CharucoDetectorParams>(DEFAULT_CHARUCO_PARAMS);
  const [markerParams, setMarkerParams] =
    useState<MarkerBoardParams>(DEFAULT_MARKER_PARAMS);
  const [puzzleParams, setPuzzleParams] = useState<PuzzleBoardParams | null>(
    null,
  );

  // Load defaults from WASM once initialized
  useEffect(() => {
    if (ready && !chessCfg) {
      setChessCfg(defaultChessConfig());
      setCbParams(defaultChessboardParams());
      setPuzzleParams(defaultPuzzleBoardParams(10, 10));
    }
  }, [ready, chessCfg]);

  const handleDetect = useCallback(() => {
    if (!image || !chessCfg || !cbParams || !puzzleParams) return;

    let params:
      | ChessboardParams
      | CharucoDetectorParams
      | MarkerBoardParams
      | PuzzleBoardParams;
    switch (mode) {
      case "corners":
        params = cbParams; // unused, but required by signature
        break;
      case "chessboard":
        params = cbParams;
        break;
      case "charuco":
        params = charucoParams;
        break;
      case "marker_board":
        params = markerParams;
        break;
      case "puzzleboard":
        params = puzzleParams;
        break;
    }

    detect(mode, image.gray, image.width, image.height, chessCfg, params);
  }, [image, mode, chessCfg, cbParams, charucoParams, markerParams, puzzleParams, detect]);

  if (initError) {
    return <div className="init-error">Failed to load WASM: {initError}</div>;
  }

  if (!ready || !chessCfg || !cbParams || !puzzleParams) {
    return <div className="loading">Loading WASM module...</div>;
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>calib-targets demo</h1>
        <span className="subtitle">
          WebAssembly calibration target detection
        </span>
      </header>

      <div className="app-body">
        <aside className="sidebar">
          <ImageUpload onImageLoaded={setImage} />
          <ConfigPanel
            mode={mode}
            onModeChange={setMode}
            chessCfg={chessCfg}
            onChessCfgChange={setChessCfg}
            chessboardParams={cbParams}
            onChessboardParamsChange={setCbParams}
            charucoParams={charucoParams}
            onCharucoParamsChange={setCharucoParams}
            markerParams={markerParams}
            onMarkerParamsChange={setMarkerParams}
            puzzleParams={puzzleParams}
            onPuzzleParamsChange={setPuzzleParams}
            onDetect={handleDetect}
            loading={loading}
            hasImage={image != null}
          />
        </aside>

        <main className="main-canvas">
          <ImageCanvas image={image} detection={result} />
        </main>

        <aside className="results-sidebar">
          <ResultsPanel result={result} timeMs={timeMs} error={error} />
        </aside>
      </div>
    </div>
  );
}
