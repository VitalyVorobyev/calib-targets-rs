# Engineering Backlog

> The ledger for **engineering** findings that are not detector algorithms:
> binding surfaces, config plumbing, packaging, tooling, dependency hygiene.
> Algorithmic gaps in `projective-grid` / the chessboard pipeline belong in
> [`../algorithms/algorithmic_gaps.md`](../algorithms/algorithmic_gaps.md);
> measured optimization work belongs in the
> [performance backlog](performance.md#optimization-backlog).

Legend — **Sev:** P1 (blocks a consumer) · P2 (real friction) · P3 (hygiene).
**Effort:** S (<½ day) · M (1–3 days) · L (multi-PR). **API:** `additive` ·
`semver` (breaking) · `internal-safe` · `none`.

An item is deleted when it is done — closure is recorded in the CHANGELOG, not
here.

| Item | What | Sev | Effort | API |
|---|---|---|---|---|
| [B-2](#b-2) | `scan.marker_size_rel` repairs on a sentinel that is never set | P2 | S | semver |
| [B-3](#b-3) | The soft ChArUco path reports a `hamming` it did not compute | P3 | S | none |
| [B-4](#b-4) | Covariance cost: upstream `chess-corners` API asks | P3 | M | internal-safe |

---

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

## <a id="b-4"></a>B-4 · Position-covariance cost: upstream `chess-corners` API asks
**Sev P3 · Effort M (upstream) · API internal-safe**

- **Where the cost is.** With the default `Uncertainty::Propagated`, corner
  detection pays a per-frame noise estimate (≈50–60 µs on the public report
  images) plus a per-corner Jacobian (≈0.5–0.7 µs per raw corner with the
  default `CenterOfMass` refiner). Raw corner clouds on the public images are
  86–589 corners, of which 31–94 % end up labelled, so covariances computed for
  corners the grid builder discards cost ≈0.15 ms per frame at most (≈15 % of
  corner detection on `small.png`, ≈3 % on `large.png`, noise on `mid.png`).
- **What the workspace already does.** The front end (`detect_corner_frame`)
  estimates the noise once per frame before the corner pass and hands it to
  `chess-corners`; when the estimate declines (JPEG-like frames) the pass runs
  at `Uncertainty::Off`, which removes the Jacobians that could never be
  reported (≈−20 % corner detection on `oblique.png`). ChArUco re-detection
  reuses the level. Estimating up front costs ≈30–40 µs more in-process than
  letting `chess-corners` estimate after its response pass (same work; the
  difference is allocation/page reuse order), ≈1–2 % of corner detection on the
  frames where the estimate succeeds.
- **Ask 1 — report the σ the detector used.** e.g.
  `Detector::detect_u8_with_noise(&mut self, img, w, h) -> Result<(Vec<CornerDescriptor>, Option<f32>), ChessError>`
  (or `Detector::last_noise_sigma()`). Lets the workspace drop its own up-front
  estimate and recover the in-process ordering cost.
- **Ask 2 — estimate before refining, skip Jacobians on decline.** Estimate σ
  right after the response pass (image hot) and run the localizers at `Off`
  for the frame when it declines. Gives every `chess-corners` user the
  declined-frame saving the workspace now gets by estimating first.
- **Ask 3 — covariance for selected corners after the fact.** e.g.
  `Detector::detect_u8_deferred(..) -> DeferredDetection` exposing
  `corners()` and `position_covariance(index, noise_sigma) -> Option<PositionCovariance>`,
  bit-identical to what `detect_u8` at `Propagated` attaches (it must keep the
  response map and each corner's integer seed). Not reproducible from the
  current API: `CornerDescriptor` drops the seed, the `CenterOfMass` centroid
  runs in absolute response-map coordinates (an ROI re-run is local, so it
  rounds differently), and `RefineResult`'s systematic model is crate-private.
  Measured alternative through the public ROI path (`detect_u8_roi` per
  labelled corner) is ≈5 µs per corner — slower than the in-pass Jacobian it
  would replace — so it was not adopted. Worth doing only if ≈0.15 ms/frame
  matters to a consumer.

