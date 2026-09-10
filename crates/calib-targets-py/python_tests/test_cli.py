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
    dxf_path = stem.with_suffix(".dxf")
    assert dxf_path.is_file()
    dxf = dxf_path.read_text()
    assert "AC1015" in dxf
    assert "$INSUNITS\n 70\n4\n" in dxf


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


def test_cli_gen_chessboard_with_inner_square_rel(tmp_path: Path) -> None:
    baseline_stem = tmp_path / "baseline"
    inset_stem = tmp_path / "inset"

    assert cli_main([
        "gen", "chessboard",
        "--out-stem", str(baseline_stem),
        "--inner-rows", "6",
        "--inner-cols", "8",
        "--square-size-mm", "20",
    ]) == 0
    assert cli_main([
        "gen", "chessboard",
        "--out-stem", str(inset_stem),
        "--inner-rows", "6",
        "--inner-cols", "8",
        "--square-size-mm", "20",
        "--inner-square-rel", "0.4",
    ]) == 0
    _assert_bundle(baseline_stem)
    _assert_bundle(inset_stem)

    baseline_svg = baseline_stem.with_suffix(".svg").read_text()
    inset_svg = inset_stem.with_suffix(".svg").read_text()
    assert inset_svg.count("<rect ") > baseline_svg.count("<rect ")

    inset_json = json.loads(inset_stem.with_suffix(".json").read_text())
    assert inset_json["target"]["inner_square_rel"] == 0.4


def test_cli_init_charuco_and_marker_board_accept_inner_square_rel(tmp_path: Path) -> None:
    charuco_spec = tmp_path / "charuco.json"
    assert cli_main([
        "init", "charuco",
        "--out", str(charuco_spec),
        "--rows", "5", "--cols", "7",
        "--square-size-mm", "20", "--marker-size-rel", "0.75",
        "--dictionary", "DICT_4X4_50",
        "--inner-square-rel", "0.3",
    ]) == 0
    charuco_data = json.loads(charuco_spec.read_text())
    assert charuco_data["target"]["inner_square_rel"] == 0.3

    marker_spec = tmp_path / "marker.json"
    assert cli_main([
        "init", "marker-board",
        "--out", str(marker_spec),
        "--inner-rows", "6", "--inner-cols", "8",
        "--square-size-mm", "20",
        "--inner-square-rel", "0.4",
    ]) == 0
    marker_data = json.loads(marker_spec.read_text())
    assert marker_data["target"]["inner_square_rel"] == 0.4


def test_helper_roundtrip_puzzlepole() -> None:
    doc = ct.puzzlepole_document(18, 8, 13.0)
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_puzzlepole_periods_come_from_rust() -> None:
    """The period table has one source of truth, and it is not this file.

    It is derived from the shipped code maps on the Rust side and pinned by a
    Rust test; a Python copy could drift from the pattern it describes. So this
    checks the bridge works and that the canonical start row agrees with the
    table, not that any particular number is right.
    """
    periods = ct.supported_puzzlepole_periods()
    assert periods, "the extension returned no supported periods"
    for circumference, start_row in periods:
        assert circumference % 6 == 0
    doc = ct.puzzlepole_document(12, 6, 10.0)
    assert (12, doc.target.start_row) in periods


def test_puzzlepole_diameter_is_quantised_by_its_period() -> None:
    doc = ct.puzzlepole_document(12, 6, 30.0)
    # The paper's own pole: 12 pieces at 3 cm is 11.46 cm across.
    assert abs(doc.target.diameter_mm - 114.59) < 0.01
    # Two pieces more are printed than wrap: one is the glue overlap, and the
    # trim lines fall mid-piece at both ends.
    assert doc.target.printed_strip_squares == 14


def test_cli_gen_puzzlepole_writes_bundle(tmp_path: Path) -> None:
    stem = tmp_path / "pole"
    rc = cli_main([
        "gen", "puzzlepole",
        "--out-stem", str(stem),
        "--circumference-squares", "18",
        "--axial-squares", "8",
        "--square-size-mm", "13",
        "--page-size", "a4",
    ])
    assert rc == 0
    for ext in ("json", "svg", "png", "dxf"):
        assert (tmp_path / f"pole.{ext}").is_file()


def test_cli_init_validate_puzzlepole(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    spec = tmp_path / "pole.json"
    assert cli_main([
        "init", "puzzlepole",
        "--out", str(spec),
        "--circumference-squares", "24",
        "--axial-squares", "6",
        "--square-size-mm", "10",
        "--page-size", "a4",
    ]) == 0
    assert cli_main(["validate", "--spec", str(spec)]) == 0
    # The wire tag and the kind name agree for this target, unlike its
    # PuzzleBoard sibling, so no translation is needed on the way out.
    assert "valid puzzlepole" in capsys.readouterr().out


def test_cli_puzzlepole_rejects_an_unsupported_circumference(tmp_path: Path) -> None:
    """A pole's diameter is quantised; rounding to a neighbour would silently
    produce a target that does not close."""
    with pytest.raises(SystemExit):
        cli_main([
            "gen", "puzzlepole",
            "--out-stem", str(tmp_path / "pole"),
            "--circumference-squares", "13",
            "--axial-squares", "6",
            "--square-size-mm", "10",
        ])
