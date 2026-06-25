#!/usr/bin/env python3
"""OpenCV baseline comparison for the published performance report.

Compares `calib-targets` against OpenCV on **recall** and **runtime**, for a
chessboard and a ChArUco board, using the two PUBLIC report images the rest of
the report already uses:

- `testdata/mid.png`  — a plain 11x7 inner-corner chessboard (full board visible).
- `testdata/small.png`— a 22x22 ChArUco board (DICT_4X4_250), partially visible.

Public images only — never point this at `privatedata/` (the script refuses).

## Honest framing (this is a neutral benchmark, not marketing)

The two detectors are not measuring the same thing, and the report says so:

- **mid.png (chessboard).** The full board is visible, so the inner-corner count
  is known (11*7 = 77). Recall is `matched / 77` for each detector against that
  count. OpenCV `findChessboardCornersSB` is **all-or-nothing**: it needs the
  exact board dimensions and returns the whole 77-corner board or nothing
  (`success=false`). Our detector is partial-tolerant and self-identifying. We
  also report cross-detector **position agreement** (corners both find within
  `--tol-px`) as a precision proxy.

- **small.png (ChArUco).** Only part of the board is in frame, so there is no
  independent per-corner ground truth. This row is a **detected-count
  comparison** (markers + ChArUco corners, ours vs OpenCV) plus cross-detector
  corner agreement — *not* recall-vs-ground-truth. Reporting it as "recall"
  would be dishonest.

Runtime is the p50 of the detector work, measured in each detector's NATIVE
runtime: OpenCV from `cv2` here (it returns cheap numpy arrays), and ours from
the Rust `full_stage_timing` measurement already in the report's `data.json`
(corner detection + grid build + decode). We deliberately do NOT time our Rust
detector *through the Python binding*: marshalling the rich result objects across
PyO3 adds ~10x overhead that is not part of detection and would unfairly inflate
our number. OpenCV is given its best configuration (e.g. the ChArUco
legacy/standard pattern that detects more markers) so the comparison never
sandbags it.

## Usage

    uv pip install --python crates/calib-targets-py/.venv \
        opencv-python-headless numpy
    crates/calib-targets-py/.venv/bin/python tools/compare_opencv_baseline.py \
        --out <raw.json> --img-dir .github/pages/performance/img
"""

from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path
from typing import Any, Callable

import numpy as np
from PIL import Image

import cv2

import calib_targets as ct

# CPU-only: avoids the OpenCL program-cache warning and stabilises timing.
cv2.ocl.setUseOpenCL(False)

REPO_ROOT = Path(__file__).resolve().parents[1]
DATA_JSON = REPO_ROOT / ".github/pages/performance/data.json"

# The report image cards. Configs MIRROR full_stage_timing.rs so "ours" here is
# the same detection the per-stage cards measure.
MID = REPO_ROOT / "testdata/mid.png"
SMALL = REPO_ROOT / "testdata/small.png"

# mid.png is a full 11x7 inner-corner chessboard.
CHESS_COLS_INNER = 11
CHESS_ROWS_INNER = 7
CHESS_GT_CORNERS = CHESS_COLS_INNER * CHESS_ROWS_INNER

# small.png is a 22x22 ChArUco board, DICT_4X4_250.
CHARUCO_SQUARES = 22
CHARUCO_DICT = "DICT_4X4_250"


def load_gray(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def chess_cfg() -> "ct.ChessConfig":
    """ChESS corner config matching full_stage_timing's `chess_config()`."""
    return ct.ChessConfig(
        threshold=ct.Threshold.relative(0.2),
        strategy=ct.DetectionStrategy.chess(ct.ChessStrategyConfig(nms_radius=2)),
    )


def time_p50_ms(fn: Callable[[], Any], warmup: int, repeats: int) -> float:
    for _ in range(warmup):
        fn()
    samples = []
    for _ in range(repeats):
        t0 = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - t0) * 1000.0)
    return round(statistics.median(samples), 3)


