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
            spec = WindowSpec(span=span, readout=readout)
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
                spec = WindowSpec(span=span, readout=readout)
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

    return parser


def run(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)
    return int(args.func(args))
