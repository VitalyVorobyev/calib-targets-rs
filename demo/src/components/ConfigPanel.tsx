import { useCallback } from "react";
import type {
  ChessConfig,
  ChessboardParams,
  CharucoDetectorParams,
  MarkerBoardParams,
  DetectionMode,
} from "../types/calib-targets";

interface Props {
  mode: DetectionMode;
  onModeChange: (mode: DetectionMode) => void;
  chessCfg: ChessConfig;
  onChessCfgChange: (cfg: ChessConfig) => void;
  chessboardParams: ChessboardParams;
  onChessboardParamsChange: (p: ChessboardParams) => void;
  charucoParams: CharucoDetectorParams;
  onCharucoParamsChange: (p: CharucoDetectorParams) => void;
  markerParams: MarkerBoardParams;
  onMarkerParamsChange: (p: MarkerBoardParams) => void;
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
  chessCfg,
  onChessCfgChange,
  chessboardParams,
  onChessboardParamsChange,
  charucoParams,
  onCharucoParamsChange,
  markerParams,
  onMarkerParamsChange,
  onDetect,
  loading,
  hasImage,
}: Props) {
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
    (partial: Partial<CharucoDetectorParams>) => {
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
            label="Min Strength"
            value={chessboardParams.min_corner_strength}
            min={0}
            max={1}
            step={0.01}
            onChange={(v) => updateCb({ min_corner_strength: v })}
          />
          <Slider
            label="Min Corners"
            value={chessboardParams.min_corners}
            min={4}
            max={200}
            step={1}
            onChange={(v) => updateCb({ min_corners: v })}
          />
          <NumberInput
            label="Expected Rows"
            value={chessboardParams.expected_rows}
            min={2}
            max={100}
            onChange={(v) => updateCb({ expected_rows: v })}
          />
          <NumberInput
            label="Expected Cols"
            value={chessboardParams.expected_cols}
            min={2}
            max={100}
            onChange={(v) => updateCb({ expected_cols: v })}
          />
          <Slider
            label="Completeness"
            value={chessboardParams.completeness_threshold}
            min={0}
            max={1}
            step={0.01}
            onChange={(v) => updateCb({ completeness_threshold: v })}
          />
          <Slider
            label="Max Spacing (px)"
            value={chessboardParams.graph.max_spacing_pix}
            min={10}
            max={500}
            step={5}
            onChange={(v) =>
              updateCb({
                graph: { ...chessboardParams.graph, max_spacing_pix: v },
              })
            }
          />
        </>
      )}

      {mode === "charuco" && (
        <>
          <h3>ChArUco Board</h3>
          <NumberInput
            label="Board Rows"
            value={charucoParams.charuco.rows}
            min={2}
            max={50}
            onChange={(v) =>
              updateCharuco({
                charuco: { ...charucoParams.charuco, rows: v ?? 8 },
              })
            }
          />
          <NumberInput
            label="Board Cols"
            value={charucoParams.charuco.cols}
            min={2}
            max={50}
            onChange={(v) =>
              updateCharuco({
                charuco: { ...charucoParams.charuco, cols: v ?? 8 },
              })
            }
          />
          <Slider
            label="Marker Size Rel"
            value={charucoParams.charuco.marker_size_rel}
            min={0.1}
            max={0.95}
            step={0.05}
            onChange={(v) =>
              updateCharuco({
                charuco: { ...charucoParams.charuco, marker_size_rel: v },
              })
            }
          />
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
                layout: { ...markerParams.layout, rows: v ?? 22 },
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
                layout: { ...markerParams.layout, cols: v ?? 22 },
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
