import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest

import calib_targets


def _image() -> np.ndarray:
    return np.zeros((32, 32), dtype=np.uint8)


def test_detect_chessboard_typed_params() -> None:
    # `ChessboardParams` has no `min_corners` field; `min_labeled_corners`
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
        calib_targets.detect_charuco(_image(), chess_cfg={"threshold": {"absolute": 15.0}}, params=params)  # type: ignore[arg-type]

    puzzle_params = calib_targets.PuzzleBoardParams.for_board(
        calib_targets.PuzzleBoardSpec(rows=10, cols=10, cell_size=1.0)
    )
    with pytest.raises(TypeError):
        calib_targets.detect_puzzleboard(_image(), params=puzzle_params, chess_cfg={"threshold": {"absolute": 15.0}})  # type: ignore[arg-type]


def test_chess_config_default_matches_rust_shape() -> None:
    # The default ``ChessConfig()`` must produce the exact wire shape
    # that ``calib_targets::detect::default_chess_config()`` emits via
    # ``serde_json``. Keep this hardcoded — if the Rust side renames a
    # field or swaps a tagged variant, this test fails loudly.
    cfg = calib_targets.ChessConfig()
    expected = {
        "strategy": {
            "chess": {
                "ring": "canonical",
                "descriptor_ring": "follow_detector",
                "nms_radius": 2,
                "min_cluster_size": 2,
                "refiner": {"center_of_mass": {"radius": 2}},
            }
        },
        "threshold": {"absolute": 15.0},
        "multiscale": "single_scale",
        "upscale": "disabled",
        "orientation_method": "ring_fit",
        "merge_radius": 3.0,
    }
    assert cfg.to_dict() == expected
    assert calib_targets.ChessConfig.from_dict(expected).to_dict() == expected


def test_chess_config_threshold_constructors() -> None:
    abs_cfg = calib_targets.ChessConfig(
        threshold=calib_targets.Threshold.absolute(8.0)
    )
    assert abs_cfg.to_dict()["threshold"] == {"absolute": 8.0}

    rel_cfg = calib_targets.ChessConfig(
        threshold=calib_targets.Threshold.relative(0.15)
    )
    assert rel_cfg.to_dict()["threshold"] == {"relative": 0.15}

    # Round-trip via dict preserves both threshold variants.
    for cfg in (abs_cfg, rel_cfg):
        restored = calib_targets.ChessConfig.from_dict(cfg.to_dict())
        assert restored.to_dict() == cfg.to_dict()


def test_chess_config_tagged_subtrees() -> None:
    cfg = calib_targets.ChessConfig(
        threshold=calib_targets.Threshold.absolute(10.0),
        multiscale=calib_targets.MultiscaleConfig.pyramid(levels=2, min_size=64),
        upscale=calib_targets.UpscaleConfig.fixed(2),
        orientation_method=calib_targets.OrientationMethod.DISK_FIT,
    )
    serialized = cfg.to_dict()
    assert serialized["multiscale"] == {
        "pyramid": {"levels": 2, "min_size": 64, "refinement_radius": 3}
    }
    assert serialized["upscale"] == {"fixed": 2}
    assert serialized["orientation_method"] == "disk_fit"
    restored = calib_targets.ChessConfig.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_chess_config_refiner_round_trip() -> None:
    forstner = calib_targets.ChessConfig(
        strategy=calib_targets.DetectionStrategy.chess(
            calib_targets.ChessStrategyConfig(
                refiner=calib_targets.ChessRefiner.forstner(
                    calib_targets.ForstnerConfig(radius=3, min_trace=20.0)
                )
            )
        )
    )
    serialized = forstner.to_dict()
    assert serialized["strategy"]["chess"]["refiner"] == {
        "forstner": {
            "radius": 3,
            "min_trace": 20.0,
            "min_det": 1e-3,
            "max_condition_number": 50.0,
            "max_offset": 1.5,
        }
    }
    restored = calib_targets.ChessConfig.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_chess_config_legacy_refiner_shim() -> None:
    # The deprecated ``RefinerConfig`` shim returns a ``ChessRefiner``;
    # callers from before chess-corners 0.10 keep working.
    legacy = calib_targets.RefinerConfig(kind="forstner")
    assert isinstance(legacy, calib_targets.ChessRefiner)
    assert legacy.to_dict() == {"forstner": calib_targets.ForstnerConfig().to_dict()}


def test_chess_config_rejects_old_flat_shape() -> None:
    bad = {"threshold_mode": "absolute", "threshold_value": 15.0}
    with pytest.raises(ValueError, match="pre-0.10 flat shape"):
        calib_targets.ChessConfig.from_dict(bad)


