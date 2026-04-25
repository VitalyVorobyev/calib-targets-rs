import { useCallback, useMemo } from "react";
import { listArucoDictionaries } from "../lib/detector";
import type {
  CharucoParams,
  ChessConfig,
  ChessboardParams,
  DetectionMode,
  MarkerBoardParams,
  PuzzleBoardParams,
} from "../types/calib-targets";

interface Props {
  mode: DetectionMode;
  onModeChange: (mode: DetectionMode) => void;
  useSweep: boolean;
  onUseSweepChange: (v: boolean) => void;
  chessCfg: ChessConfig;
  onChessCfgChange: (cfg: ChessConfig) => void;
  chessboardParams: ChessboardParams;
  onChessboardParamsChange: (p: ChessboardParams) => void;
  charucoParams: CharucoParams;
  onCharucoParamsChange: (p: CharucoParams) => void;
  markerParams: MarkerBoardParams;
  onMarkerParamsChange: (p: MarkerBoardParams) => void;
  puzzleParams: PuzzleBoardParams;
  onPuzzleParamsChange: (p: PuzzleBoardParams) => void;
  onDetect: () => void;
  loading: boolean;
  hasImage: boolean;
}

function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="slider-row">
      <label>
        {label}: <strong>{value}</strong>
      </label>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}

function NumberInput({
  label,
  value,
  onChange,
  min,
  max,
}: {
  label: string;
  value: number | null | undefined;
  onChange: (v: number | null) => void;
  min?: number;
  max?: number;
}) {
  return (
    <div className="input-row">
      <label>{label}:</label>
      <input
        type="number"
        value={value ?? ""}
        placeholder="auto"
        min={min}
        max={max}
        onChange={(e) => {
          const s = e.target.value;
          onChange(s === "" ? null : Number(s));
        }}
      />
    </div>
  );
}

