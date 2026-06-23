# Architecture Critique

> A critical, evidence-anchored review of the detection stack — where the
> redundancy, duplication, overengineering, and naming debt actually are, and
> whether the structure is worth keeping. Each finding cites `file.rs::symbol`.
> Actionable items are tracked in [`chore-backlog.md`](chore-backlog.md).

**Framing decision (from the owner):** `projective-grid` is a **standalone
published library**. Its hex / dot-grid / orientation-free breadth is *intended
product surface for external users*, not bloat to delete. That reframes this whole
review: the question is **not** "is the scope too big?" (it is deliberately big) but
"is the workspace carrying *avoidable* duplication, frozen migrations, and naming
debt on top of that scope?" It is — and that, not the breadth, is what makes the
stack hard to grasp.

---

## What is genuinely good (don't "fix" these)

A critical review has to start by protecting what works, so cleanup doesn't damage it.

- **The crate DAG is clean.** No cycles; sensible layering (L0 `projective-grid` →
  L1 `core` → L2 `aruco`/`chessboard` → L3 composed detectors). See
  [`dependency-and-layering.md`](dependency-and-layering.md). Almost every problem
  below is *within* a crate, not in the boundaries.
- **One grid spine, shared by embedding — not copy-paste.** `charuco`/`puzzle`/
  `marker` get the grid by embedding `chessboard`, not by reimplementing it
  ([pipeline-maps](pipeline-maps.md)). That is the *right* kind of reuse.
- **The decode layers are cohesive and duplication-free.** `aruco/src/scan.rs`
  (the codec), `puzzle` hard/soft+CRT decode, and `marker` circle scoring are each
  self-contained, single-responsibility, and share nothing they shouldn't.
- **The precision contract is enforced structurally.** Every check in
  `pg shared/validate/*` is *drop-only* (a corner is removed, never relabelled),
  which is exactly how you honour "misses are OK, false positives are not".
- **API hygiene is consistently high.** `#[non_exhaustive]` + named constructors on
  public types; no `todo!()`/`unimplemented!()` debris; clean clippy under
  `-D warnings`. The detector public surfaces (`charuco`/`marker`/`aruco` `lib.rs`,
  ~44–64 LOC each) are tight.

This is *not* a codebase that needs a rewrite. It needs **consolidation**.

---

## Findings

Severity: **P1** worth doing soon · **P2** worth doing · **P3** judgment / nice-to-have.

### <a id="d-1-homography-is-forked-verbatim"></a>D-1 Homography is forked verbatim
**Severity: P1 · the top finding.**

`pg geometry/homography.rs::estimate_homography` and
`core homography.rs::estimate_homography_rect_to_img` are the **same algorithm,
copied**: identical Hartley normalization → `AᵀA` accumulation → `symmetric_eigen`
→ denormalize → scale-fix, down to the variable names and the 4-point LU variant.
The differences are cosmetic: the numeric bound (`Float` vs `RealField`), the
function name, and the image-domain extras `core` adds (`warp_perspective_gray`,
`homography_to_next`/`from_next`). ~250 LOC of numerically-sensitive code maintained
in two places.

- **Why it exists:** `core → projective-grid` in the DAG, so `pg` (below `core`)
  cannot reuse `core`'s solver without a cycle. It forked instead.
