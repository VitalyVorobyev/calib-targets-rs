import { useState, useEffect, useCallback, useRef } from "react";
import {
  initialize,
  isReady,
  detectCorners,
  detectChessboard,
  detectCharuco,
  detectMarkerBoard,
} from "../lib/detector";
import type {
  ChessConfig,
  ChessboardParams,
  CharucoDetectorParams,
  MarkerBoardParams,
  Corner,
  ChessboardDetectionResult,
  CharucoDetectionResult,
  MarkerBoardDetectionResult,
  DetectionMode,
} from "../types/calib-targets";

export type DetectionResult =
  | { mode: "corners"; corners: Corner[] }
  | { mode: "chessboard"; result: ChessboardDetectionResult | null }
  | { mode: "charuco"; result: CharucoDetectionResult }
  | { mode: "marker_board"; result: MarkerBoardDetectionResult | null };

interface UseDetectorReturn {
  ready: boolean;
  initError: string | null;
  loading: boolean;
  result: DetectionResult | null;
  error: string | null;
  timeMs: number | null;
  detect: (
    mode: DetectionMode,
    gray: Uint8Array,
    width: number,
    height: number,
    chessCfg: ChessConfig,
    params: ChessboardParams | CharucoDetectorParams | MarkerBoardParams,
  ) => void;
}

export function useDetector(): UseDetectorReturn {
  const [ready, setReady] = useState(isReady());
  const [initError, setInitError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<DetectionResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [timeMs, setTimeMs] = useState<number | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    if (!isReady()) {
      initialize()
        .then(() => {
          if (mountedRef.current) setReady(true);
        })
        .catch((e: unknown) => {
          if (mountedRef.current)
            setInitError(e instanceof Error ? e.message : String(e));
        });
    }
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const detect = useCallback(
    (
      mode: DetectionMode,
      gray: Uint8Array,
      width: number,
      height: number,
      chessCfg: ChessConfig,
      params: ChessboardParams | CharucoDetectorParams | MarkerBoardParams,
    ) => {
      setLoading(true);
      setError(null);

      // Run detection synchronously (it's WASM, no async needed)
      // but wrap in setTimeout to allow React to show loading state
      setTimeout(() => {
        try {
          const t0 = performance.now();
          let detectionResult: DetectionResult;

          switch (mode) {
            case "corners":
              detectionResult = {
                mode: "corners",
                corners: detectCorners(gray, width, height, chessCfg),
              };
              break;
            case "chessboard":
              detectionResult = {
                mode: "chessboard",
                result: detectChessboard(
                  gray,
                  width,
                  height,
                  chessCfg,
                  params as ChessboardParams,
                ),
              };
              break;
            case "charuco":
              detectionResult = {
                mode: "charuco",
                result: detectCharuco(
                  gray,
                  width,
                  height,
                  chessCfg,
                  params as CharucoDetectorParams,
                ),
              };
              break;
            case "marker_board":
              detectionResult = {
                mode: "marker_board",
                result: detectMarkerBoard(
                  gray,
                  width,
                  height,
                  chessCfg,
                  params as MarkerBoardParams,
                ),
              };
              break;
          }

          const elapsed = performance.now() - t0;
          if (mountedRef.current) {
            setResult(detectionResult);
            setTimeMs(elapsed);
            setLoading(false);
          }
        } catch (e: unknown) {
          if (mountedRef.current) {
            setError(e instanceof Error ? e.message : String(e));
            setResult(null);
            setTimeMs(null);
            setLoading(false);
          }
        }
      }, 0);
    },
    [],
  );

  return { ready, initError, loading, result, error, timeMs, detect };
}
