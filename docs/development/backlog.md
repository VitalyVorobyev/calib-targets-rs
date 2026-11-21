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
| [B-3](#b-3) | The soft ChArUco path reports a `hamming` it did not compute | P3 | S | none |
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
| [B-18](#b-18) | The accuracy suite does not test covariance calibration | P3 | M | none |
| [B-19](#b-19) | Upstream `chess-corners` findings from the real-data localizer study | P3 | S | none |
| [B-20](#b-20) | Hand-maintained mirrors next to the generated schemas | P2 | M | internal-safe |
| [B-21](#b-21) | Two parallel recovery engines | P2 | L | internal-safe |
| [B-22](#b-22) | Overlay renderer sprawl | P3 | M | none |
| [B-23](#b-23) | Unreferenced scripts and tools | P3 | S | none |
| [B-24](#b-24) | Source files over ~1000 lines | P3 | M | internal-safe |
| [B-25](#b-25) | Vestigial algorithm slug in the bench | P3 | S | none |
| [B-26](#b-26) | PuzzlePole lives in the PuzzleBoard crate | P3 | M | semver |

---

## <a id="b-3"></a>B-3 · The soft ChArUco path reports `hamming` it did not compute
**Sev P3 · Effort S · API none**

- **Problem.** `MarkerDetection::hamming` is documented as "Hamming distance
  between the observed code and the matched dictionary entry", which is what
  the legacy per-marker matcher (`aruco matcher.rs`) computes. The board-level
  matcher does not decode per marker — it scores soft per-cell likelihoods and
  picks an alignment — yet its markers still carry the field, reading `0`.
  A `0` there is not evidence of a clean decode (a board sampled with the wrong
  ring reports `hamming = 0` for every marker).
- **Fix.** Either compute the real distance on the board-match path, or make
  the field `Option<u8>` / document it as legacy-matcher-only so a `0` cannot
  be mistaken for a verified decode. The second is cheaper and honest; the
  first is more useful for diagnostics.

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

- Bounding-box loops ~8× (one in core, two in the chessboard rectifiers).
- Step-vector statistics outside `shared/cell_size.rs`: the chessboard
  `estimate_grid_steps` and the booster's directional medians.
- `orient.rs` re-implements the K = 2 cases of `cluster::circular`
  (`global_two_modes`, `refine_axes`, `smooth_circular`, `UndirectedMean`).
- Hex vs square: component split, residual-drop fit (hex swallows errors with
  `.ok()`), solution assembly.
- **Fix.** One `bbox` and one per-axis edge-vector helper.

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

- Chessboard `rectified_view.rs` / `mesh_warp.rs`: re-exported, untested,
  re-implement `CornerMap`, and order cell corners TL, TR, BL, BR.
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
**Sev P3 · Effort M · API none**

- **Problem.** `tests/accuracy.rs` bounds σ̂₀² from above only. No lower bound
  (over-conservative covariances pass), no χ²₂ coverage (fraction of corners
  with `d² ≤ 5.99`), no calibration per σ bin; `variance_factor_rank_corr` is
  computed but not asserted; the noise estimate is printed, not compared to the
  rendered σ. Corners whose error exceeds `MATCH_RADIUS_PX` (1 px) drop out of
  both RMSE and σ̂₀², so an overconfident gross error is invisible.
- **Fix.** A coverage band and a two-sided σ̂₀² band per family × refiner;
  count correctly labelled corners beyond the match radius as their own metric.

## <a id="b-19"></a>B-19 · Upstream `chess-corners` findings from the real-data localizer study
**Sev P3 · Effort S · API none**

Filed upstream as
[chess-corners-rs#135](https://github.com/VitalyVorobyev/chess-corners-rs/issues/135),
[#136](https://github.com/VitalyVorobyev/chess-corners-rs/issues/136) and
[#137](https://github.com/VitalyVorobyev/chess-corners-rs/issues/137); delete
this item when they are resolved.
- **The center-of-mass covariance ranks real corners backwards.** On most real
  sets its σ correlates *negatively* with the held-out residual.
- **A saddle-point fit that does not converge is still accepted.** It sits at
  an integer-grid position and is not handed to the `KeepCenterOfMass`
  fallback.
- **The `edge_width` estimate saturates on blurred corners.** It compresses a
  wide range of measured edge blur into a narrow one, so an adaptive window
  driven by it would barely adapt.

## <a id="b-20"></a>B-20 · Hand-maintained mirrors next to the generated schemas
**Sev P2 · Effort M · API internal-safe**

- **Problem.** `schemas/*.json` are generated, but field metadata is restated by
  hand in `crates/calib-targets-studio/src/routes/params_schema.rs`,
  `crates/calib-targets-py/python/calib_targets/config.py` and
  `crates/calib-targets-wasm/typescript-extras.d.ts`.
- **Fix.** Generate the field metadata from the schemars output.

## <a id="b-21"></a>B-21 · Two parallel recovery engines
**Sev P2 · Effort L · API internal-safe**

- **Problem.** The chessboard's `pipeline/boosters.rs` + `recover.rs` and
  `projective-grid`'s `RecoverySchedule` implement the same boundary-extension /
  interior-fill idea twice.
- **Fix.** Converge on one engine.

## <a id="b-22"></a>B-22 · Overlay renderer sprawl
**Sev P3 · Effort M · API none**

- **Problem.** Overlays are drawn by the bench `overlay.rs`,
  `tools/plot_*_overlay.py`, `crates/calib-targets-py/examples/overlay_*.py` and
  `scripts/render_*_overlays.py`.
- **Fix.** Consolidate on the bench overlay plus one shared Python helper.

## <a id="b-23"></a>B-23 · Unreferenced scripts and tools
**Sev P3 · Effort S · API none**

- **Problem.** Nothing references `scripts/puzzlesnap_overlay.sh`,
  `scripts/puzzlesnap_plot.sh`, `scripts/run-topo-bench.sh`,
  `scripts/topological_report.py`, `tools/stereodata.py`, `tools/runxfeat.py`,
  `tools/render_opencv_charuco_labeled.py`.
- **Fix.** Delete them or move them under `research/`.

## <a id="b-24"></a>B-24 · Source files over ~1000 lines
**Sev P3 · Effort M · API internal-safe**

- **Problem.** Past the ~1000-line split rule: `calib-targets-wasm/src/lib.rs`,
  `projective-grid/src/topological/square_detector.rs`,
  `projective-grid/src/orient.rs`,
  `calib-targets-puzzleboard/src/detector/pipeline.rs`,
  `calib-targets-print/src/render.rs`, `calib-targets-py/src/lib.rs`,
  `projective-grid/src/shared/validate/wrong_label_filters.rs`,
  `calib-targets-print/src/render_dxf.rs`, `calib-targets-aruco/src/scan.rs`.
- **Fix.** Split each along its natural seams.

## <a id="b-25"></a>B-25 · Vestigial algorithm slug in the bench
**Sev P3 · Effort S · API none**

- **Problem.** `ALGORITHM_SLUG` in `crates/calib-targets-bench/src/bin/bench/cli.rs`
  and the `algorithm` parameter of `scripts/evaluate_topo_grid_regression.py`
  carry a single constant ("topological"): the bench has one grid builder.
- **Fix.** Drop both at the next baseline re-bless.

## <a id="b-26"></a>B-26 · PuzzlePole lives in the PuzzleBoard crate
**Sev P3 · Effort M · API semver**

- **Problem.** `crates/calib-targets-puzzleboard/src/pole/` is a separate target
  family inside the PuzzleBoard crate.
- **Fix.** Optionally split it into its own crate (lowest priority).