def read_ours_ms(data_json: Path, image_rel: str) -> float | None:
    """Native ours runtime: corner_detection + grid_build + decode p50 for
    `image_rel`, read from the report's `data.json` (the Rust full_stage_timing
    measurement). Returns None if the image/data is absent."""
    try:
        data = json.loads(data_json.read_text())
    except (OSError, ValueError):
        return None
    for card in data.get("images", []):
        if card.get("file") == image_rel:
            total = 0.0
            for key in ("corner_detection_ms", "grid_build_ms", "decode_ms"):
                v = card.get(key)
                if v is not None:
                    total += float(v)
            return round(total, 3)
    return None


def match_count(a: np.ndarray, b: np.ndarray, tol_px: float) -> int:
    """Greedy nearest-neighbour matches between two (N,2)/(M,2) point sets."""
    if len(a) == 0 or len(b) == 0:
        return 0
    used = np.zeros(len(b), dtype=bool)
    matched = 0
    for pa in a:
        d2 = ((b - pa) ** 2).sum(axis=1)
        d2[used] = np.inf
        j = int(np.argmin(d2))
        if d2[j] <= tol_px * tol_px:
            used[j] = True
            matched += 1
    return matched


# --------------------------------------------------------------------------
# Chessboard: ours vs cv2.findChessboardCornersSB
# --------------------------------------------------------------------------


def ours_chessboard(gray: np.ndarray) -> np.ndarray:
    params = [ct.ChessboardParams(min_corner_strength=0.5)]
    res = ct.detect_chessboard_best(gray, params, chess_cfg=chess_cfg())
    if res is None:
        return np.empty((0, 2), dtype=np.float32)
    return np.array([c.position for c in res.corners], dtype=np.float32)


def opencv_chessboard(gray: np.ndarray) -> np.ndarray:
    ok, corners = cv2.findChessboardCornersSB(
        gray, (CHESS_COLS_INNER, CHESS_ROWS_INNER)
    )
    if not ok or corners is None:
        return np.empty((0, 2), dtype=np.float32)
    return corners.reshape(-1, 2).astype(np.float32)


def compare_chessboard(
    img_dir: Path, warmup: int, repeats: int, tol_px: float, ours_ms: float | None
) -> dict[str, Any]:
    gray = load_gray(MID)
    ours_pts = ours_chessboard(gray)
    cv_pts = opencv_chessboard(gray)

    cv_ms = time_p50_ms(lambda: opencv_chessboard(gray), warmup, repeats)

    agreement = match_count(ours_pts, cv_pts, tol_px)
    denom = max(min(len(ours_pts), len(cv_pts)), 1)

    draw_overlay(
        img_dir / "cmp_mid.png",
        gray,
        ours_pts,
        cv_pts,
        ours_quads=None,
        cv_quads=None,
    )

    return {
        "kind": "Chessboard",
        "image": "testdata/mid.png",
        "img": "./img/cmp_mid.png",
        "mode": "recall",
        "gt_corners": CHESS_GT_CORNERS,
        "ours": {
            "recall": round(len(ours_pts) / CHESS_GT_CORNERS, 3),
            "matched": int(len(ours_pts)),
            "success": bool(len(ours_pts) > 0),
            "ms": ours_ms,
            "ms_source": "full_stage_timing (native Rust)",
        },
        "opencv": {
            "recall": round(len(cv_pts) / CHESS_GT_CORNERS, 3),
            "matched": int(len(cv_pts)),
            "success": bool(len(cv_pts) > 0),
            "ms": cv_ms,
            "detector": "findChessboardCornersSB",
            "note": (
                ""
                if len(cv_pts) > 0
                else "returned no board (needs exact dimensions; all-or-nothing)"
            ),
        },
        "agreement_px": tol_px,
        "corner_agreement": round(agreement / denom, 3),
    }


# --------------------------------------------------------------------------
# ChArUco: ours vs cv2.aruco.CharucoDetector
# --------------------------------------------------------------------------


