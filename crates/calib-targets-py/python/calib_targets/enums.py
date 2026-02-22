from __future__ import annotations

from enum import Enum

from ._generated_dictionary import DICTIONARY_NAMES, DictionaryName


class TargetKind(str, Enum):
    CHESSBOARD = "chessboard"
    CHARUCO = "charuco"
    CHECKERBOARD_MARKER = "checkerboard_marker"


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
