import { useState } from "react";
import type { DetectionResult } from "../hooks/useDetector";

interface Props {
  result: DetectionResult | null;
  timeMs: number | null;
  error: string | null;
}

function cornerCount(result: DetectionResult): number {
  if (result.mode === "corners") return result.corners.length;
  if (result.mode === "chessboard")
    return result.result?.detection.corners.length ?? 0;
  if (result.mode === "charuco")
    return result.result.detection.corners.length;
  if (result.mode === "marker_board")
    return result.result?.detection.corners.length ?? 0;
  if (result.mode === "puzzleboard")
    return result.result.detection.corners.length;
  return 0;
}

function gridDims(
  result: DetectionResult,
): { rows: number; cols: number } | null {
  let corners;
  if (result.mode === "chessboard" && result.result) {
    corners = result.result.detection.corners;
  } else if (result.mode === "charuco") {
    corners = result.result.detection.corners;
  } else if (result.mode === "marker_board" && result.result) {
    corners = result.result.detection.corners;
  } else if (result.mode === "puzzleboard") {
    corners = result.result.detection.corners;
  } else {
    return null;
  }

  let maxI = 0,
    maxJ = 0;
  for (const c of corners) {
    if (c.grid) {
      maxI = Math.max(maxI, c.grid.i);
      maxJ = Math.max(maxJ, c.grid.j);
    }
  }
  return corners.length > 0 ? { rows: maxJ + 1, cols: maxI + 1 } : null;
}

export function ResultsPanel({ result, timeMs, error }: Props) {
  const [showJson, setShowJson] = useState(false);

  if (error) {
    return (
      <div className="results-panel">
        <h3>Results</h3>
        <div className="error-box">Error: {error}</div>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="results-panel">
        <h3>Results</h3>
        <p className="muted">Run detection to see results.</p>
      </div>
    );
  }

  const count = cornerCount(result);
  const dims = gridDims(result);
  const markerCount =
    result.mode === "charuco" ? result.result.markers.length : null;
  const detected =
    result.mode === "corners" ||
    (result.mode === "chessboard" && result.result != null) ||
    result.mode === "charuco" ||
    (result.mode === "marker_board" && result.result != null) ||
    result.mode === "puzzleboard";

  return (
    <div className="results-panel">
      <h3>Results</h3>
      <table className="results-table">
        <tbody>
          <tr>
            <td>Status</td>
            <td>{detected ? "Detected" : "Not detected"}</td>
          </tr>
          <tr>
            <td>Corners</td>
            <td>{count}</td>
          </tr>
          {dims && (
            <tr>
              <td>Grid</td>
              <td>
                {dims.cols} x {dims.rows}
              </td>
            </tr>
          )}
          {markerCount != null && (
            <tr>
              <td>Markers</td>
              <td>{markerCount}</td>
            </tr>
          )}
          {timeMs != null && (
            <tr>
              <td>Time</td>
              <td>{timeMs.toFixed(1)} ms</td>
            </tr>
          )}
        </tbody>
      </table>

      <button
        className="json-toggle"
        onClick={() => setShowJson(!showJson)}
      >
        {showJson ? "Hide" : "Show"} JSON
      </button>
      {showJson && (
        <pre className="json-view">{JSON.stringify(result, null, 2)}</pre>
      )}
    </div>
  );
}
