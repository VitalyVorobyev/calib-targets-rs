import numpy as np
import pytest

import calib_targets


def _image() -> np.ndarray:
    return np.zeros((32, 32), dtype=np.uint8)


def test_detect_chessboard_typed_params() -> None:
    # v2 `ChessboardParams` has no `min_corners` field; `min_labeled_corners`
    # is the output floor and `min_corner_strength` is the Stage-1 pre-filter.
    params = calib_targets.ChessboardParams(min_corner_strength=0.1)
    result = calib_targets.detect_chessboard(_image(), params=params)
    assert result is None or isinstance(result, calib_targets.ChessboardDetectionResult)


def test_detect_charuco_typed_params() -> None:
    board = calib_targets.CharucoBoardSpec(
        rows=3,
        cols=3,
        cell_size=1.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_50",
        marker_layout=calib_targets.MarkerLayout.OPENCV_CHARUCO,
    )
    params = calib_targets.CharucoDetectorParams(board=board)
    try:
        result = calib_targets.detect_charuco(_image(), params=params)
    except RuntimeError:
        result = None
    assert result is None or isinstance(result, calib_targets.CharucoDetectionResult)


def test_detect_marker_board_typed_layout() -> None:
    layout = calib_targets.MarkerBoardLayout(
        rows=6,
        cols=8,
        circles=(
            calib_targets.MarkerCircleSpec(i=2, j=2, polarity=calib_targets.CirclePolarity.WHITE),
            calib_targets.MarkerCircleSpec(i=3, j=2, polarity=calib_targets.CirclePolarity.BLACK),
            calib_targets.MarkerCircleSpec(i=2, j=3, polarity=calib_targets.CirclePolarity.WHITE),
        ),
    )
    params = calib_targets.MarkerBoardParams(layout=layout)
    result = calib_targets.detect_marker_board(_image(), params=params)
    assert result is None or isinstance(result, calib_targets.MarkerBoardDetectionResult)


def test_detect_puzzleboard_typed_params() -> None:
    board = calib_targets.PuzzleBoardSpec(rows=10, cols=10, cell_size=1.0)
    params = calib_targets.PuzzleBoardParams.for_board(board)
    try:
        result = calib_targets.detect_puzzleboard(_image(), params=params)
    except RuntimeError:
        result = None
    assert result is None or isinstance(result, calib_targets.PuzzleBoardDetectionResult)


def test_dict_inputs_are_rejected() -> None:
    with pytest.raises(TypeError):
        calib_targets.detect_chessboard(_image(), params={"min_corners": 16})  # type: ignore[arg-type]

    board = calib_targets.CharucoBoardSpec(
        rows=3,
        cols=3,
        cell_size=1.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_50",
        marker_layout=calib_targets.MarkerLayout.OPENCV_CHARUCO,
    )
    params = calib_targets.CharucoDetectorParams(board=board)
    with pytest.raises(TypeError):
        calib_targets.detect_charuco(_image(), chess_cfg={"threshold_value": 0.1}, params=params)  # type: ignore[arg-type]

    puzzle_params = calib_targets.PuzzleBoardParams.for_board(
        calib_targets.PuzzleBoardSpec(rows=10, cols=10, cell_size=1.0)
    )
    with pytest.raises(TypeError):
        calib_targets.detect_puzzleboard(_image(), params=puzzle_params, chess_cfg={"threshold_value": 0.1})  # type: ignore[arg-type]


def test_chess_config_roundtrip() -> None:
    cfg = calib_targets.ChessConfig(
        threshold_value=0.3,
        pyramid_levels=2,
        pyramid_min_size=64,
        refiner=calib_targets.RefinerConfig(kind="forstner"),
    )
    serialized = cfg.to_dict()
    restored = calib_targets.ChessConfig.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_chessboard_params_roundtrip() -> None:
    # Exercise a couple of v2-specific fields to confirm the round-trip
    # covers the flat DetectorParams shape.
    params = calib_targets.ChessboardParams(
        min_corner_strength=0.25,
        cluster_tol_deg=10.0,
        max_validation_iters=5,
    )
    serialized = params.to_dict()
    restored = calib_targets.ChessboardParams.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_puzzleboard_params_roundtrip() -> None:
    params = calib_targets.PuzzleBoardParams.for_board(
        calib_targets.PuzzleBoardSpec(rows=12, cols=13, cell_size=2.5, origin_row=4, origin_col=7)
    )
    params.decode.max_bit_error_rate = 0.25
    serialized = params.to_dict()
    restored = calib_targets.PuzzleBoardParams.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_puzzleboard_printing_roundtrip() -> None:
    doc = calib_targets.PrintableTargetDocument(
        target=calib_targets.PuzzleBoardTargetSpec(
            rows=10,
            cols=10,
            square_size_mm=12.0,
            origin_row=1,
            origin_col=2,
        )
    )
    restored = calib_targets.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def _sample_chessboard_result() -> dict:
    # Schema matches `serde_json::to_value(
    # calib_targets_chessboard::Detection)` byte-for-byte.
    return {
        "grid_directions": [0.1, 1.6],
        "cell_size": 25.0,
        "target": {
            "kind": "chessboard",
            "corners": [
                {
                    "position": [10.0, 20.0],
                    "grid": {"i": 0, "j": 1},
                    "id": None,
                    "target_position": None,
                    "score": 0.9,
                }
            ],
        },
        "strong_indices": [0],
    }


