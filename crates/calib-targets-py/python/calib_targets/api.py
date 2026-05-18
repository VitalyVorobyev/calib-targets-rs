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
    CharucoDetectionResult,
    ChessboardDetectionResult,
    MarkerBoardDetectionResult,
    PuzzleBoardDetectionResult,
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


def detect_chessboard_debug(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: ChessboardParams | None = None,
) -> dict[str, Any]:
    """Run the instrumented chessboard detector and return a raw debug
    payload (``ChessboardDebugFrame``) as a plain ``dict``.

    Unlike :func:`detect_chessboard`, this entry point always returns a
    dict — even when detection fails — so callers can inspect per-stage
    counts and continuous metrics to diagnose the failure.

    The payload shape intentionally stays schemaless on the Python side;
    it is designed for overlay scripts and JSON persistence, not typed
    consumption. Top-level keys include ``image_width``, ``image_height``,
    ``strong_corners``, ``graph_neighbors``, ``stage_counts``, ``metrics``,
    ``orientations``, ``orientation_histogram``, and ``result``.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, ChessboardParams)

    return _core.detect_chessboard_debug(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=chessboard_params_to_payload(params),
    )


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
) -> CharucoDetectionResult:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, CharucoDetectorParams)

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
    return MarkerBoardDetectionResult.from_dict(raw)


def detect_charuco_with_diagnostics(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: CharucoDetectorParams,
) -> dict[str, Any]:
    """Detect a ChArUco board and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  ``result`` is
    the ``CharucoDetectionResult`` dict (or ``None`` when detection failed);
    ``diagnostics`` is the raw ``CharucoDetectDiagnostics`` payload, produced
    even on a failed frame.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    — it is designed for overlay scripts and JSON persistence, and carries a
    looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, CharucoDetectorParams)

    return _core.detect_charuco_with_diagnostics(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=charuco_detector_params_to_payload(params),
    )


def detect_marker_board_with_diagnostics(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: MarkerBoardParams | None = None,
) -> dict[str, Any]:
    """Detect a marker board and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  Both keys are
    ``None`` when no board is found — the marker-board diagnostics channel
    yields evidence only on a successful detection.  ``result`` is the
    ``MarkerBoardDetectionResult`` dict; ``diagnostics`` is the raw
    ``MarkerBoardDiagnostics`` payload.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    and carries a looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    if params is not None:
        _check_type("params", params, MarkerBoardParams)

    return _core.detect_marker_board_with_diagnostics(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=marker_board_params_to_payload(params),
    )


def detect_puzzleboard(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: PuzzleBoardParams,
) -> PuzzleBoardDetectionResult:
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, PuzzleBoardParams)

    raw = _core.detect_puzzleboard(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
        params=puzzleboard_params_to_payload(params),
    )
    return PuzzleBoardDetectionResult.from_dict(raw)


def detect_puzzleboard_with_diagnostics(
    image: npt.NDArray[np.uint8],
    *,
    chess_cfg: ChessConfig | None = None,
    params: PuzzleBoardParams,
) -> dict[str, Any]:
    """Detect a PuzzleBoard and additionally return the diagnostics channel.

    Returns a ``dict`` ``{"result": ..., "diagnostics": ...}``.  ``result`` is
    the ``PuzzleBoardDetectionResult`` dict (or ``None`` when detection
    failed); ``diagnostics`` is the raw ``PuzzleBoardDiagnostics`` payload (raw
    pre-alignment per-edge bit observations and winner-vs-runner-up scoring
    evidence), produced even on a failed decode.

    The ``diagnostics`` payload is intentionally schemaless on the Python side
    and carries a looser stability promise than the typed result API.
    """
    if chess_cfg is not None:
        _check_type("chess_cfg", chess_cfg, ChessConfig)
    _check_type("params", params, PuzzleBoardParams)

    return _core.detect_puzzleboard_with_diagnostics(
        image,
        chess_cfg=chess_config_to_payload(chess_cfg),
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
) -> CharucoDetectionResult:
    """Try multiple ChArUco configs, return the best result (most markers, then corners)."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, CharucoDetectorParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_charuco_best(image, payloads)
    return CharucoDetectionResult.from_dict(raw)


def detect_marker_board_best(
    image: npt.NDArray[np.uint8],
    configs: list[MarkerBoardParams],
) -> MarkerBoardDetectionResult | None:
    """Try multiple marker board configs, return the best result (most corners)."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, MarkerBoardParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_marker_board_best(image, payloads)
    if raw is None:
        return None
    return MarkerBoardDetectionResult.from_dict(raw)


def detect_puzzleboard_best(
    image: npt.NDArray[np.uint8],
    configs: list[PuzzleBoardParams],
) -> PuzzleBoardDetectionResult:
    """Try multiple PuzzleBoard configs, return the best result."""
    payloads = []
    for i, cfg in enumerate(configs):
        _check_type(f"configs[{i}]", cfg, PuzzleBoardParams)
        payloads.append(cfg.to_dict())

    raw = _core.detect_puzzleboard_best(image, payloads)
    return PuzzleBoardDetectionResult.from_dict(raw)


def default_puzzleboard_params(rows: int, cols: int) -> PuzzleBoardParams:
    """Return Rust-side default PuzzleBoard parameters for a board size."""
    raw = _core.default_puzzleboard_params(rows, cols)
    return PuzzleBoardParams.from_dict(raw)


__all__ = [
    "detect_chessboard",
    "detect_chessboard_all",
    "detect_chessboard_debug",
    "trace_chessboard_topological",
    "detect_charuco",
    "detect_charuco_with_diagnostics",
    "detect_marker_board",
    "detect_marker_board_with_diagnostics",
    "detect_puzzleboard",
    "detect_puzzleboard_with_diagnostics",
    "detect_chessboard_best",
    "detect_charuco_best",
    "detect_marker_board_best",
    "detect_puzzleboard_best",
    "default_puzzleboard_params",
]
