import { useState, useEffect, useCallback, useRef } from "react";
import {
  initialize,
  isReady,
  detectCorners,
  detectChessboard,
  detectChessboardBest,
  detectCharuco,
  detectCharucoBest,
  detectMarkerBoard,
  detectMarkerBoardBest,
  detectPuzzleBoard,
  detectPuzzleBoardBest,
} from "../lib/detector";
import type {
  ChessConfig,
  ChessboardParams,
  CharucoParams,
  MarkerBoardParams,
  PuzzleBoardParams,
  Corner,
  ChessboardDetectionResult,
  CharucoDetectionResult,
  MarkerBoardDetectionResult,
  PuzzleBoardDetectionResult,
  DetectionMode,
} from "../types/calib-targets";

export type DetectionResult =
  | { mode: "corners"; corners: Corner[] }
  | { mode: "chessboard"; result: ChessboardDetectionResult | null }
  | { mode: "charuco"; result: CharucoDetectionResult }
  | { mode: "marker_board"; result: MarkerBoardDetectionResult | null }
  | { mode: "puzzleboard"; result: PuzzleBoardDetectionResult };

export type DetectArgs =
  | {
      mode: "corners";
      gray: Uint8Array;
      width: number;
      height: number;
      chessCfg: ChessConfig;
    }
  | {
      mode: "chessboard";
      gray: Uint8Array;
      width: number;
      height: number;
      chessCfg: ChessConfig;
      params: ChessboardParams;
      sweep?: ChessboardParams[];
    }
  | {
      mode: "charuco";
      gray: Uint8Array;
      width: number;
      height: number;
      chessCfg: ChessConfig;
      params: CharucoParams;
      sweep?: CharucoParams[];
    }
  | {
      mode: "marker_board";
      gray: Uint8Array;
      width: number;
      height: number;
      chessCfg: ChessConfig;
      params: MarkerBoardParams;
      sweep?: MarkerBoardParams[];
    }
  | {
      mode: "puzzleboard";
      gray: Uint8Array;
      width: number;
      height: number;
      chessCfg: ChessConfig;
      params: PuzzleBoardParams;
      sweep?: PuzzleBoardParams[];
    };

interface UseDetectorReturn {
  ready: boolean;
  initError: string | null;
  loading: boolean;
  result: DetectionResult | null;
  error: string | null;
  timeMs: number | null;
  detect: (args: DetectArgs) => void;
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

  const detect = useCallback((args: DetectArgs) => {
    setLoading(true);
    setError(null);

    setTimeout(() => {
      try {
        const t0 = performance.now();
        let detectionResult: DetectionResult;

        switch (args.mode) {
          case "corners":
            detectionResult = {
              mode: "corners",
              corners: detectCorners(
                args.gray,
                args.width,
                args.height,
                args.chessCfg,
              ),
            };
            break;
          case "chessboard":
            detectionResult = {
              mode: "chessboard",
              result: args.sweep
                ? detectChessboardBest(
                    args.gray,
                    args.width,
                    args.height,
                    args.sweep,
                  )
                : detectChessboard(
                    args.gray,
                    args.width,
                    args.height,
                    args.chessCfg,
                    args.params,
                  ),
            };
            break;
          case "charuco":
            detectionResult = {
              mode: "charuco",
              result: args.sweep
                ? detectCharucoBest(
                    args.gray,
                    args.width,
                    args.height,
                    args.sweep,
                  )
                : detectCharuco(
                    args.gray,
                    args.width,
                    args.height,
                    args.chessCfg,
                    args.params,
                  ),
            };
            break;
          case "marker_board":
            detectionResult = {
              mode: "marker_board",
              result: args.sweep
                ? detectMarkerBoardBest(
                    args.gray,
                    args.width,
                    args.height,
                    args.sweep,
                  )
                : detectMarkerBoard(
                    args.gray,
                    args.width,
                    args.height,
                    args.chessCfg,
                    args.params,
                  ),
            };
            break;
          case "puzzleboard":
            detectionResult = {
              mode: "puzzleboard",
              result: args.sweep
                ? detectPuzzleBoardBest(
                    args.gray,
                    args.width,
                    args.height,
                    args.sweep,
                  )
                : detectPuzzleBoard(
                    args.gray,
                    args.width,
                    args.height,
                    args.chessCfg,
                    args.params,
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
  }, []);

  return { ready, initError, loading, result, error, timeMs, detect };
}

export type { DetectionMode };
