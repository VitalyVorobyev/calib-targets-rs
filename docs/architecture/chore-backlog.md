# Chore Backlog — Detection Stack Consolidation

> The actionable output of the [critique](critique.md): a ranked ledger of
> evidence-backed cleanup items. **Doc-only as of this snapshot — nothing here is
> executed yet.** Each item is tagged with effort, regression risk, blast radius,
> and **API impact** (because `projective-grid`, `core`, and the detectors are
> published — semver matters).

Legend — **Effort:** S (<½ day) · M (1–3 days) · L (multi-PR). **Risk:** regression
risk. **API:** `internal-safe` (no published-surface change) · `semver` (breaking,
stage with deprecations) · `additive` (new items only) · `feature` (new cfg flag).

## Recommended execution order

Legibility quick-wins first, then the one high-value structural fix, then the large
migration once the surface is clearer. Optional/strategic last.

| Order | Item | What | Sev | Effort | Risk | API |
|---|---|---|---|---|---|---|
| 1 | [C-8](#c-8) | Sanitize private-dataset specifics in source comments | P3 | S | Low | none |
| 2 | [C-3](#c-3) | Rename `recovery`/`validation` modules by role | P2 | S | Low | internal-safe¹ |
| 3 | [C-6](#c-6) | Share one `build_corner_map` | P2 | S | Low | additive |
| 4 | [C-4](#c-4) | Delete the single-variant `SquareAlgorithm` seam | P3 | S | Low | semver² |
| 5 | [C-1](#c-1) | Unify the forked homography (single source of truth) | **P1** | M | Med | internal-safe |
| 6 | [C-5](#c-5) | Decide what `pg`'s advanced tier *is* | P2 | M | Low | semver/doc |
| 7 | [C-7](#c-7) | Retire or justify the legacy ChArUco matcher | P3 | M | Med | semver |
| 8 | [C-2](#c-2) | Finish the `GridCoords → Coord` migration | **P1** | L | Med-High | semver |
| 9 | [C-9](#c-9) | Feature-gate the library-only breadth (optional) | — | M | Med | feature |

¹ The `pg shared/*` renames touch the *advanced tier*, which `lib.rs` declares
semver-exempt; the one public rename (`charuco validation.rs`) needs a deprecation
alias. ² Removes a `pub enum` — trivially breaking; deprecate one release first.

---

## <a id="c-1"></a>C-1 · Unify the forked homography
**Severity P1 · Effort M · Risk Med · API internal-safe (if names preserved)**

- **Problem:** [D-1](critique.md#d-1-homography-is-forked-verbatim). The DLT solver in
  `pg geometry/homography.rs::estimate_homography` is a verbatim copy of
  `core homography.rs::estimate_homography_rect_to_img` (~250 LOC, identical body).
  Two hand-maintained copies of numerically-sensitive code that will drift silently.
- **Fix:** make the lowest crate canonical. Keep the pure solver in `pg geometry`;
  in `core`, delete the duplicated body and re-implement
  `estimate_homography_rect_to_img` as a thin wrapper over `pg`'s solver, retaining
  *only* the image-domain extras (`warp_perspective_gray`, the `Projective2` bridges).
  Preserve every existing public name so callers don't change.
- **Blast radius:** `core homography.rs`, `pg geometry/homography.rs`; all detector
  callers recompile unchanged.
- **Risk control:** the existing homography unit tests in both files + both private
  regression sets must stay green; assert pixel-level parity of the two solvers on a
  fixed correspondence set before deleting the `core` body.
- **Deps:** none. Do before [C-2](#c-2) (shrinks the `core` public surface in flux).

## <a id="c-2"></a>C-2 · Finish the `GridCoords → Coord` migration
**Severity P1 · Effort L · Risk Med-High · API semver**

- **Problem:** [D-2](critique.md#d-2-the-gridcoords--coord-migration-is-frozen-mid-flight).
  Dual coordinate models (`core GridCoords{i,j}` vs `pg Coord{u,v}`) bridged by
  `grid_alignment_to_next`/`_from_next` shims. The single biggest "what's canonical?"
  tax in the stack.
- **Fix:** adopt `pg Coord` as canonical. Migrate `core` adapters and `chessboard`
  (and any `charuco`/`puzzle`/`marker` use of `core` grid types) onto it; delete the
  `*_to_next`/`*_from_next` shims. Provide `#[deprecated]` aliases for the removed
  public `core` types for one minor release.
- **Blast radius:** `core grid_alignment.rs` (public), `chessboard`, transitively the
  L3 detectors. Largest item here.
- **Risk control:** stage in two PRs — (a) internal migration behind deprecated
  aliases, (b) alias removal next minor. Full regression matrix each step.
- **Deps:** after [C-1](#c-1); coordinate the deprecation window with [C-4](#c-4).

## <a id="c-3"></a>C-3 · Rename `recovery`/`validation` modules by role
**Severity P2 · Effort S · Risk Low · API internal-safe¹**

- **Problem:** [D-3](critique.md#d-3-two-validations-two-corner-maps). Two `recovery.rs`
  in `pg shared/` (schedule vs drop-filters); two "validation" concepts in `charuco`.
- **Fix:** `pg shared/recovery.rs` → `recovery_schedule.rs`;
  `pg shared/validate/recovery.rs` → `wrong_label_filters.rs`;
  `charuco detector/corner_validation.rs` → `corner_refit.rs`;
  `charuco validation.rs` → `link_check.rs` (public — add a deprecation re-export).
- **Blast radius:** internal `pg`/`charuco` paths (compiler-checked); `chessboard`'s
  advanced-tier import paths update (semver-exempt tier).
- **Risk control:** pure renames; `cargo build` + `cargo doc` (zero-warning gate)
  catch every miss.

## <a id="c-4"></a>C-4 · Delete the single-variant `SquareAlgorithm` seam
**Severity P3 · Effort S · Risk Low · API semver (trivial)**

- **Problem:** [D-4](critique.md#d-4-deadspeculative-seams-kept-for-later).
  `pg detect.rs::SquareAlgorithm` has one variant; the `with_algorithm` builder and
  the bench `AlgorithmReq` seam scaffold a builder that doesn't exist.
- **Fix:** remove the enum + builder + the `match` with one arm; inline the
  topological path. Keep `DetectionParams` `#[non_exhaustive]` for headroom. Update
  `bench/compare.rs` to drop the synthetic second-builder slug.
- **Blast radius:** `pg detect.rs` (removes a `pub enum`), `chessboard` (one call
  site), `bench`.
- **Judgment call:** legitimate to *keep* as semver headroom on a library. Recommend
  deleting (deprecate one release) — re-add when a real second builder lands.

## <a id="c-5"></a>C-5 · Decide what `pg`'s advanced tier *is*
**Severity P2 · Effort M · Risk Low · API semver/doc**

- **Problem:** [D-5](critique.md#d-5-the-advanced-tier-is-a-private-api-wearing-a-pub-badge).
  `pub mod shared` / `pub mod topological` are "semver-exempt" yet published — both a
  public API and chessboard's private engine.
- **Fix (pick one):** (a) **embrace it as public** — document the composition
  contract (the `SquareAttachPolicy` a consumer supplies, the recovery-schedule
  guarantees), drop the "private" framing; or (b) **make it private** — move the
  engine to a non-published `*-engine` crate (or `pub(crate)` + workspace path),
  leaving only the facade public.
- **Blast radius:** `pg lib.rs` + docs (option a), or a new crate + `chessboard`
  import paths (option b).
- **Recommendation:** option (a) doc-first (cheaper, keeps external composability);
  revisit (b) only if no external consumer composes the engine.

## <a id="c-6"></a>C-6 · Share one `build_corner_map`
**Severity P2 · Effort S · Risk Low · API additive**

- **Problem:** [D-3](critique.md#d-3-two-validations-two-corner-maps).
  `charuco detector/marker_sampling.rs` and `marker detector.rs` each implement
  "chessboard grid → grid→pixel map" in parallel.
- **Fix:** add a `build_corner_map` helper to `chessboard` (it owns
  `ChessboardDetection`) and have both `charuco` and `marker` call it.
- **Blast radius:** `chessboard` (new `pub` helper), `charuco`, `marker`.
- **Risk control:** assert the shared helper reproduces both current maps on a fixed
  detection before deleting the locals.

## <a id="c-7"></a>C-7 · Retire or justify the legacy ChArUco matcher
**Severity P3 · Effort M · Risk Med · API semver**

- **Problem:** [D-6](critique.md#d-6-a-superseded-legacy-fallback-and-private-dataset-specifics-in-source).
  The board-level soft-LL matcher is the default; the legacy vote matcher
  (`alignment.rs` + `alignment_select.rs`, ~300 LOC + the `use_board_level_matcher`
  flag) is an off-by-default fallback.
- **Fix:** **decision first** — does the legacy matcher ever beat the default on any
  frame in the regression sets? If no → delete it (+ the flag/types, deprecated one
  release). If yes → keep it but add a one-line "why we still ship this" rationale at
  the flag definition.
- **Blast radius:** `charuco` only.
- **Risk control:** the data check *is* the gate; don't delete a fallback on a hunch.

## <a id="c-8"></a>C-8 · Sanitize private-dataset specifics in source comments
**Severity P3 · Effort S · Risk Low · API none**

- **Problem:** [D-6](critique.md#d-6-a-superseded-legacy-fallback-and-private-dataset-specifics-in-source).
  `charuco detector/params.rs` default-constant comments cite concrete private-dataset
  board sizes and frame counts, against
  [`private-dataset-policy.md`](../development/private-dataset-policy.md).
- **Fix:** rewrite those comments as general statements ("tuned on internal ArUco/
  AprilTag regression sets; κ is the minimum that clears them with zero wrong-ids");
  move any concrete figures to a local-only note. Grep the workspace for the same
  pattern elsewhere while here.
- **Blast radius:** comments only — zero behaviour change.
- **Why early:** it's a policy-compliance item and trivially safe.

## <a id="c-9"></a>C-9 · Feature-gate the library-only breadth (optional / strategic)
**Severity — · Effort M · Risk Med · API feature**

- **Problem:** [What the breadth costs](critique.md#what-the-breadth-costs). Hex and
  orientation-free paths are always compiled though no workspace detector uses them.
- **Fix:** gate them behind `feature = "hex"` and `feature = "orientation-free"`.
  Keep current defaults *on* to avoid a breaking change, or flip defaults *off* in the
  next minor (semver-sensitive) so the minimal engine is the default mental model.
- **Blast radius:** `pg` cfg matrix + CI (must build all feature combinations).
- **Note:** strategic, not urgent. Only worthwhile if compile time / surface size is
  a felt cost; the breadth itself is intended product and stays either way.

---

## What this backlog deliberately does **not** propose

- **No rewrite, no rearchitecture.** The crate DAG stays
  ([critique: what's good](critique.md#what-is-genuinely-good-dont-fix-these)).
- **No deletion of hex / dot-grid / orientation-free** algorithms — they are intended
  published library scope ([layering](dependency-and-layering.md#the-library-only-surface)).
- **No detector-decode changes** — `aruco`/`puzzle`/`marker` decode layers are clean.
- **No tuning of constants to examples** — any constant touched (e.g. in C-7/C-8) is
  reasoned from principle, per the workspace anti-overfitting rule.
