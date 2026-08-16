"""Parity pins: every Python sweep preset *is* the Rust sweep preset.

The Python ``sweep_*`` classmethods are thin delegators to the Rust presets
exposed on ``_core`` (``chessboard_sweep_default``, ``charuco_sweep_for_board``,
``marker_board_sweep_for_board``, ``puzzleboard_sweep_for_board``). These tests
assert that what a Python caller actually sees — the parsed dataclass list —
carries the values Rust produced: same config count, same field values.

Why this matters: the presets used to be hand-ported into ``config.py``, and a
hand port drifts silently. ``PuzzleBoardParams.sweep_for_board`` had drifted onto
a completely different axis (it varied ``chess.threshold``) while its docstring
still claimed to mirror Rust, so ``detect_puzzleboard_best`` searched a different
configuration space from Python than from Rust. These tests fail on exactly that
class of divergence.

The comparison is directional on purpose: every leaf the Rust preset emits must
appear, with the same value, in the Python-visible ``to_dict()``. The Python
dataclasses always materialise the opt-in ``"advanced"`` blocks that Rust omits
when unset, and omit ``Option`` fields that Rust emits as ``null``; an exact dict
equality would therefore compare serialisation conventions rather than
configuration. A missing key, a changed value, an extra varied axis, or a
differing config count all fail.

The properties this pins, given the delegation:

- Python drifting alone (a hand-written body reintroduced, a preset renamed away,
  a classmethod that filters or reorders configs) fails immediately.
- Rust drifting alone fails whenever the Python dataclass cannot carry the new
  configuration — a knob Rust varies but ``from_dict`` drops falls back to the
  Python default and mismatches the Rust leaf.
"""

from __future__ import annotations

import contextlib
from typing import Any

import numpy as np

import calib_targets as ct
from calib_targets import _core


# ---------------------------------------------------------------------------
# Board specs used to instantiate the board-dependent presets
# ---------------------------------------------------------------------------


def _charuco_board() -> ct.CharucoBoardSpec:
    return ct.CharucoBoardSpec(
        rows=22,
        cols=22,
        cell_size=5.2,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_1000",
        marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
    )


def _marker_board() -> ct.MarkerBoardSpec:
    return ct.MarkerBoardSpec(
        rows=22,
        cols=22,
        circles=(
            ct.MarkerCircleSpec(i=11, j=11, polarity=ct.CirclePolarity.WHITE),
            ct.MarkerCircleSpec(i=12, j=11, polarity=ct.CirclePolarity.BLACK),
            ct.MarkerCircleSpec(i=12, j=12, polarity=ct.CirclePolarity.WHITE),
        ),
        cell_size=5.2,
    )


def _puzzle_board() -> ct.PuzzleBoardSpec:
    return ct.PuzzleBoardSpec(rows=13, cols=13, cell_size=1.014)


# ---------------------------------------------------------------------------
# Comparison helpers
# ---------------------------------------------------------------------------


def _same_value(rust: Any, py: Any) -> bool:
    """Compare two config leaves for equality *as the Rust config sees them*.

    The tolerance is not a fudge factor: the numeric fields of these params
    structs are ``f32``, so a Python float that travels Python → Rust → Python
    comes back as the nearest ``f32`` (``1.014`` → ``1.0140000581741333``). Two
    values that land on the same ``f32`` configure the detector identically;
    values that differ meaningfully still land on different ``f32``\\ s and fail.
    """
    if rust == py:
        return True
    if isinstance(rust, bool) or isinstance(py, bool):
        return False
    if isinstance(rust, (int, float)) and isinstance(py, (int, float)):
        return np.float32(rust) == np.float32(py)
    return False


def _assert_rust_leaves_present(rust: Any, py: Any, path: str) -> None:
    """Assert every leaf of the Rust config appears unchanged in ``py``."""
    if rust is None:
        assert py is None, f"{path}: Rust emits null, Python has {py!r}"
        return
    if isinstance(rust, dict):
        assert isinstance(py, dict), (
            f"{path}: expected a dict on the Python side, got {type(py).__name__}"
        )
        for key, value in rust.items():
            child = f"{path}.{key}"
            if value is None:
                # A Rust `Option::None`; the Python `to_dict` may omit the key.
                assert py.get(key) is None, (
                    f"{child}: Rust emits null, Python has {py.get(key)!r}"
                )
                continue
            assert key in py, f"{child}: missing from the Python config"
            _assert_rust_leaves_present(value, py[key], child)
        return
    if isinstance(rust, list):
        assert isinstance(py, list), (
            f"{path}: expected a list on the Python side, got {type(py).__name__}"
        )
        assert len(py) == len(rust), (
            f"{path}: Rust has {len(rust)} items, Python has {len(py)}"
        )
        for idx, (rust_item, py_item) in enumerate(zip(rust, py)):
            _assert_rust_leaves_present(rust_item, py_item, f"{path}[{idx}]")
        return
    # `bool` is a subclass of `int`, so compare the kinds too: `True == 1`.
    assert isinstance(py, bool) == isinstance(rust, bool), (
        f"{path}: Rust {rust!r} and Python {py!r} disagree on type"
    )
    assert _same_value(rust, py), f"{path}: Rust {rust!r} != Python {py!r}"