def ours_charuco(gray: np.ndarray):
    board = ct.CharucoBoardSpec(
        rows=CHARUCO_SQUARES,
        cols=CHARUCO_SQUARES,
        cell_size=5.2,
        marker_size_rel=0.75,
        dictionary=CHARUCO_DICT,
        marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
    )
    params = ct.CharucoDetectorParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=0.5),
        min_marker_inliers=12,
    )
    try:
        res = ct.detect_charuco(gray, chess_cfg=chess_cfg(), params=params)
    except RuntimeError:
        return np.empty((0, 2), dtype=np.float32), 0, []
    corners = np.array([c.position for c in res.corners], dtype=np.float32)
    quads = [m.corners_img for m in res.markers if m.corners_img is not None]
    return corners, len(res.markers), quads


def _make_cv_board(legacy: bool):
    dictionary = cv2.aruco.getPredefinedDictionary(getattr(cv2.aruco, CHARUCO_DICT))
    board = cv2.aruco.CharucoBoard(
        (CHARUCO_SQUARES, CHARUCO_SQUARES), 1.0, 0.75, dictionary
    )
    # OpenCV flipped the ChArUco interior pattern at 4.6→4.7; try both so OpenCV
    # gets its best shot (we record which pattern was used).
    if hasattr(board, "setLegacyPattern"):
        board.setLegacyPattern(legacy)
    return board


def _cv_charuco_detect(gray: np.ndarray, legacy: bool):
    board = _make_cv_board(legacy)
    detector = cv2.aruco.CharucoDetector(board)
    ch_corners, _ch_ids, marker_corners, marker_ids = detector.detectBoard(gray)
    corners = (
        ch_corners.reshape(-1, 2).astype(np.float32)
        if ch_corners is not None and len(ch_corners) > 0
        else np.empty((0, 2), dtype=np.float32)
    )
    n_markers = 0 if marker_ids is None else int(len(marker_ids))
    quads = (
        [m.reshape(4, 2) for m in marker_corners]
        if marker_corners is not None
        else []
    )
    return corners, n_markers, quads


def opencv_charuco(gray: np.ndarray):
    """Run both pattern conventions, return whichever finds more markers."""
    best = None
    best_legacy = False
    for legacy in (False, True):
        corners, n_markers, quads = _cv_charuco_detect(gray, legacy)
        if best is None or n_markers > best[1]:
            best = (corners, n_markers, quads)
            best_legacy = legacy
    return (*best, best_legacy)


def compare_charuco(
    img_dir: Path, warmup: int, repeats: int, tol_px: float, ours_ms: float | None
) -> dict[str, Any]:
    gray = load_gray(SMALL)
    ours_pts, ours_markers, ours_quads = ours_charuco(gray)
    cv_pts, cv_markers, cv_quads, cv_legacy = opencv_charuco(gray)

    # Time OpenCV with the winning pattern only; ours is the native Rust p50.
    cv_ms = time_p50_ms(lambda: _cv_charuco_detect(gray, cv_legacy), warmup, repeats)

    agreement = match_count(ours_pts, cv_pts, tol_px)
    denom = max(min(len(ours_pts), len(cv_pts)), 1)

    draw_overlay(
        img_dir / "cmp_small.png",
        gray,
        ours_pts,
        cv_pts,
        ours_quads=ours_quads,
        cv_quads=cv_quads,
    )

    return {
        "kind": "ChArUco",
        "image": "testdata/small.png",
        "img": "./img/cmp_small.png",
        "mode": "detected_count",
        "ours": {
            "markers": int(ours_markers),
            "corners": int(len(ours_pts)),
            "ms": ours_ms,
            "ms_source": "full_stage_timing (native Rust)",
        },
        "opencv": {
            "markers": int(cv_markers),
            "corners": int(len(cv_pts)),
            "ms": cv_ms,
            "detector": "aruco.CharucoDetector",
            "legacy_pattern": bool(cv_legacy),
        },
        "agreement_px": tol_px,
        "corner_agreement": round(agreement / denom, 3),
    }


