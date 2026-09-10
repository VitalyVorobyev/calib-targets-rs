"""``pbr`` — the reproducible front end for every number in the study."""

from __future__ import annotations

import argparse
import json
import time

import numpy as np

from .counting import count_summary
from .evaluate import evaluate_window
from .graph import RingGraph
from .params import REAL, TOY, Params
from .ring import Ring
from .window import ALL, INTERIOR, WindowSpec, paper_window


def _params(name: str) -> Params:
    return {"real": REAL, "toy": TOY}[name]


def _emit(payload: object, as_json: bool) -> None:
    if as_json:
        print(json.dumps(payload, indent=2, default=str))


# ---------------------------------------------------------------------------
# graph
# ---------------------------------------------------------------------------


def cmd_graph_info(args) -> int:
    p = _params(args.params)
    summary = RingGraph(p).summary()
    if args.json:
        _emit(summary, True)
        return 0
    print(f"ring graph for {p}")
    for key, value in summary.items():
        print(f"  {key:<24} {value}")
    return 0


def cmd_graph_count(args) -> int:
    p = _params(args.params)
    g = RingGraph(p)
    summary = count_summary(g, p)
    if args.json:
        _emit(summary, True)
        return 0
    rings = int(summary["valid_rings"])  # type: ignore[arg-type]
    print(f"size of the search space for {p}")
    print(f"  arborescences to root 0      {summary['arborescences_to_root_0']}")
    circuits = summary["cyclic_circuits_per_loop"]
    assert isinstance(circuits, dict)
    per = next(iter(circuits.values()))
    print(f"  self-loops that may be cut   {len(circuits)}")
    print(f"  cyclic circuits per loop     {per:.6e}" if isinstance(per, float) else
          f"  cyclic circuits per loop     ~1e{len(str(per)) - 1}")
    print(f"  closing lifts per circuit    {summary['closing_lifts_per_circuit']}")
    print(f"  shift images                 {summary['shift_images_per_circuit_lift']}")
    print(f"  valid rings                  ~1e{len(str(rings)) - 1}  ({len(str(rings))} digits)")
    print(f"  candidate pairs              ~1e{len(str(rings * rings)) - 1}")
    return 0


# ---------------------------------------------------------------------------
# ring
# ---------------------------------------------------------------------------


def _parse_ring(text: str, p: Params) -> Ring:
    if p.alphabet <= 16:
        letters = tuple(int(ch, 16) for ch in text.strip())
    else:  # pragma: no cover - no such params in use
        letters = tuple(int(x) for x in text.split(","))
    return Ring(letters, p)


def cmd_ring_sample(args) -> int:
    from .sampling import sample_ring

    p = _params(args.params)
    g = RingGraph(p)
    rng = np.random.default_rng(args.seed)
    rows = []
    for _ in range(args.count):
        ring = sample_ring(g, p, rng)
        rows.append(
            {
                "letters": str(ring),
                "omitted_loop": ring.omitted_edges(g)[0],
                "stabiliser": ring.stabiliser_size,
            }
        )
    if args.json:
        _emit(rows, True)
        return 0
    for row in rows:
        print(f"{row['letters']}  loop={row['omitted_loop']}")
    return 0


def cmd_ring_check(args) -> int:
    from . import refboard

    p = _params(args.params)
    g = RingGraph(p)
    if args.authors:
        board = refboard.load()
        targets = [("map_a", board.ring_a), ("map_b", board.ring_b)]
    else:
        targets = [(f"ring{i}", _parse_ring(t, p)) for i, t in enumerate(args.rings)]
    rows = []
    for name, ring in targets:
        omitted = ring.omitted_edges(g)
        rows.append(
            {
                "name": name,
                "valid": ring.is_valid,
                "sigma_fixed_windows": ring.n_sigma_fixed,
                "orbit_duplicates": ring.n_orbit_duplicates,
                "omitted_edges": list(omitted),
                "omitted_is_self_loop": all(e in g.loops for e in omitted),
            }
        )
    if args.json:
        _emit(rows, True)
        return 0
    for row in rows:
        flag = "valid" if row["valid"] else "INVALID"
        print(
            f"{row['name']}: {flag}  omits {row['omitted_edges']}"
            f"  self-loop={row['omitted_is_self_loop']}"
        )
    return 0


