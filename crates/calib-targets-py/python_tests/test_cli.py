"""Smoke tests for the `calib-targets` Python CLI and the new `*_document` helpers.

The CLI is installed as a console script via `[project.scripts]` in
`pyproject.toml`; we invoke the module with `python -m calib_targets.cli` so
the test works whether or not the console script shim is on $PATH.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

import calib_targets as ct
from calib_targets.cli import main as cli_main


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-m", "calib_targets.cli", *args],
        check=False,
        capture_output=True,
        text=True,
    )


def _assert_bundle(stem: Path) -> None:
    assert stem.with_suffix(".json").is_file()
    assert stem.with_suffix(".svg").is_file()
    assert stem.with_suffix(".png").is_file()


def test_helper_roundtrip_chessboard() -> None:
    doc = ct.chessboard_document(6, 8, 20.0)
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_helper_roundtrip_charuco() -> None:
    doc = ct.charuco_document(5, 7, 20.0, 0.75, "DICT_4X4_50")
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_helper_roundtrip_puzzleboard() -> None:
    doc = ct.puzzleboard_document(10, 12, 15.0)
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_helper_roundtrip_marker_board() -> None:
    doc = ct.marker_board_document(6, 8, 20.0)
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_cli_gen_chessboard_writes_bundle(tmp_path: Path) -> None:
    stem = tmp_path / "board"
    # Use the in-process entry point to avoid shelling out in the hot path.
    rc = cli_main([
        "gen", "chessboard",
        "--out-stem", str(stem),
        "--inner-rows", "6",
        "--inner-cols", "8",
        "--square-size-mm", "20",
    ])
    assert rc == 0
    _assert_bundle(stem)


def test_cli_gen_puzzleboard_writes_bundle(tmp_path: Path) -> None:
    stem = tmp_path / "puzzle"
    rc = cli_main([
        "gen", "puzzleboard",
        "--out-stem", str(stem),
        "--rows", "8",
        "--cols", "10",
        "--square-size-mm", "15",
    ])
    assert rc == 0
    _assert_bundle(stem)


def test_cli_init_then_generate_puzzleboard(tmp_path: Path) -> None:
    spec = tmp_path / "puzzle.json"
    stem = tmp_path / "generated/puzzle"
    assert cli_main([
        "init", "puzzleboard",
        "--out", str(spec),
        "--rows", "8",
        "--cols", "10",
        "--square-size-mm", "15",
    ]) == 0
    assert spec.is_file()
    data = json.loads(spec.read_text())
    assert data["target"]["kind"] == "puzzle_board"

    assert cli_main([
        "generate",
        "--spec", str(spec),
        "--out-stem", str(stem),
    ]) == 0
    _assert_bundle(stem)


def test_cli_list_dictionaries_via_subprocess() -> None:
    proc = _run("list-dictionaries")
    assert proc.returncode == 0, proc.stderr
    lines = proc.stdout.splitlines()
    assert lines, "expected at least one dictionary"
    assert "DICT_4X4_50" in lines
    assert lines == sorted(lines)


def test_cli_rejects_unknown_dictionary(tmp_path: Path) -> None:
    with pytest.raises(SystemExit):
        cli_main([
            "gen", "charuco",
            "--out-stem", str(tmp_path / "charuco"),
            "--rows", "5", "--cols", "7",
            "--square-size-mm", "20", "--marker-size-rel", "0.75",
            "--dictionary", "DICT_DOES_NOT_EXIST",
        ])
