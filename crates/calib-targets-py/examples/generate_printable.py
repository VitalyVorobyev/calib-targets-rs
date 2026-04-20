from __future__ import annotations

import sys
from pathlib import Path

import calib_targets as ct


def main() -> None:
    out_stem = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tmpdata/printable_charuco")
    doc = ct.charuco_document(
        rows=5,
        cols=7,
        square_size_mm=20.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_50",
    )
    written = ct.write_target_bundle(doc, out_stem)
    print(written.json_path)
    print(written.svg_path)
    print(written.png_path)


if __name__ == "__main__":
    main()