# ---------------------------------------------------------------------------
# eval
# ---------------------------------------------------------------------------

_GROUPS = ("fixed", "c4", "d4")


def cmd_eval_reference(args) -> int:
    from . import refboard

    board = refboard.load()
    rows = []
    for readout, spans in (
        (ALL, range(3, args.max_span + 1)),
        (INTERIOR, range(4, args.max_span + 3)),
    ):
        for span in spans:
            spec = WindowSpec.square(span, readout=readout)
            if not spec.v_slots or not spec.h_slots:
                continue
            entry = {
                "readout": readout,
                "span": span,
                "pieces": spec.pieces,
                "edges": spec.n_edges,
                "informative_bits": spec.informative_bits(REAL.n_rows),
            }
            for group in _GROUPS:
                m = evaluate_window(board.ring_a, board.ring_b, spec, group, REAL)
                entry[group] = {
                    "unique_positions": m.n_unique,
                    "positions": m.positions,
                    "unique_patterns_pct": round(
                        100.0 * m.fraction_unique_patterns, 4
                    ),
                    "unique_positions_pct": round(
                        100.0 * m.fraction_unique_positions, 4
                    ),
                }
            rows.append(entry)
    if args.json:
        _emit(rows, True)
        return 0

    print("Reference board (authors' shipped maps), all 251001 master positions\n")
    print("  readout=all      every edge bounding the visible pieces (the paper's model)")
    print("  readout=interior only edges our sampler can read (outer ring excluded)\n")
    head = f"{'readout':>8} {'span':>4} {'pieces':>6} {'edges':>5} {'bits':>4}"
    head += f" | {'fixed':>10} {'C4':>10} {'D4':>10}   (unique positions)"
    print(head)
    print("-" * len(head))
    for row in rows:
        cells = " ".join(
            f"{100.0 * row[g]['unique_positions'] / row[g]['positions']:>9.4f}%"
            for g in _GROUPS
        )
        print(
            f"{row['readout']:>8} {row['span']:>4} {row['pieces']:>6} "
            f"{row['edges']:>5} {row['informative_bits']:>4} | {cells}"
        )

    print("\nPublished claims, in the paper's own denominator (distinct patterns):")
    for pieces in (3, 4):
        m = evaluate_window(board.ring_a, board.ring_b, paper_window(pieces), "c4", REAL)
        print(
            f"  {pieces}x{pieces} pieces / {m.n_edges} edges, C4:  "
            f"{m.format_patterns()}   [positions: {m.format_positions()}]"
        )
    return 0


def cmd_eval_verify(args) -> int:
    """Fast evaluator against the independent brute-force one."""
    from .brute import brute_metrics
    from .sampling import sample_ring

    p = TOY if args.toy else REAL
    g = RingGraph(p)
    rng = np.random.default_rng(args.seed)
    failures = 0
    checked = 0
    for _ in range(args.count):
        ring_a = sample_ring(g, p, rng)
        ring_b = sample_ring(g, p, rng)
        for readout in (ALL, INTERIOR):
            for span in args.spans:
                spec = WindowSpec.square(span, readout=readout)
                if not spec.v_slots or not spec.h_slots:
                    continue
                for group in _GROUPS:
                    t0 = time.perf_counter()
                    fast = evaluate_window(ring_a, ring_b, spec, group, p)
                    t_fast = time.perf_counter() - t0
                    slow = brute_metrics(ring_a, ring_b, span, readout, group, p)
                    checked += 1
                    same = fast.hypothesis_histogram == slow.hypothesis_histogram
                    if not same:
                        failures += 1
                        print(
                            f"MISMATCH {spec} {group}: "
                            f"fast={fast.hypothesis_histogram} slow={slow.hypothesis_histogram}"
                        )
                    if args.verbose:
                        print(f"  {spec} {group:>5} ok  fast={t_fast * 1e3:.1f}ms")
    print(f"{checked - failures}/{checked} exact agreements with the brute-force reference")
    return 1 if failures else 0


# ---------------------------------------------------------------------------
# pole
# ---------------------------------------------------------------------------


