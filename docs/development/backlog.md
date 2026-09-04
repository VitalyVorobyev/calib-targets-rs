# Engineering Backlog

> The ledger for **engineering** findings that are not detector algorithms:
> binding surfaces, config plumbing, packaging, tooling, dependency hygiene.
> Algorithmic gaps in `projective-grid` / the chessboard pipeline belong in
> [`../algorithms/algorithmic_gaps.md`](../algorithms/algorithmic_gaps.md);
> measured optimization work belongs in the
> [performance backlog](performance.md#optimization-backlog).
>
> This file replaces `architecture/chore-backlog.md`, which was deleted with the
> consolidation effort it tracked and left these findings homeless.

Legend — **Sev:** P1 (blocks a consumer) · P2 (real friction) · P3 (hygiene).
**Effort:** S (<½ day) · M (1–3 days) · L (multi-PR). **API:** `additive` ·
`semver` (breaking) · `internal-safe` · `none`.

An item is deleted when it is done — closure is recorded in the CHANGELOG, not
here.

| Item | What | Sev | Effort | API |
|---|---|---|---|---|
| [B-1](#b-1) | Printable specs cannot express an inner white square | P1 | M | additive |
| [B-2](#b-2) | `scan.marker_size_rel` repairs on a sentinel that is never set | P2 | S | semver |
| [B-3](#b-3) | The soft ChArUco path reports a `hamming` it did not compute | P3 | S | none |

---

## <a id="b-1"></a>B-1 · Printable specs cannot express an inner white square
**Sev P1 · Effort M · API additive**

- **Problem.** vitavision's own target generator draws a white square inset
  inside each black square — a real, shipped feature of its printable boards —
  and nothing in `calib-targets-print` expresses it. `ChessboardTargetSpec`,
  `CharucoTargetSpec` and `MarkerBoardTargetSpec` describe a plain
  black/white chequer, so a document round-tripped through this library loses
  the inset entirely.
- **Why it matters.** This is the *last* reason a second implementation of the
  printable-target geometry exists downstream. That second implementation is
  what shipped ChArUco markers rotated 180° — the defect this library was
  independently right about. `render_target_bundle_json` (0.14.0)
  closed the page/spec half of the gap; the inset is what still forces the fork.
  As long as the fork exists it can drift again.
- **Fix.** Add an optional `inner_square_rel: f64` (fraction of the square side,
  `None`/absent = no inset) to the three specs, with a serde default that keeps
  every existing document byte-identical, and honour it in the SVG / PNG / DXF
  renderers *and* in `resolved_points` if the inset moves any output point.
  Additive on `#[non_exhaustive]` structs, so no break. Mirror it in the WASM
  `typescript-extras.d.ts` and the Python config, per the binding-parity rule in
  [`conventions.md`](conventions.md).
- **Care.** The inset changes what the *detector* sees: an inner square adds
  four extra corners per cell that ChESS will fire on. Before shipping the
  renderer, check a rendered inset board through the chessboard and ChArUco
  detectors — if the extra corners enter the grid, the spec needs to state
  the supported inset range rather than accept any value.

## <a id="b-2"></a>B-2 · `scan.marker_size_rel` repairs on an unset-sentinel that is never unset
**Sev P2 · Effort S · API semver (behaviour)**

- **Problem.** `CharucoDetector::new` fills `scan.marker_size_rel` from the
  board only `if !is_finite() || <= 0.0`. `ScanDecodeConfig::default()` sets it
  to `1.0`, so the repair never fires for a defaulted scan block — the same
  shape of defect as the `border_bits` one fixed in 0.14.0, and the same two
  reachable paths (reassigning `params.board`, or deserialising a config that
  names the board and omits `scan`, which is what the Python bindings take).
  A board with `marker_size_rel = 0.75` is then sampled as if the marker filled
  the whole square.
- **Why it was not fixed alongside `border_bits`.** Scope: it changes sampling
  *geometry* rather than a ring predicate, so it needs its own before/after on
  the regression sets rather than riding a fix whose default-path output is
  byte-identical.
- **Fix.** Same argument: `marker_size_rel` describes the printed target, so
  derive it from `CharucoBoardSpec` unconditionally and document
  `scan.marker_size_rel` as derived, exactly as `border_bits` now is. Then
  audit `ScanDecodeConfig` for any *other* field a ChArUco detector should be
  deriving rather than accepting.
- **Evidence to gather first.** Measure what a mismatch actually costs before
  changing it. For `border_bits` the answer was surprising — a synthetic
  `border_bits = 3` board still decoded at hamming 0 with a one-cell ring
  configured, because the inset grid lands inside the payload either way — so
  do not assume `marker_size_rel` behaves as the arithmetic suggests either.

## <a id="b-3"></a>B-3 · The soft ChArUco path reports `hamming` it did not compute
**Sev P3 · Effort S · API none**

- **Problem.** `MarkerDetection::hamming` is documented as "Hamming distance
  between the observed code and the matched dictionary entry", which is what
  the legacy per-marker matcher (`aruco matcher.rs`) computes. The board-level
  matcher does not decode per marker — it scores soft per-cell likelihoods and
  picks an alignment — yet its markers still carry the field, reading `0`.
  A `0` there is not evidence of a clean decode, and it was read as such while
  investigating the `border_bits` derivation (a board sampled with the wrong
  ring reported `hamming = 0` for every marker).
- **Fix.** Either compute the real distance on the board-match path, or make
  the field `Option<u8>` / document it as legacy-matcher-only so a `0` cannot
  be mistaken for a verified decode. The second is cheaper and honest; the
  first is more useful for diagnostics.
