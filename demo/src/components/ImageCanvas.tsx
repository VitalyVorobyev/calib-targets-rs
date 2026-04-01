import { useEffect, useRef } from "react";
import type { ImageData } from "../lib/image-utils";
import type { DetectionResult } from "../hooks/useDetector";
import type { Corner, LabeledCorner } from "../types/calib-targets";

interface Props {
  image: ImageData | null;
  detection: DetectionResult | null;
}

// Color palette for grid-position-based coloring
const PALETTE = [
  "#e6194b",
  "#3cb44b",
  "#ffe119",
  "#4363d8",
  "#f58231",
  "#911eb4",
  "#42d4f4",
  "#f032e6",
  "#bfef45",
  "#fabed4",
  "#469990",
  "#dcbeff",
];

function cornerColor(corner: LabeledCorner, _idx: number): string {
  if (corner.id != null) {
    return PALETTE[corner.id % PALETTE.length]!;
  }
  if (corner.grid) {
    return PALETTE[(corner.grid.i * 7 + corner.grid.j) % PALETTE.length]!;
  }
  return "#00ff00";
}

function rawCornerColor(corner: Corner, _idx: number): string {
  // Color by orientation (hue from 0..pi mapped to 0..360)
  const hue = ((corner.orientation / Math.PI) * 360 + 360) % 360;
  return `hsl(${hue}, 100%, 50%)`;
}

export function ImageCanvas({ image, detection }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !image) return;

    canvas.width = image.width;
    canvas.height = image.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Draw image from RGBA data
    const imgData = new globalThis.ImageData(
      new Uint8ClampedArray(image.rgba),
      image.width,
      image.height,
    );
    ctx.putImageData(imgData, 0, 0);

    if (!detection) return;

    if (detection.mode === "corners") {
      // Draw raw corners
      for (let i = 0; i < detection.corners.length; i++) {
        const c = detection.corners[i]!;
        const color = rawCornerColor(c, i);
        const radius = Math.max(2, Math.min(6, c.strength * 3));
        const [cx, cy] = c.position;

        ctx.beginPath();
        ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
        ctx.fillStyle = color;
        ctx.globalAlpha = 0.8;
        ctx.fill();

        // Draw orientation line
        const len = radius * 2.5;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(
          cx + len * Math.cos(c.orientation),
          cy + len * Math.sin(c.orientation),
        );
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
      return;
    }

    // For detection results with TargetDetection
    let corners: LabeledCorner[] = [];
    if (detection.mode === "chessboard" && detection.result) {
      corners = detection.result.detection.corners;
    } else if (detection.mode === "charuco") {
      corners = detection.result.detection.corners;
    } else if (detection.mode === "marker_board" && detection.result) {
      corners = detection.result.detection.corners;
    }

    // Draw grid edges between adjacent corners
    if (corners.length > 0) {
      const gridMap = new Map<string, LabeledCorner>();
      for (const c of corners) {
        if (c.grid) {
          gridMap.set(`${c.grid.i},${c.grid.j}`, c);
        }
      }

      ctx.strokeStyle = "rgba(0, 255, 0, 0.4)";
      ctx.lineWidth = 1;
      for (const c of corners) {
        if (!c.grid) continue;
        const right = gridMap.get(`${c.grid.i + 1},${c.grid.j}`);
        const down = gridMap.get(`${c.grid.i},${c.grid.j + 1}`);
        if (right) {
          ctx.beginPath();
          ctx.moveTo(c.position[0], c.position[1]);
          ctx.lineTo(right.position[0], right.position[1]);
          ctx.stroke();
        }
        if (down) {
          ctx.beginPath();
          ctx.moveTo(c.position[0], c.position[1]);
          ctx.lineTo(down.position[0], down.position[1]);
          ctx.stroke();
        }
      }
    }

    // Draw detected corners
    for (let i = 0; i < corners.length; i++) {
      const c = corners[i]!;
      const color = cornerColor(c, i);
      const radius = 4;
      const [cx, cy] = c.position;

      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.9;
      ctx.fill();

      // ID label for charuco corners
      if (c.id != null) {
        ctx.font = "10px monospace";
        ctx.fillStyle = "white";
        ctx.globalAlpha = 1;
        ctx.fillText(String(c.id), cx + 6, cy - 4);
      }
    }
    ctx.globalAlpha = 1;
  }, [image, detection]);

  if (!image) {
    return <div className="canvas-placeholder">No image loaded</div>;
  }

  return (
    <div className="canvas-container">
      <canvas
        ref={canvasRef}
        style={{ maxWidth: "100%", height: "auto" }}
      />
    </div>
  );
}