def _pole_periods(args):
    """The (squares, start_row) pairs a pole subcommand should act on."""
    from . import refboard
    from .pole import PAPER_CIRCUMFERENCES, PolePeriod, supported_periods

    board = refboard.load()
    circumferences = tuple(args.squares) if args.squares else PAPER_CIRCUMFERENCES
    periods = supported_periods(board.ring_a, board.ring_b, REAL, circumferences)
    if getattr(args, "phases", False):
        # A start row names a pole only modulo m·P: adding P keeps the stripe and
        # advances map A's row phase, giving a genuinely different pattern.
        periods = [
            PolePeriod(q.squares, q.start_row + REAL.period * j)
            for q in periods
            for j in range(REAL.n_rows)
        ]
    if args.canonical:
        seen: set[int] = set()
        keep: list[PolePeriod] = []
        for period in periods:
            if period.squares not in seen:
                seen.add(period.squares)
                keep.append(period)
        periods = keep
    return board, periods


def cmd_pole_periods(args) -> int:
    """The seam table, re-derived, plus what the derived stripe inherits."""
    from . import refboard
    from .pole import PolePattern, seam_depth

    board, periods = _pole_periods(args)
    # A start row names a pole only modulo 501, and the crate is free to list
    # its entries in either fundamental domain, so compare on the map-B phase.
    crate = list(refboard.crate_pole_periods())
    crate_phases = {(p, s % REAL.period) for p, s in crate}
    rows = []
    for period in periods:
        pole = PolePattern(period, REAL.master, board.ring_a, board.ring_b, REAL)
        windows = pole.stripe_windows()
        rows.append(
            {
                "squares": period.squares,
                "start_row": period.start_row,
                "strip_corner_rows": period.strip_corner_rows,
                "seam_depth_rows": seam_depth(board.ring_a, board.ring_b, period, REAL),
                "stripe_windows": len(windows),
                "stripe_windows_distinct": len(set(windows)),
                "in_crate_table": (period.squares, period.start_row % REAL.period)
                in crate_phases,
            }
        )
    derived = {(r["squares"], r["start_row"] % REAL.period) for r in rows}
    agrees = (
        derived == crate_phases
        if not args.squares and not args.canonical and not getattr(args, "phases", False)
        else None
    )
    if args.json:
        _emit({"periods": rows, "crate_table": crate, "agrees_with_crate": agrees}, True)
        return 0

    print("Seamless circumferences, re-derived from the shipped maps\n")
    head = f"{'p':>4} {'start':>6} {'strip rows':>11} {'seam depth':>11} {'stripe 3x3 windows':>20} {'in crate':>9}"
    print(head)
    print("-" * len(head))
    for r in rows:
        distinct = f"{r['stripe_windows_distinct']} / {r['stripe_windows']}"
        print(
            f"{r['squares']:>4} {r['start_row']:>6} {r['strip_corner_rows']:>11} "
            f"{r['seam_depth_rows']:>11} {distinct:>20} {r['in_crate_table']!s:>9}"
        )
    print(
        "\nseam depth is piece rows that recur p rows later; the construction gives"
        "\nexactly 2, which is why a pole is not decodable against the master."
    )
    if agrees is not None:
        print(f"agrees with the crate's SUPPORTED_PERIODS: {agrees}")
    return 0


def _floor_rows(board, periods, args) -> list[dict]:
    from .pole import PolePattern, pareto_floor, window_uniqueness

    rows = []
    for period in periods:
        for corner_cols in args.corner_cols:
            pole = PolePattern(period, corner_cols, board.ring_a, board.ring_b, REAL)
            for group in args.groups:
                t0 = time.perf_counter()
                frontier = pareto_floor(pole, group, args.max_span)
                elapsed = time.perf_counter() - t0
                points = []
                for span_y, span_x in frontier:
                    clean = window_uniqueness(pole, WindowSpec(span_y, span_x, INTERIOR), group)
                    entry = {
                        "span_y": span_y,
                        "span_x": span_x,
                        "edges": clean.n_edges,
                        "ambiguous": clean.n_ambiguous,
                        "placements": clean.n_placements,
                    }
                    if span_y - 1 >= 4:
                        # The floor is only a floor if one corner less fails.
                        below = window_uniqueness(
                            pole, WindowSpec(span_y - 1, span_x, INTERIOR), group
                        )
                        entry["witness_span_y"] = span_y - 1
                        entry["witness_ambiguous"] = below.n_ambiguous
                        entry["witness_placements"] = below.n_placements
                    points.append(entry)
                rows.append(
                    {
                        "squares": period.squares,
                        "start_row": period.start_row,
                        "corner_cols": corner_cols,
                        "group": group,
                        "frontier": points,
                        "seconds": round(elapsed, 3),
                    }
                )
    return rows


