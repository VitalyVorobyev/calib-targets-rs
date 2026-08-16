from __future__ import annotations

from typing import Any

import numpy as np
import numpy.typing as npt

from . import _core
from ._convert_in import (
    charuco_detector_params_to_payload,
    chess_config_to_payload,
    chessboard_params_to_payload,
    marker_board_params_to_payload,
    puzzleboard_params_to_payload,
)
from .config import (
    CharucoDetectorParams,
    ChessConfig,
    ChessboardParams,
    MarkerBoardParams,
    PuzzleBoardParams,
)
from .results import (
    CharucoDetection,
    ChessboardDetectionResult,
    MarkerBoardDetection,
    PuzzleBoardDetection,
)


def _check_type(name: str, value: Any, typ: type[Any]) -> None:
    if not isinstance(value, typ):
        raise TypeError(f"{name} must be {typ.__name__}, got {type(value).__name__}")


def detect_chessboard(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: ChessboardParams | None = None,
) -> ChessboardDetectionResult | None:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, ChessboardParams)

    raw = _core.detect_chessboard(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=chessboard_params_to_payload(params),
    )
    if raw is None:
        return None
    return ChessboardDetectionResult.from_dict(raw)


def detect_chessboard_all(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: ChessboardParams | None = None,
) -> list[ChessboardDetectionResult]:
    """Detect all chessboard components in a grayscale image.

    Like :func:`detect_chessboard` but returns every same-board component the
    detector recovers (up to ``params.max_components``), rather than just the
    first one.  Useful when the board is partially occluded and multiple
    disjoint patches are visible.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, ChessboardParams)

    raw = _core.detect_chessboard_all(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=chessboard_params_to_payload(params),
    )
    return [ChessboardDetectionResult.from_dict(item) for item in raw]


def trace_chessboard_topological(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: ChessboardParams | None = None,
) -> dict[str, Any]:
    """Return the Rust-backed topological trace used by blog/debug overlays."""
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, ChessboardParams)

    return _core.trace_chessboard_topological(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=chessboard_params_to_payload(params),
    )


def detect_charuco(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: CharucoDetectorParams,
) -> CharucoDetection:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, CharucoDetectorParams)

    raw = _core.detect_charuco(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=charuco_detector_params_to_payload(params),
    )
    return CharucoDetection.from_dict(raw)


def detect_charuco_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: CharucoDetectorParams,
) -> CharucoDetection:
    """Detect a ChArUco board from a pre-detected corner cloud.

    ``corners`` is a list of ``ChessCorner``-shaped dicts —
    ``{"position": [x, y], "axes": [...], "strength": ...}`` — e.g. the
    ``"corners"`` field of :func:`trace_chessboard_topological`'s return
    value, or a custom upstream. ``params.chess`` is not read: ``corners`` is
    taken as given.
    """
    _check_type("params", params, CharucoDetectorParams)

    raw = _core.detect_charuco_with_corners(
        image,
        corners,
        params=charuco_detector_params_to_payload(params),
    )
    return CharucoDetection.from_dict(raw)


def detect_marker_board(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: MarkerBoardParams | None = None,
) -> MarkerBoardDetection | None:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, MarkerBoardParams)

    raw = _core.detect_marker_board(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=marker_board_params_to_payload(params),
    )
    if raw is None:
        return None
    return MarkerBoardDetection.from_dict(raw)


def detect_marker_board_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: MarkerBoardParams | None = None,
) -> MarkerBoardDetection | None:
    """Detect a marker board from a pre-detected corner cloud.

    ``corners`` is a list of ``ChessCorner``-shaped dicts — see
    :func:`detect_charuco_with_corners`. ``params.chess`` is not read.
    """
    if params is not None:
        _check_type("params", params, MarkerBoardParams)

    raw = _core.detect_marker_board_with_corners(
        image,
        corners,
        params=marker_board_params_to_payload(params),
    )
    if raw is None:
        return None
    return MarkerBoardDetection.from_dict(raw)


def diagnose_charuco(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: CharucoDetectorParams,
) -> dict[str, Any]:
    """Detect a ChArUco board and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  ``result`` is
    the ``CharucoDetection`` dict (or ``None`` when detection failed);
    ``diagnostics`` is the raw ``CharucoDetectDiagnostics`` payload, produced
    even on a failed frame.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    — it is designed for overlay scripts and JSON persistence, and carries a
    looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, CharucoDetectorParams)

    return _core.diagnose_charuco(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=charuco_detector_params_to_payload(params),
    )


def diagnose_charuco_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: CharucoDetectorParams,
) -> dict[str, Any]:
    """:func:`diagnose_charuco` on a pre-detected corner cloud.

    ``params.chess`` is not read. See :func:`detect_charuco_with_corners` for
    the ``corners`` shape.
    """
    _check_type("params", params, CharucoDetectorParams)

    return _core.diagnose_charuco_with_corners(
        image,
        corners,
        params=charuco_detector_params_to_payload(params),
    )


