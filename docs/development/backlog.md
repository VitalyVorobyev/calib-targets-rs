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
| [B-2](#b-2) | `scan.marker_size_rel` repairs on a sentinel that is never set — **resolved** | — | — | — |
| [B-3](#b-3) | The soft ChArUco path reports a `hamming` it did not compute | P3 | S | none |
| [B-4](#b-4) | Covariance cost: upstream `chess-corners` API asks — **resolved** | — | — | — |
| [B-5](#b-5) | Config knobs that nothing reads — **resolved** | — | — | — |
| [B-6](#b-6) | Two corner-strength floors on one response | P2 | S | semver |
| [B-7](#b-7) | "Board not found" is reported differently on every surface | P2 | M | semver |
| [B-8](#b-8) | Bindings mirror Rust params and defaults by hand | P2 | L | semver |
| [B-9](#b-9) | PuzzleBoard binding helpers fix `cell_size = 1` and require unused `rows`/`cols` | P2 | S | semver |
| [B-10](#b-10) | Binding and tooling parity gaps | P2 | M | additive |
| [B-11](#b-11) | `rows`/`cols` count corners on marker boards, squares elsewhere | P2 | S | semver |
| [B-12](#b-12) | Five `*_best` sweeps, five rule sets | P2 | M | semver |
| [B-13](#b-13) | Per-family detection skeleton is copied, not shared | P2 | M | internal-safe |
| [B-14](#b-14) | `projective-grid` helper duplication | P3 | M | internal-safe |
| [B-15](#b-15) | Print target specs mirror detector board specs | P2 | M | semver |
| [B-16](#b-16) | Dead code, dead features, forwarding modules | P3 | S | semver |
| [B-17](#b-17) | `#[non_exhaustive]` + constructor convention gaps | P3 | S | semver |
| [B-18](#b-18) | The accuracy suite does not test covariance calibration | P2 | M | none |

---

## <a id="b-2"></a>B-2 · `scan.marker_size_rel` repairs on an unset-sentinel that is never unset — **resolved**

Resolved in 0.16 with B-5: `CharucoParams` no longer carries a scan block;
the marker size and ring come from the board, and `inset_frac` is the one
sampling knob.

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

## <a id="b-4"></a>B-4 · Position-covariance cost: upstream `chess-corners` API asks — **resolved**

Resolved by `chess-corners` 2.0 and the 0.16 migration. The covariance is off
by default (`Uncertainty::Off`), so the default pays nothing for it. Ask 1
shipped as `Detector::last_noise_sigma()`, which the corner front end now reads
instead of estimating the noise itself; upstream skips the Jacobians when its
own estimate declines (Ask 2). Ask 3 (covariance for selected corners after the
fact) is moot while the covariance is opt-in.

## <a id="b-5"></a>B-5 · Config knobs that nothing reads — **resolved**

Resolved in 0.16 (see `docs/migrations/0.16.0.md`).

**Sev P2 · Effort S · API semver**

- **Problem.** Fields users can set — and docs used to tell them to tune — with
  no effect on detection:
  - `PuzzleBoardParams.px_per_square`: declared, defaulted, plumbed through
    Python and the C ABI, never read by the PuzzleBoard detector.
  - `CharucoParams.scan.{min_border_score, dedup_by_id, multi_threshold}`: the
    ChArUco matcher calls only `aruco::sample_cell`, which reads
    `border_bits`, `marker_size_rel` and `inset_frac`. `for_board` still sets
    `min_border_score`.
  - `aruco::ArucoScanConfig`: an all-`Option` copy of `ScanDecodeConfig`, used
    only by the bench and an example.
  - Python `ChessboardParams.chess`: exists only in Python, ignored by
    `detect_chessboard` (the corner config is a separate argument).
  - Python `OrientationClusteringParams`, `GridGraphParams`,
    `OrientationHistogram`, `GridGraph*`, `ChessboardDebug`: no Rust
    counterpart.
- **Fix.** Remove them (one 0.x minor). For ChArUco, replace `scan` with the
  one field the user has a reason to set (`inset_frac`) and derive the rest from
  the board — the audit B-2 asks for.

## <a id="b-6"></a>B-6 · Two corner-strength floors on one response
**Sev P2 · Effort S · API semver**

- **Problem.** `default_chess_config()` accepts ChESS responses ≥ 15;
  `ChessboardParams.min_corner_strength` (default 33) then drops corners whose
  `strength` — the same response — is below 33. Lowering the ChESS
  `threshold` therefore does nothing unless `min_corner_strength` is lowered
  too, and every family's params carry both.
- **Fix.** One floor at one level. Either `min_corner_strength` goes and the
  ChESS threshold default becomes the floor, or the ChESS threshold becomes
  internal (the paper-faithful `0`) and `min_corner_strength` is the knob.

## <a id="b-7"></a>B-7 · "Board not found" is reported differently on every surface
**Sev P2 · Effort M · API semver**

- **Problem.** Rust: a miss is one of ~10 nested `DetectError` variants with no
  `is_not_found()`. Python: `None` for chessboard and marker board,
  `RuntimeError` for ChArUco / PuzzleBoard / pole; `detect_chessboard` also
  maps an invalid config to `None`. WASM: `null` or a throw, same split. FFI:
  `CT_STATUS_NOT_FOUND` vs `CONFIG_ERROR` — the only surface that separates a
  miss from a misuse.
- **Fix.** `DetectError::is_not_found()`; one convention per binding (Python:
  `None` for a miss, a `ConfigError` exception for misuse; WASM likewise).

## <a id="b-8"></a>B-8 · Bindings mirror Rust params and defaults by hand
**Sev P2 · Effort L · API semver (Python/TS)**

- **Problem.** `xtask emit-schemas` generates `schemas/*.json`, but the Python
  `config.py`, the TypeScript `typescript-extras.d.ts`, the studio's
  `params_schema.rs`, the FFI `convert.rs` and the bench ablation catalogue each
  restate the params tree and its defaults. Python's
  `ChessboardParams.to_dict` always sends the whole unstable `advanced` block
  filled with Python's copies of the defaults, so a Rust default change never
  reaches Python users, and it flattens the stable/advanced split.
- **Fix.** Send only what the user set (omit `advanced` unless set); generate
  the Python/TS/studio mirrors from the schemas, or at least test them against
  the schemas the way `wasm/tests/check-schema-keys.mjs` does.

## <a id="b-9"></a>B-9 · PuzzleBoard binding helpers fix `cell_size = 1` and require unused `rows`/`cols`
**Sev P2 · Effort S · API semver**

- **Problem.** Python `default_puzzleboard_params(rows, cols)` and WASM
  `default_puzzleboard_params` / `puzzleboard_sweep_for_board` (and WASM
  `charuco_board_spec`) require `rows`/`cols`, which `Full` search only
  validates, and hard-code `cell_size = 1.0` — the one value that sets
  `target_position`.
- **Fix.** Take `cell_size`; default to `PuzzleBoardSpec::master(cell_size)`.

## <a id="b-10"></a>B-10 · Binding and tooling parity gaps
**Sev P2 · Effort M · API additive**

- PuzzlePole: no WASM detection or TS types, no FFI, no params schema, no
  `diagnose_*`, no `schemars` derive.
- FFI: no `*_best`, no `*_with_corners` (yet `docs/ffi/README.md` mentions
  them), no `border_bits` on the ChArUco board spec, a subset of the chessboard
  advanced knobs, and `*_diagnose_json` discards the result's `quality`.
- Covariance helpers: `mahalanobis_sq` exists only in Rust; Python has
  `sigma_max_px`; WASM and C have plain structs.
- Studio: no covariance or `DetectionQuality` in its responses (its README says
  it shows σ max), no `noise_sigma` / `uncertainty` knobs; the bench CLI has
  none either.
- Python has no `detect_corners`, so `*_with_corners` can only be fed from a
  trace.

## <a id="b-11"></a>B-11 · `rows`/`cols` count corners on marker boards, squares elsewhere
**Sev P2 · Effort S · API semver**

- **Problem.** `CharucoBoardSpec` and `PuzzleBoardSpec` count squares;
  `MarkerBoardSpec.rows`/`cols` count inner corners; the printable model spells
  it `inner_rows`/`inner_cols`. Same name, off by one between families.
- **Fix.** Rename the marker fields `inner_rows`/`inner_cols` (serde alias for
  one release), or count squares everywhere.

## <a id="b-12"></a>B-12 · Five `*_best` sweeps, five rule sets
**Sev P2 · Effort M · API semver**

- **Problem.** The facade's `detect_*_best` functions are one loop written five
  times with different rules: chessboard and marker keep the *last* best,
  ChArUco / PuzzleBoard / pole the first; ChArUco records constructor errors,
  marker silently skips them; an empty config list reports an invented cause.
  None says which config won. The sweeps mostly vary two angle tolerances,
  which suggests `detect_*` should try them itself rather than make "use
  `_best`" the recommended default. `*_from_gray_u8` copies the buffer and
  covers only the base `detect_*`.
- **Fix.** One generic `best_of` with one tie-break rule and the winning index
  in the result; take `GrayImageView` everywhere and drop `*_from_gray_u8`.

## <a id="b-13"></a>B-13 · Per-family detection skeleton is copied, not shared
**Sev P2 · Effort M · API internal-safe**

- Chessboard detection → labelled corners: 4 copies (ChArUco, marker,
  PuzzleBoard, pole). Grid → pixel corner map: 4 copies.
- ChArUco, PuzzleBoard and pole build an identity `inliers` vector and pass it
  down; two call sites then `inliers.contains()` in a loop (O(n²)).
- Pole `decode_component` copies the planar decode, and the copies have
  drifted: pole sets `logical_bit_error_rate` from the raw-dot rate in soft
  mode. `soft_cfg_from`, `effective_tuning` and `required_edges*` are duplicated
  between planar and pole.
- Best-component choice: `is_better_component_decode` for PuzzleBoard,
  `corners.len()` for pole. PuzzleBoard and marker build diagnostics on the hot
  path and drop them; ChArUco uses a zero-cost sink.
- Chessboard "detect corners → run detector → attach noise" is written in the
  facade (×3), WASM, FFI (×2) and the bench.
- **Fix.** `ChessboardDetection::{labeled_corners, corner_map}`, drop the
  `inliers` parameters, one `decode_edges` shared by planar and pole, a
  diagnostics sink everywhere, `ChessboardDetector::detect_image`.

## <a id="b-14"></a>B-14 · `projective-grid` helper duplication
**Sev P3 · Effort M · API internal-safe**

- Rebase-to-`(0, 0)` written 5×; bounding-box loops ~8× (one in core, two in
  the chessboard rectifiers).
- Edge-length / step statistics ~8×; `square_detector::estimate_cell_size`
  is the one that summed in hash order.
- Two DLT solvers: `geometry::estimate_projective` (unnormalised, used by
  `shared/fit.rs` and `check`) and `geometry::estimate_homography`
  (Hartley-normalised); the atlas says `shared::fit` uses the latter.
- The reordered D4 table 3× (core, `shared/merge.rs`, bench); the chessboard
  `recover.rs` comment that keeps its merge local because pg lacks the table is
  stale.
- `orient.rs` re-implements the K = 2 cases of `cluster::circular`
  (`global_two_modes`, `refine_axes`, `smooth_circular`, `UndirectedMean`).
- Hex vs square: component split, residual-drop fit (hex swallows errors with
  `.ok()`), solution assembly.
- **Fix.** One `rebase_to_origin`, `bbox`, per-axis edge-vector helper, one
  DLT, one exported D4 table.

## <a id="b-15"></a>B-15 · Print target specs mirror detector board specs
**Sev P2 · Effort M · API semver**

- **Problem.** `CharucoTargetSpec`/`CharucoBoardSpec`,
  `PuzzleBoardTargetSpec`/`PuzzleBoardSpec`, `PuzzlePoleTargetSpec`/
  `PuzzlePoleSpec`, `MarkerBoardTargetSpec`/`MarkerBoardSpec`, and two
  `MarkerCircleSpec`s, converted ad hoc. Corner-id and board-position math is
  re-derived in print, marker and bench. For PuzzleBoard the two disagree on
  the frame: print's `position_mm` is board-local, the detector's
  `target_position` is master-absolute — equal only when the origin is `(0, 0)`.
- **Fix.** Print specs embed the detector spec plus print-only fields; each
  board spec owns `corner_id` / `corner_xy`.

## <a id="b-16"></a>B-16 · Dead code, dead features, forwarding modules
**Sev P3 · Effort S · API semver**

- `core::rectify` (`RectifiedView`, `RectToImgMapper`): no users.
- Chessboard `rectified_view.rs` / `mesh_warp.rs`: re-exported, untested,
  re-implement `CornerMap`, and order cell corners TL, TR, BL, BR.
- `charuco/src/diagnostics.rs`: `pub use` only.
- PuzzleBoard depends on `chess-corners` without using it; ChArUco's
  `link-check` has no consumers.
- Core exports internal helpers (`log_sigmoid`, `sample_bilinear_fast`,
  `complete_cell_corners`, …) as public API.

## <a id="b-17"></a>B-17 · `#[non_exhaustive]` + constructor convention gaps
**Sev P3 · Effort S · API semver**

`AxisEstimate`, `GrayImageView`, `GrayImage`, `ChessboardDetector` (public
`params` bypasses validation), marker `CellCoords` / `CellOffset` /
`CircleCandidate`, `PuzzleBoardObservedEdge`, `RectifiedBoardView`,
`RectifiedMeshView`, print `ResolvedTargetPoint` / `ResolvedTargetLayout`, the
chessboard trace types, `DiagHypothesis`; `PuzzleBoardDecodeInfo` has no
constructor.

## <a id="b-18"></a>B-18 · The accuracy suite does not test covariance calibration
**Sev P2 · Effort M · API none**

- **Problem.** `tests/accuracy.rs` bounds σ̂₀² from above only. No lower bound
  (over-conservative covariances pass), no χ²₂ coverage (fraction of corners
  with `d² ≤ 5.99`), no calibration per σ bin; `variance_factor_rank_corr` is
  computed but not asserted; the noise estimate is printed, not compared to the
  rendered σ. Corners whose error exceeds `MATCH_RADIUS_PX` (1 px) drop out of
  both RMSE and σ̂₀², so an overconfident gross error is invisible.
- **Why first.** Any use of the covariance *in* detection (algorithmic gaps)
  needs this evidence that the covariance is calibrated on each family and
  refiner.
- **Fix.** A coverage band and a two-sided σ̂₀² band per family × refiner;
  count correctly labelled corners beyond the match radius as their own metric.