def cmd_pole_floor(args) -> int:
    """The decode-window uniqueness floor, as a 2-D Pareto frontier."""
    from .pole import PolePattern, floor_grid

    board, periods = _pole_periods(args)
    if args.grid:
        if len(periods) != 1 or len(args.corner_cols) != 1 or len(args.groups) != 1:
            print("--grid needs exactly one period, one width and one group")
            return 2
        pole = PolePattern(
            periods[0], args.corner_cols[0], board.ring_a, board.ring_b, REAL
        )
        grid = floor_grid(pole, args.groups[0], args.max_span)
        cells = [
            {
                "span_y": k[0],
                "span_x": k[1],
                "ambiguous": v.n_ambiguous,
                "placements": v.n_placements,
                "edges": v.n_edges,
            }
            for k, v in sorted(grid.items())
        ]
        if args.json:
            _emit(cells, True)
            return 0
        xs = sorted({c["span_x"] for c in cells})
        ys = sorted({c["span_y"] for c in cells})
        lookup = {(c["span_y"], c["span_x"]): c for c in cells}
        print(
            f"ambiguous placements, {periods[0]}, W={args.corner_cols[0]} corner cols, "
            f"{args.groups[0]}, {INTERIOR} readout"
        )
        print("cells are ambiguous placements; the denominator depends on span_x,")
        print("because the axial axis is clamped. Both are integers, never a rate.\n")
        print(" span_y \\ span_x " + " ".join(f"{x:>7}" for x in xs))
        denom = " ".join(f"{lookup[(ys[0], x)]['placements']:>7}" for x in xs)
        print(f"{'placements':>15} {denom}")
        for y in ys:
            cell_text = " ".join(
                f"{lookup[(y, x)]['ambiguous']:>7}" if (y, x) in lookup else f"{'-':>7}"
                for x in xs
            )
            print(f"{y:>15} {cell_text}")
        return 0

    rows = _floor_rows(board, periods, args)
    if args.json:
        _emit(rows, True)
        return 0
    print("PuzzlePole decode-window floor — Pareto-minimal (span_y, span_x) in CORNERS")
    print("span_y wraps the circumference (cyclic); span_x runs axially (clamped).")
    print("interior readout: a fragment's outermost ring of dots is unreadable.\n")
    head = f"{'p':>4} {'start':>6} {'W':>5} {'group':>6}  frontier (span_y x span_x : ambiguous/placements)"
    print(head)
    print("-" * len(head))
    for row in rows:
        if row["frontier"]:
            text = "   ".join(
                f"{q['span_y']}x{q['span_x']}: {q['ambiguous']}/{q['placements']}"
                for q in row["frontier"]
            )
        else:
            text = f"NONE within span {args.max_span}"
        print(
            f"{row['squares']:>4} {row['start_row']:>6} {row['corner_cols']:>5} "
            f"{row['group']:>6}  {text}"
        )
    return 0


