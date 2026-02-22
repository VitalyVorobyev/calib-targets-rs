from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import numpy as np
import numpy.typing as npt

from . import _core
from ._convert_in import (
    charuco_detector_params_to_payload,
    chess_config_to_payload,
    chessboard_params_to_payload,
    marker_board_params_to_payload,
)
from .config import CharucoDetectorParams, ChessConfig, ChessboardParams, MarkerBoardParams
from .results import (
    CharucoDetectionResult,
    ChessboardDetectionResult,
    MarkerBoardDetectionResult,
)


def _type_error(name: str, expected: str) -> TypeError:
    return TypeError(
        f"{name} must be {expected}. Mapping inputs were removed; use typed dataclasses."
    )


def _ensure_typed_or_none(name: str, value: Any, typ: type[Any]) -> None:
    if value is None:
        return
    if isinstance(value, Mapping):
        raise _type_error(name, f"{typ.__name__} | None")
    if not isinstance(value, typ):
        raise _type_error(name, f"{typ.__name__} | None")


def _ensure_typed(name: str, value: Any, typ: type[Any]) -> None:
    if isinstance(value, Mapping):
        raise _type_error(name, typ.__name__)
    if not isinstance(value, typ):
        raise _type_error(name, typ.__name__)


def detect_chessboard(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: ChessboardParams | None = None,
) -> ChessboardDetectionResult | None:
    _ensure_typed_or_none("chess_cfg", chess_cfg, ChessConfig)
    _ensure_typed_or_none("params", params, ChessboardParams)

    raw = _core.detect_chessboard(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=chessboard_params_to_payload(params),
    )
    if raw is None:
        return None
    return ChessboardDetectionResult.from_dict(raw)


def detect_charuco(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: CharucoDetectorParams,
) -> CharucoDetectionResult:
    _ensure_typed_or_none("chess_cfg", chess_cfg, ChessConfig)
    _ensure_typed("params", params, CharucoDetectorParams)

    raw = _core.detect_charuco(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=charuco_detector_params_to_payload(params),
    )
    return CharucoDetectionResult.from_dict(raw)


def detect_marker_board(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: MarkerBoardParams | None = None,
) -> MarkerBoardDetectionResult | None:
    _ensure_typed_or_none("chess_cfg", chess_cfg, ChessConfig)
    _ensure_typed_or_none("params", params, MarkerBoardParams)

    raw = _core.detect_marker_board(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=marker_board_params_to_payload(params),
    )
    if raw is None:
        return None
    return MarkerBoardDetectionResult.from_dict(raw)


__all__ = [
    "detect_chessboard",
    "detect_charuco",
    "detect_marker_board",
]
