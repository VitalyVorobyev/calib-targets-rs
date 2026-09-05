from __future__ import annotations

import json
from pathlib import Path

import calib_targets as ct


def _marker_doc() -> ct.PrintableTargetDocument:
    return ct.PrintableTargetDocument(
        target=ct.MarkerBoardTargetSpec(
            inner_rows=6,
            inner_cols=8,
            square_size_mm=20.0,
            circles=ct.MarkerBoardTargetSpec.default_circles(6, 8),
            circle_diameter_rel=0.5,
        )
    )


def test_printable_document_roundtrip() -> None:
    doc = _marker_doc()
    restored = ct.PrintableTargetDocument.from_dict(doc.to_dict())
    assert restored.to_dict() == doc.to_dict()


def test_render_target_bundle() -> None:
    bundle = ct.render_target_bundle(_marker_doc())
    assert bundle.json_text
    assert bundle.svg_text.startswith("<?xml")
    assert bundle.png_bytes.startswith(b"\x89PNG\r\n\x1a\n")
    # DXF is the chrome-on-glass photolith handoff format: R2000 ASCII,
    # mm units, single PATTERN layer.
    assert "AC1015" in bundle.dxf_text
    assert "$INSUNITS\n 70\n4\n" in bundle.dxf_text
    assert bundle.dxf_text.rstrip().endswith("EOF")


def test_write_target_bundle(tmp_path: Path) -> None:
    written = ct.write_target_bundle(_marker_doc(), tmp_path / "board")
    json_path = Path(written.json_path)
    assert json_path.is_file()
    assert Path(written.svg_path).is_file()
    assert Path(written.png_path).is_file()
    dxf_path = Path(written.dxf_path)
    assert dxf_path.is_file()
    dxf = dxf_path.read_text()
    assert "AC1015" in dxf
    assert "$INSUNITS\n 70\n4\n" in dxf
    doc = json.loads(json_path.read_text())
    assert doc["target"]["kind"] == "marker_board"


def test_inner_square_rel_omitted_when_none() -> None:
    doc = ct.chessboard_document(6, 8, 20.0)
    assert "inner_square_rel" not in doc.to_dict()["target"]


def test_inner_square_rel_roundtrips_for_all_three_specs() -> None:
    chessboard = ct.chessboard_document(6, 8, 20.0, inner_square_rel=0.4)
    assert chessboard.to_dict()["target"]["inner_square_rel"] == 0.4
    restored = ct.PrintableTargetDocument.from_dict(chessboard.to_dict())
    assert restored.to_dict() == chessboard.to_dict()

    charuco = ct.charuco_document(5, 7, 20.0, 0.75, "DICT_4X4_50", inner_square_rel=0.3)
    assert charuco.to_dict()["target"]["inner_square_rel"] == 0.3
    restored = ct.PrintableTargetDocument.from_dict(charuco.to_dict())
    assert restored.to_dict() == charuco.to_dict()

    marker_board = ct.marker_board_document(6, 8, 20.0, inner_square_rel=0.25)
    assert marker_board.to_dict()["target"]["inner_square_rel"] == 0.25
    restored = ct.PrintableTargetDocument.from_dict(marker_board.to_dict())
    assert restored.to_dict() == marker_board.to_dict()


def test_render_target_bundle_with_inner_square_rel() -> None:
    baseline = ct.render_target_bundle(ct.chessboard_document(4, 4, 20.0))
    inset = ct.render_target_bundle(ct.chessboard_document(4, 4, 20.0, inner_square_rel=0.4))

    assert inset.svg_text.count("<rect ") > baseline.svg_text.count("<rect ")
    # The inset must reach the DXF as a second LWPOLYLINE per black square,
    # never as a solid black square (see the Rust RectWithHole docs).
    assert inset.dxf_text.count("LWPOLYLINE") > baseline.dxf_text.count("LWPOLYLINE")