- **Who uses which:** *every detector* uses `core`'s copy (`charuco
  corner_validation.rs`, `chess rectified_view.rs` + `mesh_warp.rs`); only `pg`'s own
  `shared::fit` uses `pg`'s copy.
- **Why it's debt, not duplication-by-design:** two hand-maintained copies of a DLT
  *will* drift (one gets a conditioning fix, the other doesn't), and that drift is
  silent and numerical — the worst kind.
- **Fix (direction fixed by the DAG):** make the lowest crate the single source of
  truth — keep the pure solver in `pg geometry`, have `core` re-export it and add
  only its image-coupled helpers on top. Internal-safe on the `pg` side; the `core`
  re-export must preserve the existing public names (`estimate_homography_rect_to_img`)
  to stay non-breaking. → [backlog C-1](chore-backlog.md#c-1).

### <a id="d-2-the-gridcoords--coord-migration-is-frozen-mid-flight"></a>D-2 The `GridCoords → Coord` migration is frozen mid-flight
**Severity: P1 · the biggest comprehension tax.**

Two grid-coordinate models coexist: `core grid_alignment.rs::GridCoords{i,j}` /
`GridAlignment` (legacy) and `pg lattice::Coord{u,v}` (current), bridged by adapters
literally named `grid_alignment_to_next` / `grid_alignment_from_next` — and the same
`*_to_next`/`*_from_next` idiom appears for homography
(`core homography.rs::homography_to_next`). "The next crate's representation" is
migration vocabulary frozen into the public API; a reader cannot tell which model is
canonical, which is why the stack "can't be grasped".

- **Why it's debt:** a half-finished migration is strictly worse than either
  endpoint — every consumer must understand *both* models and the shims between them.
- **Fix:** pick `Coord` as canonical, migrate `core` + `chessboard` onto it, delete
  the `*_to_next`/`*_from_next` shims. Larger and partly semver-breaking (public
  `core` types change) — stage it. → [backlog C-2](chore-backlog.md#c-2).

### <a id="d-3-two-validations-two-corner-maps"></a>D-3 Two validations, two corner maps, two `recovery.rs`
**Severity: P2 · naming/observability tax (each is small; together they confuse).**

Genuinely-distinct logic given near-identical names — the reader pays to
disambiguate every time:

- **Two `recovery.rs` in one crate:** `pg shared/recovery.rs` (the *schedule* —
  orchestrates extend→fill→validate→drop) vs `pg shared/validate/recovery.rs` (the
  *drop-filters* it calls). Not duplication — but two files named `recovery` one
  directory apart is a comprehension trap.
- **Two "validation" in charuco:** `charuco detector/corner_validation.rs`
  (`validate_and_fix_corners` — internal homography refit + ROI re-detect) vs
  `charuco validation.rs` (`validate_marker_corner_links` — *public* post-hoc check
  against the board spec). Different inputs, different audience, same word.
- **Two `build_corner_map`:** `charuco detector/marker_sampling.rs` and
  `marker detector.rs` each implement "chessboard grid → grid→pixel map" in parallel.
  Real (if small) duplication of the same concept.

- **Fix:** rename by role (`recovery_schedule.rs` vs `wrong_label_filters.rs`;
  `corner_refit.rs` vs `link_check.rs`); lift one `build_corner_map` into
  `chessboard` (or `core`) and share it. Mostly internal-safe renames; the public
  `charuco validation.rs` name needs a deprecation alias if changed. →
  [backlog C-3](chore-backlog.md#c-3), [C-6](chore-backlog.md#c-6).

### <a id="d-4-deadspeculative-seams-kept-for-later"></a>D-4 Dead/speculative seams kept "for later"
**Severity: P3 · low cost, but pure comprehension drag.**

- `pg detect.rs::SquareAlgorithm` is a **single-variant `#[non_exhaustive]` enum**
  (`Topological`). Every call site passes that one value; the
  `with_algorithm(...)` builder, the bench's `AlgorithmReq` seam
  (`bench/compare.rs:281` — "reserve a seam for a future alternative builder"), and a
  `match` with one real arm all exist for a builder that was removed (seed-and-grow)
  and a hypothetical future one.