export function ConfigPanel({
  mode,
  onModeChange,
  useSweep,
  onUseSweepChange,
  chessCfg,
  onChessCfgChange,
  chessboardParams,
  onChessboardParamsChange,
  charucoParams,
  onCharucoParamsChange,
  markerParams,
  onMarkerParamsChange,
  puzzleParams,
  onPuzzleParamsChange,
  onDetect,
  loading,
  hasImage,
}: Props) {
  const dictionaries = useMemo(() => listArucoDictionaries(), []);

  const updateChess = useCallback(
    (partial: Partial<ChessConfig>) => {
      onChessCfgChange({ ...chessCfg, ...partial });
    },
    [chessCfg, onChessCfgChange],
  );

  const updateCb = useCallback(
    (partial: Partial<ChessboardParams>) => {
      onChessboardParamsChange({ ...chessboardParams, ...partial });
    },
    [chessboardParams, onChessboardParamsChange],
  );

  const updateCharuco = useCallback(
    (partial: Partial<CharucoParams>) => {
      onCharucoParamsChange({ ...charucoParams, ...partial });
    },
    [charucoParams, onCharucoParamsChange],
  );

  const updateMarker = useCallback(
    (partial: Partial<MarkerBoardParams>) => {
      onMarkerParamsChange({ ...markerParams, ...partial });
    },
    [markerParams, onMarkerParamsChange],
  );

  const updatePuzzle = useCallback(
    (partial: Partial<PuzzleBoardParams>) => {
      onPuzzleParamsChange({ ...puzzleParams, ...partial });
    },
    [puzzleParams, onPuzzleParamsChange],
  );

  const sweepDisabled = mode === "corners" || mode === "marker_board";

  return (
    <div className="config-panel">
      <h3>Detection Mode</h3>
      <div className="mode-selector">
        {(
          [
            ["corners", "Corners"],
            ["chessboard", "Chessboard"],
            ["charuco", "ChArUco"],
            ["marker_board", "Marker Board"],
            ["puzzleboard", "PuzzleBoard"],
          ] as const
        ).map(([m, label]) => (
          <label key={m} className="radio-label">
            <input
              type="radio"
              name="mode"
              value={m}
              checked={mode === m}
              onChange={() => onModeChange(m)}
            />
            {label}
          </label>
        ))}
      </div>

      <label className="sweep-toggle">
        <input
          type="checkbox"
          checked={useSweep && !sweepDisabled}
          disabled={sweepDisabled}
          onChange={(e) => onUseSweepChange(e.target.checked)}
        />
        Use 3-config sweep ({sweepDisabled ? "n/a for this mode" : "detect_*_best"})
      </label>

      <h3>ChESS Corner Config</h3>
      <Slider
        label="Threshold"
        value={chessCfg.threshold_value}
        min={0}
        max={1}
        step={0.01}
        onChange={(v) => updateChess({ threshold_value: v })}
      />
      <Slider
        label="NMS Radius"
        value={chessCfg.nms_radius}
        min={1}
        max={10}
        step={1}
        onChange={(v) => updateChess({ nms_radius: v })}
      />
      <Slider
        label="Pyramid Levels"
        value={chessCfg.pyramid_levels}
        min={1}
        max={5}
        step={1}
        onChange={(v) => updateChess({ pyramid_levels: v })}
      />

      {mode !== "corners" && (
        <>
          <h3>Chessboard Params</h3>
          <Slider
            label="Min Corner Strength"
            value={chessboardParams.min_corner_strength}
            min={0}
            max={1}
            step={0.01}
            onChange={(v) => updateCb({ min_corner_strength: v })}
          />
          <Slider
            label="Min Labeled Corners"
            value={chessboardParams.min_labeled_corners}
            min={4}
            max={64}
            step={1}
            onChange={(v) => updateCb({ min_labeled_corners: v })}
          />
          <Slider
            label="Max Components"
            value={chessboardParams.max_components}
            min={1}
            max={8}
            step={1}
            onChange={(v) => updateCb({ max_components: v })}
          />
          <NumberInput
            label="Cell Size Hint (px)"
            value={chessboardParams.cell_size_hint}
            min={1}
            max={500}
            onChange={(v) =>
              updateCb({ cell_size_hint: v ?? undefined })
            }
          />
        </>
      )}

      {mode === "charuco" && (
        <>
          <h3>ChArUco Board</h3>
          <NumberInput
            label="Board Rows"
            value={charucoParams.board.rows}
            min={2}
            max={50}
            onChange={(v) =>
              updateCharuco({
                board: { ...charucoParams.board, rows: v ?? 5 },
              })
            }
          />
          <NumberInput
            label="Board Cols"
            value={charucoParams.board.cols}
            min={2}
            max={50}
            onChange={(v) =>
              updateCharuco({
                board: { ...charucoParams.board, cols: v ?? 7 },
              })
            }
          />
          <Slider
            label="Marker Size Rel"
            value={charucoParams.board.marker_size_rel}
            min={0.1}
            max={0.95}
            step={0.05}
            onChange={(v) =>
              updateCharuco({
                board: { ...charucoParams.board, marker_size_rel: v },
              })
            }
          />
          <div className="input-row">
            <label>Dictionary:</label>
            <select
              value={charucoParams.board.dictionary}
              onChange={(e) =>
                updateCharuco({
                  board: { ...charucoParams.board, dictionary: e.target.value },
                })
              }
            >
              {dictionaries.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </div>
          <Slider
            label="Px Per Square"
            value={charucoParams.px_per_square}
            min={10}
            max={200}
            step={5}
            onChange={(v) => updateCharuco({ px_per_square: v })}
          />
          <Slider
            label="Max Hamming"
            value={charucoParams.max_hamming}
            min={0}
            max={10}
            step={1}
            onChange={(v) => updateCharuco({ max_hamming: v })}
          />
        </>
      )}

      {mode === "marker_board" && (
        <>
          <h3>Marker Board Layout</h3>
          <NumberInput
            label="Board Rows"
            value={markerParams.layout.rows}
            min={2}
            max={50}
            onChange={(v) =>
              updateMarker({
                layout: { ...markerParams.layout, rows: v ?? 6 },
              })
            }
          />
          <NumberInput
            label="Board Cols"
            value={markerParams.layout.cols}
            min={2}
            max={50}
            onChange={(v) =>
              updateMarker({
                layout: { ...markerParams.layout, cols: v ?? 8 },
              })
            }
          />
        </>
      )}

      {mode === "puzzleboard" && (
        <>
          <h3>PuzzleBoard</h3>
          <NumberInput
            label="Board Rows"
            value={puzzleParams.board.rows}
            min={4}
            max={501}
            onChange={(v) =>
              updatePuzzle({
                board: { ...puzzleParams.board, rows: v ?? 10 },
              })
            }
          />
          <NumberInput
            label="Board Cols"
            value={puzzleParams.board.cols}
            min={4}
            max={501}
            onChange={(v) =>
              updatePuzzle({
                board: { ...puzzleParams.board, cols: v ?? 10 },
              })
            }
          />
          <Slider
            label="Min Bit Confidence"
            value={puzzleParams.decode.min_bit_confidence}
            min={0}
            max={1}
            step={0.01}
            onChange={(v) =>
              updatePuzzle({
                decode: { ...puzzleParams.decode, min_bit_confidence: v },
              })
            }
          />
        </>
      )}

      <button
        className="detect-button"
        onClick={onDetect}
        disabled={loading || !hasImage}
      >
        {loading ? "Detecting..." : "Detect"}
      </button>
    </div>
  );
}
