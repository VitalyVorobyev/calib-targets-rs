import { useState, useCallback, useEffect } from "react";
import { ImageUpload } from "./components/ImageUpload";
import { ImageCanvas } from "./components/ImageCanvas";
import { ConfigPanel } from "./components/ConfigPanel";
import { ResultsPanel } from "./components/ResultsPanel";
import { useDetector } from "./hooks/useDetector";
import {
  charucoSweepForBoard,
  chessboardSweepDefault,
  defaultCharucoParams,
  defaultChessConfig,
  defaultChessboardParams,
  defaultMarkerBoardParams,
  defaultPuzzleBoardParams,
  puzzleboardSweepForBoard,
} from "./lib/detector";
import type { ImageData } from "./lib/image-utils";
import type {
  CharucoParams,
  ChessConfig,
  ChessboardParams,
  DetectionMode,
  MarkerBoardParams,
  PuzzleBoardParams,
} from "./types/calib-targets";

const DEFAULT_CHARUCO_DICTIONARY = "DICT_4X4_50";
const DEFAULT_CHARUCO_MARKER_REL = 0.75;

export default function App() {
  const { ready, initError, loading, result, error, timeMs, detect } =
    useDetector();

  const [image, setImage] = useState<ImageData | null>(null);
  const [mode, setMode] = useState<DetectionMode>("chessboard");
  const [useSweep, setUseSweep] = useState(false);
  const [chessCfg, setChessCfg] = useState<ChessConfig | null>(null);
  const [cbParams, setCbParams] = useState<ChessboardParams | null>(null);
  const [charucoParams, setCharucoParams] = useState<CharucoParams | null>(null);
  const [markerParams, setMarkerParams] = useState<MarkerBoardParams | null>(
    null,
  );
  const [puzzleParams, setPuzzleParams] = useState<PuzzleBoardParams | null>(
    null,
  );

  // Load defaults from WASM once initialised
  useEffect(() => {
    if (ready && !chessCfg) {
      setChessCfg(defaultChessConfig());
      setCbParams(defaultChessboardParams());
      setCharucoParams(
        defaultCharucoParams(
          5,
          7,
          DEFAULT_CHARUCO_MARKER_REL,
          DEFAULT_CHARUCO_DICTIONARY,
        ),
      );
      setMarkerParams(defaultMarkerBoardParams());
      setPuzzleParams(defaultPuzzleBoardParams(10, 10));
    }
  }, [ready, chessCfg]);

  const handleDetect = useCallback(() => {
    if (
      !image ||
      !chessCfg ||
      !cbParams ||
      !charucoParams ||
      !markerParams ||
      !puzzleParams
    )
      return;

    const common = {
      gray: image.gray,
      width: image.width,
      height: image.height,
      chessCfg,
    };

    switch (mode) {
      case "corners":
        detect({ mode: "corners", ...common });
        break;
      case "chessboard":
        detect({
          mode: "chessboard",
          ...common,
          params: cbParams,
          sweep: useSweep ? chessboardSweepDefault() : undefined,
        });
        break;
      case "charuco": {
        const sweep = useSweep
          ? charucoSweepForBoard(
              charucoParams.board.rows,
              charucoParams.board.cols,
              charucoParams.board.marker_size_rel,
              charucoParams.board.dictionary,
            )
          : undefined;
        detect({ mode: "charuco", ...common, params: charucoParams, sweep });
        break;
      }
      case "marker_board":
        // The marker-board crate does not yet ship a sweep preset; the toggle
        // is a no-op for this mode and falls through to the single-config path.
        detect({ mode: "marker_board", ...common, params: markerParams });
        break;
      case "puzzleboard": {
        const sweep = useSweep
          ? puzzleboardSweepForBoard(
              puzzleParams.board.rows,
              puzzleParams.board.cols,
            )
          : undefined;
        detect({ mode: "puzzleboard", ...common, params: puzzleParams, sweep });
        break;
      }
    }
  }, [
    image,
    mode,
    useSweep,
    chessCfg,
    cbParams,
    charucoParams,
    markerParams,
    puzzleParams,
    detect,
  ]);

  if (initError) {
    return <div className="init-error">Failed to load WASM: {initError}</div>;
  }

  if (
    !ready ||
    !chessCfg ||
    !cbParams ||
    !charucoParams ||
    !markerParams ||
    !puzzleParams
  ) {
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
            useSweep={useSweep}
            onUseSweepChange={setUseSweep}
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