- **Judgment, not a slam-dunk:** for a *published library* a reserved
  `#[non_exhaustive]` enum is a legitimate semver-headroom choice. But today it costs
  every reader a "wait, where are the other algorithms?" detour for zero behaviour.
  Recommend deleting and re-introducing when a second builder actually lands; the
  `#[non_exhaustive]` on the request struct already preserves headroom. →
  [backlog C-4](chore-backlog.md#c-4).

### <a id="d-5-the-advanced-tier-is-a-private-api-wearing-a-pub-badge"></a>D-5 The advanced tier is a private API wearing a `pub` badge
**Severity: P2 · library-design clarity.**

`pg lib.rs` exposes `pub mod shared` and `pub mod topological` as a "semver-exempt
pre-1.0" advanced tier "for in-workspace consumers (the chessboard detector)"
(lib.rs:37–45). The deep reach from `chessboard` into `pg shared::{grow,fill,
validate,merge}` is therefore *sanctioned*, not a leak — but on a **published** crate
a `pub mod` is still public: it lands on docs.rs and external users can build on it,
semver-exempt or not.

- **The tension:** it is simultaneously "chessboard's private engine" and "part of
  the published API". Pick one. Either (a) it is a supported composition API — then
  document the contract (what `SquareAttachPolicy` a consumer must supply, what the
  recovery schedule guarantees) and stop calling it private; or (b) it is
  chessboard-private — then it wants a workspace-internal seam (a `pub(crate)` +
  in-repo path, or a non-published `-engine` crate), not a `pub mod` on the published
  crate. → [backlog C-5](chore-backlog.md#c-5).

### <a id="d-6-a-superseded-legacy-fallback-and-private-dataset-specifics-in-source"></a>D-6 A superseded legacy fallback, and private-dataset specifics in source
**Severity: P3.**

- **Legacy matcher:** *resolved (C-7).* `charuco` previously shipped two marker
  matchers; the legacy rotation+translation vote and its `use_board_level_matcher`
  toggle have been retired, leaving the board-level soft-LL matcher
  (`detector/board_match.rs`) as the sole matcher. `alignment.rs` now holds only the
  `CharucoAlignment` result type.
- **Hygiene:** `charuco detector/params.rs` default-constant comments cite concrete
  private-dataset board sizes and frame counts to justify tuned constants. Per
  [`private-dataset-policy.md`](../development/private-dataset-policy.md) concrete
  numbers belong in local-only surfaces; source comments compiled into the published
  crate are borderline. Sanitize to general statements (this doc deliberately does
  **not** repeat the numbers). → [backlog C-7](chore-backlog.md#c-7),
  [C-8](chore-backlog.md#c-8).

### Watch-list (not findings yet)

- **Large files that are still cohesive:** `pg orient.rs` (~1.08K), `aruco scan.rs`
  (969), `puzzle detector/pipeline.rs` (807). Each is *one* algorithm family today,
  so they pass the "no giant grab-bag files" rule — but they sit at the threshold;
  if a second responsibility lands in any of them, split then.
- **Param-struct surface:** `chess params/` (~583 LOC across `mod.rs` +
  `advanced.rs`) is large but deliberately split into a 3-knob stable core and an
  opt-in `#[non_exhaustive]` `AdvancedTuning`. Acceptable; just keep new knobs out of
  the stable struct.

---

## <a id="what-the-breadth-costs"></a>What the breadth costs (accepted, not a bug)

Per the owner's decision the library breadth stays. Stating its cost honestly so the
decision is made with open eyes:

- ~1.08K LOC of orientation synthesis (`orient.rs`), ~0.88K of hex
  (`topological/hex*.rs` + `lattice/hex.rs`), plus the extension + recovery-schedule
  engine, are carried by the workspace but exercised here only by `pg`'s own tests.
- Every refactor of the shared back-half (e.g. C-1, C-5) must keep all `📚` paths
  green, not just the chessboard path — the test matrix is wider than the shipped
  surface.
- **Mitigation, not removal:** keep it, but make the cost legible — feature-gate the
  hex and orientation-free paths (`feature = "hex"`, `feature = "orientation-free"`)
  so external users opt in and in-workspace builds can compile the minimal engine.
  This preserves the product while shrinking the default surface a reader/`chessboard`
  sees. → [backlog C-9](chore-backlog.md#c-9) (optional).

---

## Would I build it this way from scratch?

**The crate structure: ~80% yes. The within-crate state: no.**

Given the same goal — a published generic grid library *plus* a family of target
detectors — I would keep the layering almost exactly (it is a clean DAG, the decode
layers are well-isolated, the grid spine is shared by embedding). What I would *not*
reproduce is the accumulated debris. Six concrete changes turn the current stack into
the one I'd design:

1. **One source of truth for geometry.** Pure homography/projective DLT lives in the
   lowest crate (`pg`); `core` re-exports and adds image-domain helpers. Kill the
   fork. *(D-1 / C-1)*
2. **Finish the coordinate migration.** `Coord` is canonical; `GridCoords` and the
   `*_to_next`/`*_from_next` shims are deleted. This single change removes most of the
   "I can't tell what's canonical" tax. *(D-2 / C-2)*
3. **Name by role, not by history.** `recovery_schedule` vs `wrong_label_filters`;
   `corner_refit` vs `link_check`; one shared `build_corner_map`. *(D-3 / C-3, C-6)*
4. **Decide what the engine *is*.** Either a documented public composition API or a
   workspace-internal seam — not a "semver-exempt `pub mod`" that is both. *(D-5 / C-5)*
5. **Delete dead seams.** No single-variant `SquareAlgorithm`; re-introduce a
   strategy enum only when a second strategy exists. *(D-4 / C-4)*
6. **Gate the breadth, don't carry it always-on.** Hex and orientation-free behind
   features so the default build and the mental model are the shipped spine. *(C-9)*

None of these is a rearchitecture. They are consolidation + finishing one migration +
renaming — which is exactly why the stack *feels* overengineered while being, at the
crate level, basically sound. The complexity you are feeling is **debt, not design**.

See [`chore-backlog.md`](chore-backlog.md) for the ranked, effort-and-risk-tagged
execution list.
