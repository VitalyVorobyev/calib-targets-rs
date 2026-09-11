"""Generate a printable PuzzlePole wrap strip — the sheet you tape to a tube.

A PuzzlePole is the PuzzleBoard pattern wrapped round a cylinder, so it stays
identifiable from a full 360°. Its diameter is *quantised*: the pattern only
closes seamlessly at seven circumferences, and the one you pick fixes the tube
you need (``diameter = pieces × piece_size / π``).

Two things this adds over ``calib-targets gen puzzlepole``, and both are the
reason a first attempt usually fails:

- **The page is sized to the strip by default.** A pole tall enough to be
  useful does not fit A4, and the underlying renderer refuses rather than
  scaling — correctly, since a scaled target is the wrong size on paper.
- **``--pole-index`` picks a pole that shares no corner with the others.**
  Poles are cut from column windows of the same 501-column master; windows that
  overlap share corner ids, so two such poles in one workcell are
  indistinguishable. The stride is the corner-column count, not the piece
  count, which is easy to get wrong by hand.

Usage:

    # what diameters exist, at the piece size you have in mind
    python generate_printable_puzzlepole.py --list --square-size-mm 20

    # the strip itself
    python generate_printable_puzzlepole.py --out-stem out/pole24 \
        --circumference-squares 24 --axial-squares 12 --square-size-mm 20

    # a second pole that cannot be confused with the first
    python generate_printable_puzzlepole.py --out-stem out/pole24_b \
        --circumference-squares 24 --axial-squares 12 --square-size-mm 20 \
        --pole-index 1

Writes ``<stem>.{json,svg,png,dxf}``. Print the SVG or PDF-converted SVG at
100% scale — never "fit to page", which silently rescales the piece size and
makes every object point wrong.
"""

from __future__ import annotations

import argparse
import math
from collections import Counter
from pathlib import Path

import calib_targets as ct

# A4's printable area at the usual 10 mm margins. Used only to warn: a strip
# larger than this still generates on an auto-sized page, it just needs a
# plotter or tiling to come out at 100% scale.
A4_PRINTABLE_MM = (190.0, 277.0)


def list_periods(square_size_mm: float) -> None:
    """Print the circumferences that close seamlessly, and their diameters.

    Derived from the shipped code maps rather than transcribed from the paper,
    so it cannot drift from the pattern the detector reads.
    """
    strips = Counter(period for period, _ in ct.supported_puzzlepole_periods())
    print(f"seamless circumferences at {square_size_mm:g} mm pieces:\n")
    print(f"  {'pieces around':>13}  {'diameter':>9}  {'strips':>6}")
    for period in sorted(strips):
        diameter = period * square_size_mm / math.pi
        print(f"  {period:>13}  {diameter:>6.1f} mm  {strips[period]:>6}")
    print("\ndiameter = pieces x piece size / pi -- it is a consequence of the")
    print("period, never an input. To match a tube you already own, change the")
    print("piece size instead: every period can wrap every diameter.")


def resolve_pole(args: argparse.Namespace) -> ct.PuzzlePoleSpec:
    """The pole this run describes, as the *detector's* spec.

    Going through ``distinct`` even for ``--pole-index 0`` keeps one code path:
    index 0 is the canonical pole, so the two cases differ only in the window
    the pole is cut from.
    """
    supported = sorted({period for period, _ in ct.supported_puzzlepole_periods()})
    if args.circumference_squares not in supported:
        raise SystemExit(
            f"{args.circumference_squares} pieces around the circumference does not "
            f"close seamlessly; supported: {', '.join(str(p) for p in supported)}"
        )
    family = ct.PuzzlePoleSpec.distinct(
        args.circumference_squares, args.axial_squares, args.square_size_mm
    )
    if not 0 <= args.pole_index < len(family):
        raise SystemExit(
            f"--pole-index {args.pole_index} is out of range: a "
            f"{args.axial_squares}-piece pole yields {len(family)} poles with "
            "disjoint corner ids at this circumference"
        )
    return family[args.pole_index]