def test_chessboard_params_roundtrip() -> None:
    # Exercise a couple of chessboard-specific fields to confirm the round-trip
    # covers the flat DetectorParams shape.
    params = calib_targets.ChessboardParams(
        min_corner_strength=0.25,
        cluster_tol_deg=10.0,
        max_booster_iters=5,
        topological=calib_targets.TopologicalParams(axis_align_tol_rad=0.30),
    )
    serialized = params.to_dict()
    restored = calib_targets.ChessboardParams.from_dict(serialized)
    assert restored.to_dict() == serialized


def test_chessboard_params_graph_build_algorithm() -> None:
    # Topological is the only builder (seed-and-grow retired); it is the default.
    default = calib_targets.ChessboardParams()
    assert default.graph_build_algorithm == "topological"
    assert default.to_dict()["graph_build_algorithm"] == "topological"

    # Setting it explicitly round-trips.
    topo = calib_targets.ChessboardParams(graph_build_algorithm="topological")
    assert topo.to_dict()["graph_build_algorithm"] == "topological"

    # Round-trip preserves the selector exactly.
    restored = calib_targets.ChessboardParams.from_dict(topo.to_dict())
    assert restored.graph_build_algorithm == "topological"

    preset = calib_targets.ChessboardParams.for_topological(min_labeled_corners=16)
    assert preset.graph_build_algorithm == "topological"
    assert preset.min_labeled_corners == 16
    assert preset.to_dict()["graph_build_algorithm"] == "topological"


def test_topological_trace_wrapper_shape() -> None:
    params = calib_targets.ChessboardParams(graph_build_algorithm="topological")
    payload = calib_targets.trace_chessboard_topological(_image(), params=params)
    assert payload["schema"] == 1
    assert payload["graph_build_algorithm"] == "topological"
    assert payload["image"] == {"width": 32, "height": 32}
    assert isinstance(payload["corners"], list)
    assert "trace" in payload
    assert "detections" in payload


def test_topo_grid_regression_evaluator_smoke(tmp_path: Path) -> None:
    repo = Path(__file__).resolve().parents[3]
    image = repo / "testdata/02-topo-grid/GeminiChess3.png"
    if not image.exists():
        pytest.skip("topological regression fixture is not available")

    script = repo / "scripts/evaluate_topo_grid_regression.py"
    subprocess.run(
        [
            sys.executable,
            str(script),
            "--image",
            image.name,
            "--algorithm",
            "topological",
            "--repeats",
            "1",
            "--warmup",
            "0",
            "--output-dir",
            str(tmp_path),
        ],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    report_path = tmp_path / "report.json"
    payload = json.loads(report_path.read_text())
    assert payload["runs"]
    assert payload["runs"][0]["labelled_count"] >= 42


def test_puzzleboard_params_roundtrip() -> None:
    params = calib_targets.PuzzleBoardParams.for_board(
        calib_targets.PuzzleBoardSpec(rows=12, cols=13, cell_size=2.5, origin_row=4, origin_col=7)
    )
    params.decode.search_mode = calib_targets.PuzzleBoardSearchMode.fixed_board()
    params.decode.scoring_mode = calib_targets.PuzzleBoardScoringMode.soft_log_likelihood()
    params.decode.max_bit_error_rate = 0.25
    params.decode.bit_likelihood_slope = 15.0
    params.decode.per_bit_floor = -5.0
    params.decode.alignment_min_margin = 0.05
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
    # calib_targets_chessboard::ChessboardDetection)` byte-for-byte.
    # `cell_size` is serialized unconditionally on the Rust side.
    return {
        "corners": [
            {
                "position": [10.0, 20.0],
                "grid": {"i": 0, "j": 1},
                "input_index": 0,
                "score": 0.9,
            }
        ],
        "cell_size": 41.5,
    }


def _sample_charuco_result() -> dict:
    return {
        "corners": [
            {
                "position": [10.0, 20.0],
                "grid": {"i": 0, "j": 1},
                "id": 4,
                "target_position": [1.0, 2.0],
                "score": 0.95,
            }
        ],
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
        "corners": [
            {
                "position": [10.0, 20.0],
                "grid": {"i": 0, "j": 1},
                "id": None,
                "target_position": None,
                "score": 0.9,
            }
        ],
        "alignment": {
            "transform": {"a": 1, "b": 0, "c": 0, "d": 1},
            "translation": [1, 2],
        },
    }


def _sample_puzzleboard_result() -> dict:
    return {
        "corners": [
            {
                "position": [10.0, 20.0],
                "grid": {"i": 4, "j": 5},
                "id": 2509,
                "target_position": [4.0, 5.0],
                "score": 0.9,
            }
        ],
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