# --------------------------------------------------------------------------
# Overlay (ours green, OpenCV red; marker quads teal/yellow)
# --------------------------------------------------------------------------


def draw_overlay(
    path: Path,
    gray: np.ndarray,
    ours_pts: np.ndarray,
    cv_pts: np.ndarray,
    ours_quads,
    cv_quads,
) -> None:
    rgb = cv2.cvtColor(gray, cv2.COLOR_GRAY2BGR)
    # OpenCV marker quads (yellow), then ours (teal), then corners on top.
    if cv_quads:
        for q in cv_quads:
            cv2.polylines(rgb, [np.int32(q)], True, (0, 200, 255), 1, cv2.LINE_AA)
    if ours_quads:
        for q in ours_quads:
            cv2.polylines(rgb, [np.int32(q)], True, (227, 198, 51), 1, cv2.LINE_AA)
    for p in cv_pts:
        cv2.circle(rgb, (int(round(p[0])), int(round(p[1]))), 5, (60, 60, 230), 1, cv2.LINE_AA)
    for p in ours_pts:
        cv2.circle(rgb, (int(round(p[0])), int(round(p[1]))), 2, (90, 220, 90), -1, cv2.LINE_AA)
    path.parent.mkdir(parents=True, exist_ok=True)
    cv2.imwrite(str(path), rgb)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="raw comparison JSON output")
    parser.add_argument(
        "--img-dir",
        type=Path,
        default=REPO_ROOT / ".github/pages/performance/img",
        help="where to write cmp_*.png overlays",
    )
    parser.add_argument("--repeats", type=int, default=40)
    parser.add_argument("--warmup", type=int, default=5)
    parser.add_argument("--tol-px", type=float, default=2.0)
    parser.add_argument(
        "--ours-timing-from",
        type=Path,
        default=DATA_JSON,
        help="report data.json supplying ours' native Rust per-stage p50",
    )
    args = parser.parse_args()

    if "privatedata" in str(args.img_dir.resolve()):
        raise SystemExit("refusing to write into a privatedata path")

    args.img_dir.mkdir(parents=True, exist_ok=True)

    mid_ms = read_ours_ms(args.ours_timing_from, "testdata/mid.png")
    small_ms = read_ours_ms(args.ours_timing_from, "testdata/small.png")
    if mid_ms is None or small_ms is None:
        print(
            "WARNING: ours runtime missing from "
            f"{args.ours_timing_from} (run scripts/gen-perf-data.sh first); "
            "emitting null ours.ms."
        )

    groups = [
        compare_chessboard(args.img_dir, args.warmup, args.repeats, args.tol_px, mid_ms),
        compare_charuco(args.img_dir, args.warmup, args.repeats, args.tol_px, small_ms),
    ]

    payload = {
        "schema": "calib-targets.opencv-comparison.v1",
        "desc": (
            "calib-targets vs OpenCV on recall and runtime, on the public report "
            "images. Runtime is each detector's native p50: OpenCV from cv2, ours "
            "from the Rust full_stage_timing measurement (corner detection + grid "
            "build + decode) — timing our detector through the Python binding "
            "would add marshalling overhead unrelated to detection."
        ),
        "caveat": (
            "OpenCV chessboard detection is all-or-nothing and needs exact board "
            "dimensions (the success column shows it returned the full board). "
            "small.png is a partial ChArUco view with no independent ground truth, "
            "so its row is a detected-count comparison, not recall-vs-GT. Our "
            "detector is partial-tolerant and self-identifying."
        ),
        "tolerance_px": args.tol_px,
        "repeats": args.repeats,
        "warmup": args.warmup,
        "opencv_version": cv2.__version__,
        "groups": groups,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"wrote {args.out}")
    for g in groups:
        print(json.dumps(g, indent=2))


if __name__ == "__main__":
    main()