def _sample_charuco_result() -> dict:
    return {
        "detection": {
            "kind": "charuco",
            "corners": [
                {
                    "position": [10.0, 20.0],
                    "grid": {"i": 0, "j": 1},
                    "id": 4,
                    "target_position": [1.0, 2.0],
                    "score": 0.95,
                }
            ],
        },
        "markers": [
            {
                "id": 1,
                "gc": {"i": 2, "j": 3},
                "rotation": 0,
                "hamming": 0,
                "score": 1.0,
                "border_score": 0.99,
                "code": 1234,
                "inverted": False,
                "corners_rect": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                "corners_img": [[2.0, 2.0], [3.0, 2.0], [3.0, 3.0], [2.0, 3.0]],
            }
        ],
        "alignment": {
            "transform": {"a": 1, "b": 0, "c": 0, "d": 1},
            "translation": [0, 0],
        },
    }


def _sample_marker_board_result() -> dict:
    return {
        "detection": {
            "kind": "checkerboard_marker",
            "corners": [
                {
                    "position": [10.0, 20.0],
                    "grid": {"i": 0, "j": 1},
                    "id": None,
                    "target_position": None,
                    "score": 0.9,
                }
            ],
        },
        "inliers": [0],
        "circle_candidates": [
            {
                "center_img": [11.0, 21.0],
                "cell": {"i": 2, "j": 3},
                "polarity": "white",
                "score": 4.0,
                "contrast": 12.0,
            }
        ],
        "circle_matches": [
            {
                "expected": {"cell": {"i": 2, "j": 3}, "polarity": "white"},
                "matched_index": 0,
                "distance_cells": 0.1,
                "offset_cells": {"di": 0, "dj": 1},
            }
        ],
        "alignment": {
            "transform": {"a": 1, "b": 0, "c": 0, "d": 1},
            "translation": [1, 2],
        },
        "alignment_inliers": 1,
    }


def _sample_puzzleboard_result() -> dict:
    return {
        "detection": {
            "kind": "puzzle_board",
            "corners": [
                {
                    "position": [10.0, 20.0],
                    "grid": {"i": 4, "j": 5},
                    "id": 2509,
                    "target_position": [4.0, 5.0],
                    "score": 0.9,
                }
            ],
        },
        "alignment": {
            "transform": {"a": 1, "b": 0, "c": 0, "d": 1},
            "translation": [4, 5],
        },
        "decode": {
            "edges_observed": 24,
            "edges_matched": 24,
            "mean_confidence": 0.95,
            "bit_error_rate": 0.0,
            "master_origin_row": 5,
            "master_origin_col": 4,
        },
        "observed_edges": [
            {
                "row": 1,
                "col": 2,
                "orientation": "horizontal",
                "bit": 1,
                "confidence": 0.8,
            }
        ],
    }


def test_result_roundtrip() -> None:
    chess_raw = _sample_chessboard_result()
    assert calib_targets.ChessboardDetectionResult.from_dict(chess_raw).to_dict() == chess_raw

    charuco_raw = _sample_charuco_result()
    assert calib_targets.CharucoDetectionResult.from_dict(charuco_raw).to_dict() == charuco_raw

    marker_raw = _sample_marker_board_result()
    assert (
        calib_targets.MarkerBoardDetectionResult.from_dict(marker_raw).to_dict()
        == marker_raw
    )

    puzzle_raw = _sample_puzzleboard_result()
    assert (
        calib_targets.PuzzleBoardDetectionResult.from_dict(puzzle_raw).to_dict()
        == puzzle_raw
    )


def test_result_from_dict_rejects_unknown_keys() -> None:
    bad = _sample_chessboard_result()
    bad["unknown"] = 123
    with pytest.raises(ValueError):
        calib_targets.ChessboardDetectionResult.from_dict(bad)
