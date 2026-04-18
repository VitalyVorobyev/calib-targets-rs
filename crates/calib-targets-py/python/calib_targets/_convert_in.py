"""Convert typed config dataclasses to dict payloads for the Rust _core module."""

from __future__ import annotations

from typing import Any

from .config import (
    CharucoDetectorParams,
    ChessboardParams,
    ChessConfig,
    MarkerBoardParams,
    PuzzleBoardParams,
)


def chess_config_to_payload(cfg: ChessConfig | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    return cfg.to_dict()


def chessboard_params_to_payload(cfg: ChessboardParams | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    return cfg.to_dict()


def marker_board_params_to_payload(cfg: MarkerBoardParams | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    return cfg.to_dict()


def charuco_detector_params_to_payload(cfg: CharucoDetectorParams) -> dict[str, Any]:
    return cfg.to_dict()


def puzzleboard_params_to_payload(cfg: PuzzleBoardParams) -> dict[str, Any]:
    return cfg.to_dict()


__all__ = [
    "chess_config_to_payload",
    "chessboard_params_to_payload",
    "marker_board_params_to_payload",
    "charuco_detector_params_to_payload",
    "puzzleboard_params_to_payload",
]