def resolve_page(args: argparse.Namespace, pole: ct.PuzzlePoleSpec) -> ct.PageSpec:
    """Page big enough for the strip, unless a stock size was asked for.

    The strip is *two* pieces taller than the wrap: one is trimmed away and one
    is the glue overlap, because a code dot sits on the joint and cutting at a
    piece boundary would destroy it.
    """
    if args.page != "auto":
        size = ct.PageSize.a4() if args.page == "a4" else ct.PageSize.letter()
        return ct.PageSpec(size=size, margin_mm=args.margin_mm)

    width_mm = pole.axial_squares * pole.cell_size_mm + 2.0 * args.margin_mm
    height_mm = (pole.circumference_squares + 2) * pole.cell_size_mm + 2.0 * args.margin_mm
    return ct.PageSpec(
        size=ct.PageSize.custom(width_mm=width_mm, height_mm=height_mm),
        margin_mm=args.margin_mm,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--list", action="store_true", help="print the supported periods and exit")
    parser.add_argument("--out-stem", type=Path, default=Path("out/puzzlepole"))
    parser.add_argument("--circumference-squares", type=int, default=24)
    parser.add_argument("--axial-squares", type=int, default=12)
    parser.add_argument("--square-size-mm", type=float, default=20.0)
    parser.add_argument(
        "--pole-index",
        type=int,
        default=0,
        help="which of the disjoint poles of this shape to cut (default: 0, the canonical one)",
    )
    parser.add_argument("--page", choices=("auto", "a4", "letter"), default="auto")
    parser.add_argument("--margin-mm", type=float, default=10.0)
    parser.add_argument("--dpi", type=int, default=300, help="PNG resolution (the SVG is exact)")
    args = parser.parse_args()

    if args.list:
        list_periods(args.square_size_mm)
        return 0

    pole = resolve_pole(args)
    page = resolve_page(args, pole)
    doc = ct.puzzlepole_document(
        pole.circumference_squares,
        pole.axial_squares,
        pole.cell_size_mm,
        start_row=pole.start_row,
        axial_start_col=pole.axial_start_col,
        page=page,
        render=ct.RenderOptions(png_dpi=args.dpi),
    )
    strip = doc.target
    assert isinstance(strip, ct.PuzzlePoleTargetSpec)

    strip_w = pole.axial_squares * pole.cell_size_mm
    strip_h = strip.printed_strip_squares * pole.cell_size_mm
    print(f"cylinder      : {pole.diameter_mm:.1f} mm across, {pole.axial_extent_mm:.0f} mm tall")
    print(
        f"printed strip : {pole.axial_squares} x {strip.printed_strip_squares} pieces of "
        f"{pole.cell_size_mm:g} mm  ->  {strip_w:.0f} x {strip_h:.0f} mm "
        f"(the wrap is {pole.circumference_squares}; 2 pieces are the trim and glue overlap)"
    )
    print(f"corner ids    : {pole.circumference_squares * pole.axial_corner_cols} on this pole")
    if args.page == "auto" and (
        strip_w > A4_PRINTABLE_MM[0] or strip_h > A4_PRINTABLE_MM[1]
    ):
        print("note          : larger than A4 -- print at a shop, or tile the SVG")

    args.out_stem.parent.mkdir(parents=True, exist_ok=True)
    try:
        written = ct.write_target_bundle(doc, args.out_stem)
    except RuntimeError as err:
        # The renderer refuses a strip larger than the page rather than scaling
        # it, which is right: a rescaled target is the wrong size on paper and
        # every object point it yields is then wrong by that factor.
        raise SystemExit(f"{err}\n\nDrop --page, or print smaller pieces.") from err
    print()
    for path in (written.svg_path, written.png_path, written.dxf_path, written.json_path):
        print(path)

    print("\nPrint at 100% scale (not 'fit to page'), wrap the strip round a")
    print(f"{pole.diameter_mm:.1f} mm tube and trim through the *middle* of the end")
    print("pieces -- a code dot sits on the joint. Then detect it with:\n")
    print(f"  python detect_puzzlepole.py <image.png> --doc {written.json_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
