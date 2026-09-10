"""Generate a printable PuzzlePole wrap strip.

A PuzzlePole is the PuzzleBoard pattern wrapped around a cylinder, so it is
recognisable from any direction. Its diameter is *quantised*: the pattern only
closes at a few circumferences, and the one you pick fixes the tube you need.

Usage:
    python generate_printable_puzzlepole.py [out_stem] [circumference_squares]
"""

from __future__ import annotations

import sys
from pathlib import Path

import calib_targets as ct


def main() -> None:
    out_stem = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("tmpdata/printable_puzzlepole")
    circumference = int(sys.argv[2]) if len(sys.argv) > 2 else 18

    supported = sorted({p for p, _ in ct.supported_puzzlepole_periods()})
    if circumference not in supported:
        print(f"{circumference} is not a supported circumference; try one of {supported}")
        raise SystemExit(1)

    # 18 pieces around at 13 mm is a ~75 mm pole, and its 20-piece strip is the
    # largest that fits A4 at that period. Larger poles need a larger sheet, not
    # a different period.
    doc = ct.puzzlepole_document(circumference, axial_squares=8, square_size_mm=13.0)
    spec = doc.target
    assert isinstance(spec, ct.PuzzlePoleTargetSpec)

    print(f"cylinder diameter : {spec.diameter_mm:.1f} mm")
    print(f"strip             : {spec.printed_strip_squares} pieces "
          f"({circumference} wrap + 1 trimmed away + 1 glue overlap)")
    print("print at 100% scale, then trim through the middle of the end pieces")

    written = ct.write_target_bundle(doc, out_stem)
    print(written.json_path)
    print(written.svg_path)
    print(written.png_path)


if __name__ == "__main__":
    main()