def _assert_preset_parity(
    py_configs: list[Any], rust_configs: list[dict[str, Any]], cls: type[Any]
) -> None:
    assert len(py_configs) == len(rust_configs), (
        f"{cls.__name__}: Rust yields {len(rust_configs)} configs, "
        f"Python yields {len(py_configs)}"
    )
    assert py_configs, f"{cls.__name__}: the sweep preset must not be empty"
    for idx, (rust_config, py_config) in enumerate(zip(rust_configs, py_configs)):
        assert isinstance(py_config, cls)
        py_dict = py_config.to_dict()
        _assert_rust_leaves_present(rust_config, py_dict, f"configs[{idx}]")
        # The delegation is lossless: re-parsing the Rust dict reproduces the
        # very object the classmethod handed back.
        assert cls.from_dict(rust_config).to_dict() == py_dict, (
            f"configs[{idx}]: parsing the Rust config does not reproduce the "
            f"Python config"
        )


# ---------------------------------------------------------------------------
# Presets
# ---------------------------------------------------------------------------


def test_chessboard_sweep_default_matches_rust() -> None:
    _assert_preset_parity(
        ct.ChessboardParams.sweep_default(),
        _core.chessboard_sweep_default(),
        ct.ChessboardParams,
    )


def test_charuco_sweep_for_board_matches_rust() -> None:
    board = _charuco_board()
    _assert_preset_parity(
        ct.CharucoParams.sweep_for_board(board),
        _core.charuco_sweep_for_board(board.to_dict()),
        ct.CharucoParams,
    )


def test_marker_board_sweep_for_board_matches_rust() -> None:
    board = _marker_board()
    _assert_preset_parity(
        ct.MarkerBoardParams.sweep_for_board(board),
        _core.marker_board_sweep_for_board(board.to_dict()),
        ct.MarkerBoardParams,
    )


def test_puzzleboard_sweep_for_board_matches_rust() -> None:
    board = _puzzle_board()
    _assert_preset_parity(
        ct.PuzzleBoardParams.sweep_for_board(board),
        _core.puzzleboard_sweep_for_board(board.to_dict()),
        ct.PuzzleBoardParams,
    )


# ---------------------------------------------------------------------------
# The board spec must survive into every config
# ---------------------------------------------------------------------------


def test_board_dependent_presets_carry_the_caller_board() -> None:
    """The caller's board — not a placeholder — reaches every config.

    The board-dependent presets take a full spec rather than a ``(rows, cols)``
    pair, so board-level fields the detector needs (``cell_size``, the
    PuzzleBoard origin, the marker-circle layout) must not be dropped in the
    Python → Rust → Python hop.
    """
    charuco_board = _charuco_board()
    for idx, cfg in enumerate(ct.CharucoParams.sweep_for_board(charuco_board)):
        _assert_rust_leaves_present(
            charuco_board.to_dict(), cfg.board.to_dict(), f"charuco[{idx}].board"
        )

    marker_board = _marker_board()
    for idx, cfg in enumerate(ct.MarkerBoardParams.sweep_for_board(marker_board)):
        _assert_rust_leaves_present(
            marker_board.to_dict(), cfg.board.to_dict(), f"marker[{idx}].board"
        )

    puzzle_board = _puzzle_board()
    for idx, cfg in enumerate(ct.PuzzleBoardParams.sweep_for_board(puzzle_board)):
        _assert_rust_leaves_present(
            puzzle_board.to_dict(), cfg.board.to_dict(), f"puzzle[{idx}].board"
        )


def test_presets_are_accepted_back_by_rust() -> None:
    """Each preset config round-trips into the Rust params struct it came from.

    ``to_dict`` output is what ``detect_*_best`` forwards as ``configs=``, and
    every params struct is ``#[serde(deny_unknown_fields)]`` — so a key the
    Python dataclass invents, or a required key it drops, surfaces here as a
    ``ValueError``. A blank image detects nothing; the ChArUco / PuzzleBoard
    entry points report that as ``RuntimeError``, which is not a schema failure.
    """
    image = np.zeros((32, 32), dtype=np.uint8)

    with contextlib.suppress(RuntimeError):
        ct.detect_chessboard_best(image, ct.ChessboardParams.sweep_default())
    with contextlib.suppress(RuntimeError):
        ct.detect_charuco_best(image, ct.CharucoParams.sweep_for_board(_charuco_board()))
    with contextlib.suppress(RuntimeError):
        ct.detect_marker_board_best(
            image, ct.MarkerBoardParams.sweep_for_board(_marker_board())
        )
    with contextlib.suppress(RuntimeError):
        ct.detect_puzzleboard_best(
            image, ct.PuzzleBoardParams.sweep_for_board(_puzzle_board())
        )
