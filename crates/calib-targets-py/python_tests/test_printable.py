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


def test_write_target_bundle(tmp_path: Path) -> None:
    written = ct.write_target_bundle(_marker_doc(), tmp_path / "board")
    json_path = Path(written.json_path)
    assert json_path.is_file()
    assert Path(written.svg_path).is_file()
    assert Path(written.png_path).is_file()
    doc = json.loads(json_path.read_text())
    assert doc["target"]["kind"] == "marker_board"