def diagnose_marker_board(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: MarkerBoardParams | None = None,
) -> dict[str, Any]:
    """Detect a marker board and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  ``result`` is
    the ``MarkerBoardDetection`` dict (or ``None`` when detection failed);
    ``diagnostics`` is the raw ``MarkerBoardDiagnostics`` payload (scored
    circle candidates, circle matches, per-corner provenance, alignment-inlier
    count). Diagnostics are produced even on a failed detection — e.g. a grid
    found but its circles not matching the expected layout.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    and carries a looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, MarkerBoardParams)

    return _core.diagnose_marker_board(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=marker_board_params_to_payload(params),
    )


def diagnose_marker_board_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: MarkerBoardParams | None = None,
) -> dict[str, Any]:
    """:func:`diagnose_marker_board` on a pre-detected corner cloud.

    ``params.chess`` is not read. See :func:`detect_charuco_with_corners` for
    the ``corners`` shape.
    """
    if params is not None:
        _check_type("params", params, MarkerBoardParams)

    return _core.diagnose_marker_board_with_corners(
        image,
        corners,
        params=marker_board_params_to_payload(params),
    )


def detect_puzzleboard(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: PuzzleBoardParams,
) -> PuzzleBoardDetection:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, PuzzleBoardParams)

    raw = _core.detect_puzzleboard(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=puzzleboard_params_to_payload(params),
    )
    return PuzzleBoardDetection.from_dict(raw)


def detect_puzzleboard_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: PuzzleBoardParams,
) -> PuzzleBoardDetection:
    """Detect a PuzzleBoard from a pre-detected corner cloud.

    ``corners`` is a list of ``ChessCorner``-shaped dicts — see
    :func:`detect_charuco_with_corners`. ``params.chess`` is not read.
    """
    _check_type("params", params, PuzzleBoardParams)

    raw = _core.detect_puzzleboard_with_corners(
        image,
        corners,
        params=puzzleboard_params_to_payload(params),
    )
    return PuzzleBoardDetection.from_dict(raw)


def diagnose_puzzleboard(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: PuzzleBoardParams,
) -> dict[str, Any]:
    """Detect a PuzzleBoard and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  ``result`` is
    the ``PuzzleBoardDetection`` dict (or ``None`` when detection
    failed); ``diagnostics`` is the raw ``PuzzleBoardDiagnostics`` payload (raw
    pre-alignment per-edge bit observations and winner-vs-runner-up scoring
    evidence), produced even on a failed decode.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    and carries a looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, PuzzleBoardParams)

    return _core.diagnose_puzzleboard(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=puzzleboard_params_to_payload(params),
    )


def diagnose_puzzleboard_with_corners(
    image: npt.NDArray[np.uint8],
    corners: list[dict[str, Any]],
    *,
    params: PuzzleBoardParams,
) -> dict[str, Any]:
    """:func:`diagnose_puzzleboard` on a pre-detected corner cloud.

    ``params.chess`` is not read. See :func:`detect_charuco_with_corners` for
    the ``corners`` shape.
    """
    _check_type("params", params, PuzzleBoardParams)

    return _core.diagnose_puzzleboard_with_corners(
        image,
        corners,
        params=puzzleboard_params_to_payload(params),
    )


def detect_chessboard_best(
    image: npt.NDArray[np.uint8],
    configs: list[ChessboardParams],
    *,
    chess_cfg: ChessConfig | None = None,
) -> ChessboardDetectionResult | None:
    """Try multiple chessboard configs, return the best result (most corners)."""
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, ChessboardParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_chessboard_best(
        image,
        payloads,
        chess_cfg=chess_config_to_payload(chess_cfg),
    )
    if raw is None:
        return None
    return ChessboardDetectionResult.from_dict(raw)


def detect_charuco_best(
    image: npt.NDArray[np.uint8],
    configs: list[CharucoDetectorParams],
) -> CharucoDetection:
    """Try multiple ChArUco configs, return the best result (most markers, then corners)."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, CharucoDetectorParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_charuco_best(image, payloads)
    return CharucoDetection.from_dict(raw)


def detect_marker_board_best(
    image: npt.NDArray[np.uint8],
    configs: list[MarkerBoardParams],
) -> MarkerBoardDetection | None:
    """Try multiple marker board configs, return the best result (most corners)."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, MarkerBoardParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_marker_board_best(image, payloads)
    if raw is None:
        return None
    return MarkerBoardDetection.from_dict(raw)


def detect_puzzleboard_best(
    image: npt.NDArray[np.uint8],
    configs: list[PuzzleBoardParams],
) -> PuzzleBoardDetection:
    """Try multiple PuzzleBoard configs, return the best result."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, PuzzleBoardParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_puzzleboard_best(image, payloads)
    return PuzzleBoardDetection.from_dict(raw)


def default_puzzleboard_params(rows: int, cols: int) -> PuzzleBoardParams:
    """Return Rust-side default PuzzleBoard parameters for a board size."""
    raw = _core.default_puzzleboard_params(rows, cols)
    return PuzzleBoardParams.from_dict(raw)


__all__ = [
    "detect_chessboard",
    "detect_chessboard_all",
    "trace_chessboard_topological",
    "detect_charuco",
    "detect_charuco_with_corners",
    "diagnose_charuco",
    "diagnose_charuco_with_corners",
    "detect_marker_board",
    "detect_marker_board_with_corners",
    "diagnose_marker_board",
    "diagnose_marker_board_with_corners",
    "detect_puzzleboard",
    "detect_puzzleboard_with_corners",
    "diagnose_puzzleboard",
    "diagnose_puzzleboard_with_corners",
    "detect_chessboard_best",
    "detect_charuco_best",
    "detect_marker_board_best",
    "detect_puzzleboard_best",
    "default_puzzleboard_params",
]
