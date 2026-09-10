from __future__ import annotations

from enum import Enum

from ._generated_dictionary import DICTIONARY_NAMES, DictionaryName


class TargetKind(str, Enum):
    CHESSBOARD = "chessboard"
    CHARUCO = "charuco"
    CHECKERBOARD_MARKER = "checkerboard_marker"
    PUZZLE_BOARD = "puzzle_board"
    # Spelled to match the printable spec's tag, which chose `puzzlepole` over
    # the `puzzle_board` / `puzzleboard` split its sibling carries. One
    # spelling per target across the workspace.
    PUZZLE_POLE = "puzzlepole"


class CirclePolarity(str, Enum):
    WHITE = "white"
    BLACK = "black"


class MarkerLayout(str, Enum):
    OPENCV_CHARUCO = "opencv_charuco"


__all__ = [
    "DICTIONARY_NAMES",
    "DictionaryName",
    "TargetKind",
    "CirclePolarity",
    "MarkerLayout",
]
