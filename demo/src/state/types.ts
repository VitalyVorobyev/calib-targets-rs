import type { ChessConfig } from "@vitavision/calib-targets";

import type { BoardSync } from "../lib/boards";
import type { ImageRecord } from "../lib/image";
import type {
  DetectInput,
  DetectMode,
  DetectOutcome,
  Defaults,
  RenderedTarget,
  TargetParams,
} from "../worker/protocol";

export type Tab = "detect" | "generate";

/** The printable kinds. `marker_board` and `puzzlepole` are the printable document's names. */
export type PrintableKind = "chessboard" | "charuco" | "marker_board" | "puzzleboard" | "puzzlepole";

/**
 * The Generate tab's form. `rows` and `cols` count the board's INNER CORNERS for `chessboard`
 * and `marker_board` and its SQUARES for `charuco` and `puzzleboard` (the renderers' own
 * conventions); the form labels them by kind. A pole uses `circumference` and `axial` instead.
 */
export interface GenerateForm {
  kind: PrintableKind;
  rows: number;
  cols: number;
  dictionary: string;
  markerRel: number;
  squareMm: number;
  dpi: number;
  circumference: number;
  axial: number;
}

export type LayerId = "overlay" | "edges" | "labels" | "markers" | "circles" | "bits" | "ellipses";

export type RunState =
  | { status: "idle" }
  | { status: "running"; rev: number }
  | { status: "done"; rev: number; outcome: DetectOutcome; input: DetectInput }
  | { status: "error"; rev: number; message: string };

export interface AppState {
  tab: Tab;
  /** Null until the worker has read the defaults; `bootError` says why if it never will. */
  defaults: Defaults | null;
  bootError: string | null;
  mode: DetectMode;
  chess: ChessConfig | null;
  params: TargetParams | null;
  sweep: boolean;
  auto: boolean;
  image: ImageRecord | null;
  imageStatus: { status: "idle" } | { status: "loading"; name: string } | { status: "error"; message: string };
  /** Bumped by every change a detector run depends on; a result from an older one is stale. */
  rev: number;
  run: RunState;
  layers: Record<LayerId, boolean>;
  /** The ellipses are drawn this many times the 1-sigma size. */
  ellipseScale: number;
  selectedId: string | null;
  gen: GenerateForm;
  /** The target last rendered, for the downloads. */
  generated: { label: string; stem: string; rendered: RenderedTarget } | null;
  genStatus: { status: "idle" } | { status: "busy" } | { status: "error"; message: string };
}

export type Action =
  | { type: "booted"; defaults: Defaults }
  | { type: "bootFailed"; message: string }
  | { type: "setTab"; tab: Tab }
  | { type: "setMode"; mode: DetectMode }
  | { type: "setChess"; chess: ChessConfig }
  | { type: "setParams"; mode: Exclude<DetectMode, "corners">; params: TargetParams[Exclude<DetectMode, "corners">] }
  | { type: "setBoard"; sync: BoardSync }
  | { type: "setSweep"; sweep: boolean }
  | { type: "setAuto"; auto: boolean }
  | { type: "resetSettings" }
  | { type: "imageLoading"; name: string }
  | { type: "imageFailed"; message: string }
  /** `sync` also switches the detector to the board the image shows (a sample, a generated target). */
  | { type: "imageLoaded"; image: ImageRecord; sync?: BoardSync | undefined; mode?: DetectMode | undefined }
  | { type: "runStarted" }
  | { type: "runFinished"; rev: number; outcome: DetectOutcome }
  | { type: "runFailed"; rev: number; message: string }
  | { type: "runCancelled" }
  | { type: "setLayer"; layer: LayerId; visible: boolean }
  | { type: "setEllipseScale"; scale: number }
  | { type: "select"; id: string | null }
  | { type: "setGenerate"; patch: Partial<GenerateForm> }
  | { type: "setKind"; kind: PrintableKind }
  | { type: "generated"; generated: NonNullable<AppState["generated"]> }
  | { type: "genStatus"; status: AppState["genStatus"] };
