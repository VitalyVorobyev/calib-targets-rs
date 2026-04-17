import { useCallback, useRef, useState } from "react";
import type { ImageData } from "../lib/image-utils";
import { loadImage, loadImageFromBytes } from "../lib/image-utils";
import { renderPuzzleBoardPng, rgbaToGray } from "../lib/detector";

interface Props {
  onImageLoaded: (data: ImageData) => void;
}

export function ImageUpload({ onImageLoaded }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const [synthRows, setSynthRows] = useState(10);
  const [synthCols, setSynthCols] = useState(10);
  const [synthesising, setSynthesising] = useState(false);
  const [synthError, setSynthError] = useState<string | null>(null);

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
        const bytes = renderPuzzleBoardPng(synthRows, synthCols, 12.0, 300);
        const data = await loadImageFromBytes(bytes, "image/png", rgbaToGray);
        onImageLoaded(data);
      } catch (err) {
        setSynthError(err instanceof Error ? err.message : String(err));
      } finally {
        setSynthesising(false);
      }
    },
    [onImageLoaded, synthRows, synthCols],
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
        <span className="synthetic-label">or generate a synthetic PuzzleBoard</span>
        <div className="synthetic-row">
          <label>
            rows
            <input
              type="number"
              min={4}
              max={20}
              value={synthRows}
              onClick={stop}
              onChange={(e) => setSynthRows(Math.max(4, Number(e.target.value)))}
            />
          </label>
          <label>
            cols
            <input
              type="number"
              min={4}
              max={20}
              value={synthCols}
              onClick={stop}
              onChange={(e) => setSynthCols(Math.max(4, Number(e.target.value)))}
            />
          </label>
          <button
            type="button"
            onClick={onSynthesise}
            disabled={synthesising}
          >
            {synthesising ? "Generating…" : "Generate & load"}
          </button>
        </div>
        {synthError && <p className="synthetic-error">{synthError}</p>}
      </div>
    </div>
  );
}
