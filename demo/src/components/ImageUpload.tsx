import { useCallback, useMemo, useRef, useState } from "react";
import type { ImageData } from "../lib/image-utils";
import { loadImage, loadImageFromBytes } from "../lib/image-utils";
import {
  listArucoDictionaries,
  renderCharucoPng,
  renderChessboardPng,
  renderMarkerBoardPng,
  renderPuzzleBoardPng,
  rgbaToGray,
} from "../lib/detector";
import type { SyntheticTargetKind } from "../types/calib-targets";

interface Props {
  onImageLoaded: (data: ImageData) => void;
}

const KIND_OPTIONS: ReadonlyArray<readonly [SyntheticTargetKind, string]> = [
  ["chessboard", "Chessboard"],
  ["charuco", "ChArUco"],
  ["marker_board", "Marker board"],
  ["puzzleboard", "PuzzleBoard"],
];

const DPI = 300;
const SQUARE_MM = 12;

export function ImageUpload({ onImageLoaded }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const [synthKind, setSynthKind] = useState<SyntheticTargetKind>("puzzleboard");
  const [synthRows, setSynthRows] = useState(10);
  const [synthCols, setSynthCols] = useState(10);
  const [charucoDict, setCharucoDict] = useState<string>("DICT_4X4_50");
  const [charucoMarkerRel, setCharucoMarkerRel] = useState(0.75);
  const [synthesising, setSynthesising] = useState(false);
  const [synthError, setSynthError] = useState<string | null>(null);

  const dictionaries = useMemo(() => listArucoDictionaries(), []);

  const handleFile = useCallback(
    async (file: File) => {
      const data = await loadImage(file, rgbaToGray);
      onImageLoaded(data);
    },
    [onImageLoaded],
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) void handleFile(file);
    },
    [handleFile],
  );

  const onDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragging(true);
  }, []);

  const onDragLeave = useCallback(() => setDragging(false), []);

  const onChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) void handleFile(file);
    },
    [handleFile],
  );

  const onSynthesise = useCallback(
    async (event: React.MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      setSynthError(null);
      setSynthesising(true);
      try {
        let bytes: Uint8Array;
        switch (synthKind) {
          case "chessboard":
            bytes = renderChessboardPng(synthRows, synthCols, SQUARE_MM, DPI);
            break;
          case "charuco":
            bytes = renderCharucoPng(
              synthRows,
              synthCols,
              SQUARE_MM,
              charucoMarkerRel,
              charucoDict,
              DPI,
            );
            break;
          case "marker_board":
            bytes = renderMarkerBoardPng(synthRows, synthCols, SQUARE_MM, DPI);
            break;
          case "puzzleboard":
            bytes = renderPuzzleBoardPng(synthRows, synthCols, SQUARE_MM, DPI);
            break;
        }
        const data = await loadImageFromBytes(bytes, "image/png", rgbaToGray);
        onImageLoaded(data);
      } catch (err) {
        setSynthError(err instanceof Error ? err.message : String(err));
      } finally {
        setSynthesising(false);
      }
    },
    [
      onImageLoaded,
      synthKind,
      synthRows,
      synthCols,
      charucoDict,
      charucoMarkerRel,
    ],
  );

  const stop = useCallback(
    (e: React.MouseEvent | React.KeyboardEvent | React.ChangeEvent) =>
      e.stopPropagation(),
    [],
  );

  return (
    <div
      className={`upload-zone ${dragging ? "dragging" : ""}`}
      onDrop={onDrop}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onClick={() => inputRef.current?.click()}
    >
      <input
        ref={inputRef}
        type="file"
        accept="image/*"
        onChange={onChange}
        style={{ display: "none" }}
      />
      <p>Drop image here or click to browse</p>
      <div className="synthetic-zone" onClick={stop}>
        <span className="synthetic-label">or generate a synthetic target</span>
        <div className="synthetic-row">
          <label>
            kind
            <select
              value={synthKind}
              onClick={stop}
              onChange={(e) =>
                setSynthKind(e.target.value as SyntheticTargetKind)
              }
            >
              {KIND_OPTIONS.map(([k, label]) => (
                <option key={k} value={k}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            rows
            <input
              type="number"
              min={2}
              max={64}
              value={synthRows}
              onClick={stop}
              onChange={(e) => setSynthRows(Math.max(2, Number(e.target.value)))}
            />
          </label>
          <label>
            cols
            <input
              type="number"
              min={2}
              max={64}
              value={synthCols}
              onClick={stop}
              onChange={(e) => setSynthCols(Math.max(2, Number(e.target.value)))}
            />
          </label>
        </div>
        {synthKind === "charuco" && (
          <div className="synthetic-row">
            <label>
              dictionary
              <select
                value={charucoDict}
                onClick={stop}
                onChange={(e) => setCharucoDict(e.target.value)}
              >
                {dictionaries.map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
              </select>
            </label>
            <label>
              marker rel
              <input
                type="number"
                min={0.1}
                max={0.95}
                step={0.05}
                value={charucoMarkerRel}
                onClick={stop}
                onChange={(e) =>
                  setCharucoMarkerRel(Math.min(0.95, Math.max(0.1, Number(e.target.value))))
                }
              />
            </label>
          </div>
        )}
        <div className="synthetic-row">
          <button type="button" onClick={onSynthesise} disabled={synthesising}>
            {synthesising ? "Generating…" : "Generate & load"}
          </button>
        </div>
        {synthError && <p className="synthetic-error">{synthError}</p>}
      </div>
    </div>
  );
}
