"""End-to-end round-trip tests for detection result dicts.

These tests run the Rust extension on real test images and verify that
the resulting dict deserializes cleanly through Python's result wrappers
(``*.from_dict``) and survives a ``to_dict`` → ``from_dict`` round-trip.

They guard against key-name drift between Rust's ``serde_json::to_value``
output and the Python ``_convert_out`` deserializers — the class of bug
that hand-written fixtures cannot catch.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np
import pytest
from PIL import Image

import calib_targets as ct
from calib_targets import _core


REPO_ROOT = Path(__file__).resolve().parents[3]
TESTDATA = REPO_ROOT / "testdata"


def _load_gray(name: str) -> np.ndarray:
    path = TESTDATA / name
    if not path.exists():
        pytest.skip(f"test image not found: {path}")
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def _assert_roundtrip(result: Any) -> None:
    serialized = result.to_dict()
    restored = type(result).from_dict(serialized)
    assert restored.to_dict() == serialized


def _charuco_params_small2() -> ct.CharucoParams:
    return ct.CharucoParams(
        board=ct.CharucoBoardSpec(
            rows=22,
            cols=22,
            cell_size=5.2,
            marker_size_rel=0.75,
            dictionary="DICT_4X4_1000",
            marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
        ),
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=0.2),
        scan=ct.ScanDecodeConfig(
            border_bits=1,
            inset_frac=0.06,
            marker_size_rel=0.75,
            min_border_score=0.45,
            dedup_by_id=True,
        ),
        max_hamming=0,
        min_marker_inliers=8,
    )


def _marker_board_params() -> ct.MarkerBoardParams:
    return ct.MarkerBoardParams(
        layout=ct.MarkerBoardSpec(
            rows=22,
            cols=22,
            circles=(
                ct.MarkerCircleSpec(i=11, j=11, polarity=ct.CirclePolarity.WHITE),
                ct.MarkerCircleSpec(i=12, j=11, polarity=ct.CirclePolarity.BLACK),
                ct.MarkerCircleSpec(i=12, j=12, polarity=ct.CirclePolarity.WHITE),
            ),
        ),
        chessboard=ct.ChessboardParams(min_corner_strength=0.2),
        circle_score=ct.CircleScoreParams(
            patch_size=64,
            diameter_frac=0.5,
            ring_thickness_frac=0.35,
            ring_radius_mul=1.6,
            min_contrast=60.0,
            samples=48,
            center_search_px=2,
        ),
        match_params=ct.CircleMatchParams(
            max_candidates_per_polarity=3,
            min_offset_inliers=1,
        ),
    )


def _puzzleboard_params() -> ct.PuzzleBoardParams:
    params = ct.PuzzleBoardParams.for_board(
        ct.PuzzleBoardSpec(rows=10, cols=10, cell_size=12.0, origin_row=0, origin_col=0)
    )
    params.chessboard.chess.threshold_value = 0.15
    params.chessboard.chess.nms_radius = 3
    return params


# ---------------------------------------------------------------------------
# chessboard
# ---------------------------------------------------------------------------


def test_detect_chessboard_roundtrip() -> None:
    image = _load_gray("mid.png")
    result = ct.detect_chessboard(image)
    if result is None:
        pytest.skip("no chessboard detected on testdata/mid.png")
    assert len(result.detection.corners) > 0
    _assert_roundtrip(result)


def test_detect_chessboard_best_roundtrip() -> None:
    image = _load_gray("mid.png")
    configs = [
        ct.ChessboardParams(),
        ct.ChessboardParams(min_corner_strength=0.1),
    ]
    result = ct.detect_chessboard_best(image, configs)
    if result is None:
        pytest.skip("no chessboard detected on testdata/mid.png")
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# chessboard debug frame (Phase A instrumentation)
# ---------------------------------------------------------------------------


def _assert_debug_frame_shape(frame: dict[str, Any]) -> None:
    """Structural check for the v2 `DebugFrame` payload.

    Asserts the top-level keys Rust's `calib_targets_chessboard::
    DebugFrame` serializes so the Python overlay script stays in
    sync. If Rust ever renames a field, this test flags it before
    the overlay breaks silently.
    """
    expected_top = {
        "schema",
        "input_count",
        "grid_directions",
        "cell_size",
        "seed",
        "iterations",
        "boosters",
        "detection",
        "corners",
    }
    assert expected_top.issubset(frame.keys()), (
        f"missing top-level keys: {expected_top - frame.keys()}"
    )
    assert isinstance(frame["schema"], int) and frame["schema"] >= 1
    assert isinstance(frame["input_count"], int) and frame["input_count"] >= 0
    # `corners` mirrors the input corner array, with per-corner stage.
    assert len(frame["corners"]) == frame["input_count"]
    if frame["input_count"] > 0:
        c0 = frame["corners"][0]
        expected_corner_keys = {
            "input_index",
            "position",
            "axes",
            "strength",
            "contrast",
            "fit_rms",
            "stage",
            "label",
        }
        assert expected_corner_keys.issubset(c0.keys()), (
            f"corner missing keys: {expected_corner_keys - c0.keys()}"
        )
        assert len(c0["axes"]) == 2
        for axis in c0["axes"]:
            assert {"angle", "sigma"}.issubset(axis.keys())


def test_detect_chessboard_debug_success_path() -> None:
    """Debug frame on an image that normally detects: must have
    a labelled `detection` block plus a populated iteration trace."""
    image = _load_gray("mid.png")
    frame = ct.detect_chessboard_debug(image)
    _assert_debug_frame_shape(frame)
    assert frame["input_count"] > 0
    if frame["detection"] is None:
        pytest.skip("no chessboard detected on testdata/mid.png")
    assert frame["cell_size"] is not None
    assert frame["grid_directions"] is not None


def test_detect_chessboard_debug_failure_path_still_emits_counts() -> None:
    """Debug frame on a failure path: no detection, but `input_count` is
    populated and the caller can see which stage each corner reached via
    `corners[*].stage`. Uses `min_corner_strength` well above typical
    ChESS responses to drop every corner before clustering."""
    image = _load_gray("mid.png")
    params = ct.ChessboardParams(min_corner_strength=1e9)  # unreachable
    frame = ct.detect_chessboard_debug(image, params=params)
    _assert_debug_frame_shape(frame)
    assert frame["detection"] is None
    # Every corner should stay at `Raw` because the strength filter
    # drops every input.
    n_raw = sum(1 for c in frame["corners"] if c["stage"] == "Raw")
    assert n_raw == frame["input_count"]


# ---------------------------------------------------------------------------
# charuco — direct regression test for the MarkerDetection.gc bug
# ---------------------------------------------------------------------------


def test_detect_charuco_roundtrip_exercises_marker_gc() -> None:
    """Regression guard: MarkerDetection.gc must deserialize from Rust's
    native ``{"i": ..., "j": ...}`` dict output. Previously Python expected
    ``{"gx", "gy"}`` and every charuco detection with markers blew up."""
    image = _load_gray("small2.png")
    params = _charuco_params_small2()

    result = ct.detect_charuco(image, params=params)
    assert isinstance(result, ct.CharucoDetectionResult)
    assert len(result.markers) > 0, (
        "charuco detection produced zero markers — the MarkerDetection.gc "
        "deserialization path is not being exercised by this test"
    )

    first_marker_dict = result.markers[0].to_dict()
    assert set(first_marker_dict["gc"].keys()) == {"i", "j"}

    _assert_roundtrip(result)


def test_detect_charuco_best_roundtrip() -> None:
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    result = ct.detect_charuco_best(image, [params])
    assert isinstance(result, ct.CharucoDetectionResult)
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# marker board
# ---------------------------------------------------------------------------


def test_detect_marker_board_roundtrip() -> None:
    image = _load_gray("markerboard_crop.png")
    params = _marker_board_params()
    result = ct.detect_marker_board(image, params=params)
    if result is None:
        pytest.skip("no marker board detected on testdata/markerboard_crop.png")
    _assert_roundtrip(result)


def test_detect_marker_board_best_roundtrip() -> None:
    image = _load_gray("markerboard_crop.png")
    params = _marker_board_params()
    result = ct.detect_marker_board_best(image, [params])
    if result is None:
        pytest.skip("no marker board detected on testdata/markerboard_crop.png")
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# puzzleboard
# ---------------------------------------------------------------------------


def test_detect_puzzleboard_roundtrip() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    result = ct.detect_puzzleboard(image, params=params)
    assert isinstance(result, ct.PuzzleBoardDetectionResult)
    assert len(result.detection.corners) > 0
    assert result.decode.edges_observed > 0
    _assert_roundtrip(result)


def test_detect_puzzleboard_best_roundtrip() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    result = ct.detect_puzzleboard_best(image, [params])
    assert isinstance(result, ct.PuzzleBoardDetectionResult)
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# low-level key-shape assertions on the raw Rust dict
# ---------------------------------------------------------------------------


def test_raw_charuco_dict_keys_match_python_schema() -> None:
    """Inspect the raw dict emitted by ``_core.detect_charuco`` (before
    ``from_dict`` runs) and check that every grid-coord-shaped field uses
    ``{"i", "j"}`` keys — the shape Rust emits for ``GridCoords``. If a new
    Rust struct ever switches to a different key convention, this test
    flags it before the Python wrapper crashes at runtime."""
    from calib_targets._convert_in import (
        charuco_detector_params_to_payload,
        chess_config_to_payload,
    )

    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    raw = _core.detect_charuco(
        image,
        chess_cfg=chess_config_to_payload(None),
        params=charuco_detector_params_to_payload(params),
    )

    assert isinstance(raw, dict)
    markers = raw["markers"]
    assert len(markers) > 0, "need at least one marker to check gc keys"
    for m in markers:
        assert set(m["gc"].keys()) == {"i", "j"}, (
            f"MarkerDetection.gc has unexpected keys: {set(m['gc'].keys())}"
        )

    corners = raw["detection"]["corners"]
    for c in corners:
        grid = c["grid"]
        if grid is not None:
            assert set(grid.keys()) == {"i", "j"}, (
                f"LabeledCorner.grid has unexpected keys: {set(grid.keys())}"
            )


def test_raw_puzzleboard_dict_keys_match_python_schema() -> None:
    from calib_targets._convert_in import chess_config_to_payload, puzzleboard_params_to_payload

    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    raw = _core.detect_puzzleboard(
        image,
        chess_cfg=chess_config_to_payload(None),
        params=puzzleboard_params_to_payload(params),
    )

    assert isinstance(raw, dict)
    assert raw["detection"]["kind"] == "puzzle_board"
    assert set(raw["decode"].keys()) == {
        "edges_observed",
        "edges_matched",
        "mean_confidence",
        "bit_error_rate",
        "master_origin_row",
        "master_origin_col",
    }
    for edge in raw["observed_edges"]:
        assert set(edge.keys()) == {"row", "col", "orientation", "bit", "confidence"}

    for corner in raw["detection"]["corners"]:
        grid = corner["grid"]
        if grid is not None:
            assert set(grid.keys()) == {"i", "j"}
