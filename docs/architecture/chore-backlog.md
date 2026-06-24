# Chore Backlog — Detection Stack Consolidation

> The actionable output of the [critique](critique.md): a ranked ledger of
> evidence-backed cleanup items. **Status: the consolidation is essentially
> complete.** C-1 through C-8 have all merged; C-5 (this item) lands as a
> docs PR that reframes the advanced tier and documents the core-vs-extended
> boundary; C-9 was **deliberately deferred** (see its section for the
> rationale). Each item is tagged with effort, regression risk, blast radius,
> and **API impact** (because `projective-grid`, `core`, and the detectors are
> published — semver matters).

Legend — **Effort:** S (<½ day) · M (1–3 days) · L (multi-PR). **Risk:** regression
risk. **API:** `internal-safe` (no published-surface change) · `semver` (breaking,
stage with deprecations) · `additive` (new items only) · `feature` (new cfg flag).

## Recommended execution order

Legibility quick-wins first, then the one high-value structural fix, then the large
migration once the surface is clearer. Optional/strategic last.

| Order | Item | What | Sev | Effort | Risk | API | Status |
|---|---|---|---|---|---|---|---|
| 1 | [C-8](#c-8) | Sanitize private-dataset specifics in source comments | P3 | S | Low | none | ✅ DONE |
| 2 | [C-3](#c-3) | Rename `recovery`/`validation` modules by role | P2 | S | Low | internal-safe¹ | ✅ DONE |
| 3 | [C-6](#c-6) | Share the complete-square-cell enumeration | P2 | S | Low | additive | ✅ DONE |
| 4 | [C-4](#c-4) | Delete the single-variant `SquareAlgorithm` seam | P3 | S | Low | semver² | ✅ DONE |
| 5 | [C-1](#c-1) | Unify the forked homography (single source of truth) | **P1** | M | Med | internal-safe | ✅ DONE |
| 6 | [C-5](#c-5) | Decide what `pg`'s advanced tier *is* | P2 | M | Low | semver/doc | 📝 THIS PR (option a) |
| 7 | [C-7](#c-7) | Retire or justify the legacy ChArUco matcher | P3 | M | Med | semver | ✅ DONE |
| 8 | [C-2](#c-2) | Finish the `GridCoords → Coord` migration | **P1** | L | Med-High | semver | ✅ DONE |
| 9 | [C-9](#c-9) | Feature-gate the library-only breadth (optional) | — | M | Med | feature | ⏸️ DEFERRED |

¹ The `pg shared/*` renames touch the *advanced tier*, which `lib.rs` documents
as "advanced, may evolve" (per C-5); the one public rename (`charuco
validation.rs`) needs a deprecation alias. ² Removes a `pub enum` — trivially
breaking; C-4 took the delete-now option.

---

## <a id="c-1"></a>C-1 · Unify the forked homography — DONE
**Severity P1 · Effort M · Risk Med · API internal-safe (if names preserved)**

- **Outcome:** done. The pure DLT solver now lives once in `pg geometry`; `core`
  re-implements `estimate_homography_rect_to_img` as a thin wrapper over it and
  keeps only the image-domain extras. Public names preserved; both regression
  sets stayed green.


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

## <a id="c-2"></a>C-2 · Finish the `GridCoords → Coord` migration — DONE
**Severity P1 · Effort L · Risk Med-High · API semver**

- **Outcome:** done (PR #68). `pg Coord` is the sole grid-coordinate type; the
  `core GridCoords` model and the `grid_alignment_*_to_next` coordinate shims
  are deleted. (The `homography_to_next` / `homography_from_next` *homography*
  bridge is a separate, intentionally-retained seam, not part of this
  coordinate migration.)


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

## <a id="c-3"></a>C-3 · Rename `recovery`/`validation` modules by role — DONE
**Severity P2 · Effort S · Risk Low · API internal-safe¹**

- **Outcome:** done. `pg shared/recovery.rs` → `recovery_schedule.rs`,
  `pg shared/validate/recovery.rs` → `wrong_label_filters.rs`, the charuco
  renames applied, with a deprecation re-export on the one public name.


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

## <a id="c-4"></a>C-4 · Delete the single-variant `SquareAlgorithm` seam — DONE
**Severity P3 · Effort S · Risk Low · API semver (trivial)**

- **Outcome:** done. The recommendation to delete was taken: the single-variant
  `pg detect.rs::SquareAlgorithm` and `chessboard`'s `GraphBuildAlgorithm`
  selector enums, the `with_algorithm` builder, and the synthetic bench
  `AlgorithmReq` / `AlgorithmArg` seam are all removed. `DetectionParams` stays
  `#[non_exhaustive]` for headroom; the topological path is inlined and what to
  detect is selected by `LatticeKind` + `Evidence`, not an algorithm enum.
  Re-introduce a strategy enum only when a second builder actually lands.
- **Original problem:** [D-4](critique.md#d-4-deadspeculative-seams-kept-for-later).
  `pg detect.rs::SquareAlgorithm` had one variant; the `with_algorithm` builder and
  the bench `AlgorithmReq` seam scaffolded a builder that did not exist.

## <a id="c-5"></a>C-5 · Decide what `pg`'s advanced tier *is* — THIS PR (option a)
**Severity P2 · Effort M · Risk Low · API semver/doc**

- **Outcome:** **option (a) — embrace it as public.** This docs PR reframes
  `pub mod shared` / `pub mod topological` (plus `lattice` / `orient` /
  `cluster`) from an apologetic "semver-exempt private engine" into an
  intentional, documented **composition API**, and writes down the composition
  contract: a consumer supplies a `shared::grow::SquareAttachPolicy`, drives the
  growth / recovery primitives (`grow` / `fill` / `extension` / `grow_extend` /
  `recovery_schedule`), and composes the shared back-half (`merge` / `validate`
  / fit), with the drop-only **zero-wrong-labels** guarantee carried over. The
  stable facade tier carries normal semver; the advanced tier is framed as
  "advanced, may evolve" — a deliberate product choice, not a hedge. No public
  API surface changed (no `pub`→`pub(crate)` churn, no feature flags). Same PR
  also documents the core-vs-extended boundary (the C-9 replacement, below).
- **Original problem:** [D-5](critique.md#d-5-the-advanced-tier-is-a-private-api-wearing-a-pub-badge).
  `pub mod shared` / `pub mod topological` were "semver-exempt" yet published —
  both a public API and chessboard's engine.
- **Why not option (b):** the chessboard detector is the reference consumer, but
  the engine composes cleanly for external consumers too; moving it to a
  non-published `*-engine` crate would foreclose that without a measured reason.

## <a id="c-6"></a>C-6 · Share the complete-square-cell enumeration — DONE
**Severity P2 · Effort S · Risk Low · API additive**

- **Outcome:** done — but the framing changed on inspection. The two
  `build_corner_map` functions turned out to be genuinely different operations
  (marker's is a trivial map over a non-optional grid; charuco's is an
  inlier-filtered loop over `&[LabeledCorner]` with an optional grid), so
  merging *them* would be a worse abstraction and they stayed separate. The
  real duplication was the **complete-square-cell enumeration** shared by
  charuco's `build_marker_cells` and marker's `detect_circles_via_square_warp`
  (fold corner-map keys into a bbox; per cell form the four `g00/g10/g11/g01`
  keys, skip if any missing, assemble `[p00,p10,p11,p01]` in TL,TR,BR,BL). That
  was extracted into a new `calib-targets-core::corner_map` module.
- **Original problem:** [D-3](critique.md#d-3-two-validations-two-corner-maps),
  the "two `build_corner_map`" strand.

## <a id="c-7"></a>C-7 · Retire or justify the legacy ChArUco matcher — DONE
**Severity P3 · Effort M · Risk Med · API semver**

- **Outcome:** retired. The regression contracts showed the legacy vote matcher
  produced wrong-ids and lower recall than the board-level soft-LL matcher (already
  the default), so the decision was *delete*. The legacy vote solver
  (`alignment.rs::solve_alignment` + `detector/alignment_select.rs`), the
  `use_board_level_matcher` flag, and the `MatcherDiagKind` /
  `ComponentDiagnostics.matcher` diagnostics were removed; `alignment.rs` now holds
  only the `CharucoAlignment` result type, and the board-level matcher is the sole
  matcher. Source-breaking (`0.x` consolidation); recorded in the CHANGELOG
  `Unreleased` → `Breaking` section.
- **Blast radius:** `charuco` (+ WASM type / Python overlay parity).

## <a id="c-8"></a>C-8 · Sanitize private-dataset specifics in source comments — DONE
**Severity P3 · Effort S · Risk Low · API none**

- **Outcome:** done. The `charuco detector/params.rs` default-constant comments
  that cited concrete private-dataset board sizes / frame counts were rewritten
  as general statements ("tuned on internal ArUco/AprilTag regression sets; the
  minimum that clears them with zero wrong-ids"); concrete figures live only in
  local-only notes. Comments only — zero behaviour change.
- **Original problem:** [D-6](critique.md#d-6-a-superseded-legacy-fallback-and-private-dataset-specifics-in-source),
  against [`private-dataset-policy.md`](../development/private-dataset-policy.md).

## <a id="c-9"></a>C-9 · Feature-gate the library-only breadth — DELIBERATELY DEFERRED
**Severity — · Effort M · Risk Med · API feature**

- **Decision: deferred, in favour of documenting the boundary** (done in the
  C-5 PR). The cost/benefit did not justify the change:
  - **Benefit is unmeasured.** The claimed wins — smaller compile time, a
    narrower default surface — have not been quantified; the extended code is a
    bounded fraction of the crate, and gating it imposes a CI feature-matrix
    obligation (every combination must build) for no demonstrated payoff.
  - **Cost is a semver break on a published crate.** Moving `Hex`,
    `Evidence::Positions`, the `orient::*` helpers, and the recovery schedule
    behind off-by-default features would break every downstream user who
    composes them today.
  - The breadth is **intended product** and stays compiled in either way.
    Making the core-vs-extended split *legible* (the table in
    `crates/projective-grid/docs/DESIGN.md` and the "Core vs. extended surface"
    section in `pg lib.rs`) achieves the comprehension goal without the breaking
    change.
- **Revisit only** if compile time or surface size becomes a *measured*, felt
  cost.
- **Original problem:** [What the breadth costs](critique.md#what-the-breadth-costs).
  Hex and orientation-free paths are always compiled though no in-workspace
  detector uses them.

---

## What this backlog deliberately does **not** propose

- **No rewrite, no rearchitecture.** The crate DAG stays
  ([critique: what's good](critique.md#what-is-genuinely-good-dont-fix-these)).
- **No deletion of hex / dot-grid / orientation-free** algorithms — they are intended
  published library scope ([layering](dependency-and-layering.md#the-library-only-surface)).
- **No detector-decode changes** — `aruco`/`puzzle`/`marker` decode layers are clean.
- **No tuning of constants to examples** — any constant touched (e.g. in C-7/C-8) is
  reasoned from principle, per the workspace anti-overfitting rule.
