import numpy as np
import pytest

import calib_targets


def _image() -> np.ndarray:
    return np.zeros((32, 32), dtype=np.uint8)


def test_detect_chessboard_defaults() -> None:
    result = calib_targets.detect_chessboard(_image())
    assert result is None or isinstance(result, dict)


def test_detect_chessboard_partial_overrides() -> None:
    result = calib_targets.detect_chessboard(_image(), params={"min_corners": 16})
    assert result is None or isinstance(result, dict)


def test_detect_chessboard_typed_params() -> None:
    params = calib_targets.ChessboardParams(min_corners=16)
    result = calib_targets.detect_chessboard(_image(), params=params)
    assert result is None or isinstance(result, dict)


def test_detect_chessboard_unknown_keys() -> None:
    with pytest.raises(ValueError) as excinfo:
        calib_targets.detect_chessboard(_image(), params={"min_cornerz": 1})
    message = str(excinfo.value)
    assert "unknown keys" in message
    assert "min_corner_strength" in message


def test_detect_chessboard_typed_chess_cfg() -> None:
    chess_cfg = calib_targets.ChessConfig(
        params=calib_targets.ChessCornerParams(use_radius10=True)
    )
    result = calib_targets.detect_chessboard(_image(), chess_cfg=chess_cfg)
    assert result is None or isinstance(result, dict)
