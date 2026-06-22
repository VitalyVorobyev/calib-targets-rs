---
name: gap16-smooth-warp-backstop
description: Gap 16 global smooth-warp precision backstop was investigated and found NOT precision-safe — measured evidence (LOO residuals on GeminiChess1) falsifies the premise. Do not re-attempt without a new feature.
metadata:
  type: project
---

# Gap 16 — global smooth-warp precision backstop: NOT SAFE (measured 2026-06-22)

Investigated implementing the Gap-16 backstop in
`crates/projective-grid/src/shared/validate/recovery.rs::drop_set`: fit a global
biquadratic (6-term `1,u,v,u²,uv,v²`) warp `(i,j)→pixel` over the labelled set
(affine fallback when sparse), drop corners whose reprojection residual is a high
robust outlier. **Conclusion: a global low-order smooth-warp residual cannot serve
as a precision backstop. Do not land it.**

**Why (measured, not narrative).** On the Gap-15 witness `testdata/02-topo-grid/GeminiChess1.png`,
I reconstructed the pre-Gap-15 labelled set (53 corners incl. the known false
positive `(0,2)` at pixel ≈(210.7,163.6), the left-edge leaf one cell past the true
board edge) and fit the warp.

- **Plain biquadratic residual** of the false positive: r=3.85px, r/cell=0.078,
  z=−1.00 — bottom third of all residuals. Legitimate barrel-distorted periphery
  corners reach r=20.5px (z=3.40).
- **Leave-one-out (leverage-corrected) residual** of the false positive: loo=4.73px,
  loo/cell=0.096, z=−0.96 — **THIRD-SMALLEST of all 53 corners.** Legitimate corners
  `(7,0)`/`(7,6)`/`(1,0)` have LOO ≈ 25–29px (z 4.5–5.5), ~6× larger.

A one-cell-past-the-edge false leaf sits almost exactly where the lattice
projectively extrapolates, so any smooth low-order global polynomial predicts it
with a TINY residual. The Gap-15 signal is local second-order (spacing kink in its
own grid line); a global polynomial averages that kink away. No threshold separates
false from true: dropping the false corner would drop ~50 legitimate corners on the
same frame.

**Also unsafe on clean frames.** GeminiChess2/3 fit the biquadratic to med≈0.3–0.4px
with tiny MAD, so LEGITIMATE corners hit z=6.5–8.0 purely from the small robust
scale. A robust z-gate low enough to be useful elsewhere would manufacture false
DROPS here → recall regression on the ratchets. Classic "simultaneously too loose
and too tight" (CLAUDE.md forbids tuning around this).

**Recommendation.** Keep Gap 15's local second-order line-spacing criterion (criterion 4
in `topological_wrong_label_drops`). The premise that a single global warp subsumes
Gap 8/11/15 is falsified for the one-cell-past-edge class. Any future Gap-16 attempt
must use a feature that actually separates the classes — a LOCAL spacing/curvature
predicate (which Gap 15 already is), not a global residual. The recall half of Gap 16
(recovering the legitimate-but-unreconstructed bottom-left frontier corner on
GeminiChess1) is a separate problem and not addressed by a global-residual gate.

Reproduce: throwaway `crates/calib-targets-bench/examples/gap16_probe.rs` (deleted)
+ temporary `GAP16_DISABLE_CRIT4` env hook in `topological_wrong_label_drops`
(reverted). Both removed; tree left clean at HEAD.