def cmd_pole_view(args) -> int:
    """Is the floor reachable from a single view of a cylinder?"""
    from .pole import (
        DEFAULT_ARCS,
        PolePattern,
        ViewBudget,
        pareto_floor,
        required_arc_degrees,
    )

    board, periods = _pole_periods(args)
    arcs = tuple(args.arc_degrees) if args.arc_degrees else DEFAULT_ARCS
    budgets = [ViewBudget(a) for a in arcs]
    rows = []
    for period in periods:
        for corner_cols in args.corner_cols:
            pole = PolePattern(period, corner_cols, board.ring_a, board.ring_b, REAL)
            for group in args.groups:
                frontier = pareto_floor(pole, group, args.max_span)
                need = min((q[0] for q in frontier), default=None)
                at_span_x = min(
                    (q[1] for q in frontier if need is not None and q[0] == need),
                    default=None,
                )
                supply = {b.arc_degrees: b.visible_corner_rows(period.squares) for b in budgets}
                arc = None if need is None else required_arc_degrees(period.squares, need)
                rows.append(
                    {
                        "squares": period.squares,
                        "start_row": period.start_row,
                        "corner_cols": corner_cols,
                        "group": group,
                        "min_span_y": need,
                        "at_span_x": at_span_x,
                        "required_arc_degrees": None if arc is None else round(arc, 1),
                        "supply_corner_rows": supply,
                        "feasible": None
                        if need is None
                        else {a: supply[a] >= need for a in supply},
                    }
                )
    if args.json:
        _emit(rows, True)
        return 0
    print("Single-view feasibility. Demand = the smallest span_y anywhere on the")
    print("frontier, the span_x it needs, and the arc that many corner rows subtends.")
    print("Supply = corner rows an arc of the cylinder shows. The arc is a stated")
    print("assumption about foreshortening, not a measurement.\n")
    arc_head = " ".join(f"{a:>9.0f} deg" for a in arcs)
    head = (
        f"{'p':>4} {'start':>6} {'W':>5} {'group':>6} {'need y':>7} {'at x':>5} "
        f"{'needs arc':>10}  {arc_head}"
    )
    print(head)
    print("-" * len(head))
    for row in rows:
        need = row["min_span_y"]
        cells = " ".join(
            f"{row['supply_corner_rows'][a]:>6} rows"
            + ("  ok" if row["feasible"] and row["feasible"][a] else "  NO")
            for a in arcs
        )
        arc = row["required_arc_degrees"]
        print(
            f"{row['squares']:>4} {row['start_row']:>6} {row['corner_cols']:>5} "
            f"{row['group']:>6} {need!s:>7} {row['at_span_x']!s:>5} "
            f"{'-' if arc is None else f'{arc:.1f}':>10}  {cells}"
        )
    return 0


def cmd_pole_verify(args) -> int:
    """Vectorised pole readout against the independent brute-force oracle."""
    from . import refboard
    from .brute import pole_brute_metrics
    from .pole import PolePattern, PolePeriod, window_uniqueness

    board = refboard.load()
    checked = 0
    failures = 0
    for squares, start_row in args.cases:
        period = PolePeriod(squares, start_row)
        for corner_cols in args.corner_cols:
            pole = PolePattern(period, corner_cols, board.ring_a, board.ring_b, REAL)
            for span_y in range(4, args.max_span + 1):
                for span_x in range(4, args.max_span + 1):
                    spec = WindowSpec(span_y, span_x, INTERIOR)
                    if not pole.admits(spec):
                        continue
                    for group in ("c4", "d4"):
                        fast = window_uniqueness(pole, spec, group)
                        slow = pole_brute_metrics(
                            board.ring_a,
                            board.ring_b,
                            squares,
                            start_row,
                            corner_cols,
                            span_y,
                            span_x,
                            group,
                            REAL,
                        )
                        checked += 1
                        same = (
                            fast.n_placements == slow.n_placements
                            and fast.hypothesis_histogram == slow.hypothesis_histogram
                        )
                        if not same:
                            failures += 1
                            print(
                                f"MISMATCH p={squares}@{start_row} W={corner_cols} "
                                f"{span_y}x{span_x} {group}: "
                                f"fast={fast.hypothesis_histogram} "
                                f"slow={slow.hypothesis_histogram}"
                            )
    print(f"{checked - failures}/{checked} exact agreements with the brute-force oracle")
    return 1 if failures else 0


# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="pbr",
        description="Numerical study of the PuzzleBoard de Bruijn ring construction.",
    )
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    sub = parser.add_subparsers(dest="topic", required=True)

    graph = sub.add_parser("graph", help="the quotient ring graph").add_subparsers(
        dest="action", required=True
    )
    g_info = graph.add_parser("info", help="vertices, edges, degrees, self-loops")
    g_info.add_argument("--params", choices=["real", "toy"], default="real")
    g_info.set_defaults(func=cmd_graph_info)
    g_count = graph.add_parser("count", help="exact size of the search space")
    g_count.add_argument("--params", choices=["real", "toy"], default="real")
    g_count.set_defaults(func=cmd_graph_count)

    ring = sub.add_parser("ring", help="individual rings").add_subparsers(
        dest="action", required=True
    )
    r_sample = ring.add_parser("sample", help="uniform random valid rings")
    r_sample.add_argument("-n", "--count", type=int, default=5)
    r_sample.add_argument("--seed", type=int, default=0)
    r_sample.add_argument("--params", choices=["real", "toy"], default="real")
    r_sample.set_defaults(func=cmd_ring_sample)
    r_check = ring.add_parser("check", help="de Bruijn validity and the omitted orbit")
    r_check.add_argument("rings", nargs="*", default=[])
    r_check.add_argument("--authors", action="store_true", help="check the shipped maps")
    r_check.add_argument("--params", choices=["real", "toy"], default="real")
    r_check.set_defaults(func=cmd_ring_check)

    ev = sub.add_parser("eval", help="uniqueness metrics").add_subparsers(
        dest="action", required=True
    )
    e_ref = ev.add_parser("reference", help="the authors' board, every window and group")
    e_ref.add_argument("--max-span", type=int, default=7)
    e_ref.set_defaults(func=cmd_eval_reference)
    e_ver = ev.add_parser("verify", help="fast evaluator vs the brute-force reference")
    e_ver.add_argument("--toy", action="store_true", default=True)
    e_ver.add_argument("--full", dest="toy", action="store_false")
    e_ver.add_argument("-n", "--count", type=int, default=3)
    e_ver.add_argument("--spans", type=int, nargs="+", default=[3, 4, 5])
    e_ver.add_argument("--seed", type=int, default=0)
    e_ver.add_argument("-v", "--verbose", action="store_true")
    e_ver.set_defaults(func=cmd_eval_verify)

    pole = sub.add_parser("pole", help="PuzzlePole: the code wrapped on a cylinder")
    pole_sub = pole.add_subparsers(dest="action", required=True)

    def _common(sp, *, widths: list[int]) -> None:
        sp.add_argument(
            "--squares",
            type=int,
            nargs="+",
            default=[],
            help="circumferences in pieces (default: the paper's seven)",
        )
        sp.add_argument(
            "--canonical",
            action="store_true",
            help="one start row per circumference instead of all seamless ones",
        )
        sp.add_argument(
            "-W",
            "--corner-cols",
            type=int,
            nargs="+",
            default=widths,
            help="axial extent of the printed pole, in corner columns",
        )
        sp.add_argument(
            "--groups",
            nargs="+",
            choices=["c4", "d4"],
            default=["c4", "d4"],
            help="c4 is what production searches; d4 is the opt-in",
        )
        sp.add_argument("--max-span", type=int, default=16)

    p_per = pole_sub.add_parser("periods", help="the seam table, re-derived")
    _common(p_per, widths=[REAL.master])
    p_per.set_defaults(func=cmd_pole_periods)

    p_floor = pole_sub.add_parser("floor", help="the decode-window uniqueness floor")
    _common(p_floor, widths=[25, 51, 101, 201, REAL.master])
    p_floor.add_argument(
        "--grid",
        action="store_true",
        help="print the whole ambiguity grid for one period/width/group",
    )
    p_floor.add_argument(
        "--phases",
        action="store_true",
        help="also measure the other two map-A row phases of each start row",
    )
    p_floor.set_defaults(func=cmd_pole_floor)

    p_view = pole_sub.add_parser("view", help="is the floor reachable from one view?")
    _common(p_view, widths=[REAL.master])
    p_view.add_argument(
        "--arc-degrees",
        type=float,
        nargs="+",
        default=[],
        help="usable circumference arc of one view (default: 140 and 120)",
    )
    p_view.set_defaults(func=cmd_pole_view)

    p_ver = pole_sub.add_parser("verify", help="vectorised readout vs the oracle")
    p_ver.add_argument(
        "--cases",
        type=lambda s: tuple(int(x) for x in s.split(":")),
        nargs="+",
        default=[(12, 73), (18, 7), (30, 49)],
        metavar="SQUARES:START",
    )
    p_ver.add_argument("-W", "--corner-cols", type=int, nargs="+", default=[9, 12])
    p_ver.add_argument("--max-span", type=int, default=7)
    p_ver.set_defaults(func=cmd_pole_verify)

    return parser


def run(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)
    return int(args.func(args))
