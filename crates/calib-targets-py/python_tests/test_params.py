import numpy as np
import pytest

import calib_targets


def _image() -> np.ndarray:
    return np.zeros((32, 32), dtype=np.uint8)


def test_detect_chessboard_typed_params() -> None:
    params = calib_targets.ChessboardParams(min_corners=16)
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


def test_legacy_mapping_inputs_are_rejected() -> None:
    with pytest.raises(TypeError):
        calib_targets.detect_chessboard(_image(), params={"min_corners": 16})

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
        calib_targets.detect_charuco(_image(), chess_cfg={"params": {}}, params=params)


def test_config_roundtrip() -> None:
    cfg = calib_targets.ChessConfig(
        params=calib_targets.ChessCornerParams(threshold_rel=0.2),
        multiscale=calib_targets.CoarseToFineParams(
            pyramid=calib_targets.PyramidParams(num_levels=2, min_size=64),
            refinement_radius=3,
            merge_radius=3.0,
        ),
    )
    serialized = cfg.to_dict()
    restored = calib_targets.ChessConfig.from_dict(serialized)
    assert restored.to_dict() == serialized


def _sample_chessboard_result() -> dict:
    return {
        "detection": {
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
        "inliers": [0],
        "orientations": [0.1, 1.6],
        "debug": {
            "orientation_histogram": {"bin_centers": [0.1], "values": [2.0]},
            "graph": {
                "nodes": [
                    {
                        "position": [10.0, 20.0],
                        "neighbors": [{"index": 0, "direction": "x", "distance": 1.0}],
                    }
                ]
            },
        },
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
                "gc": {"gx": 2, "gy": 3},
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


def test_result_from_dict_rejects_unknown_keys() -> None:
    bad = _sample_chessboard_result()
    bad["unknown"] = 123
    with pytest.raises(ValueError):
        calib_targets.ChessboardDetectionResult.from_dict(bad)
