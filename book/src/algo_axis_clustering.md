# Axis clustering

> Code: `projective_grid::cluster` (`cluster_axes`, `AxisClusterCenters`,
> `AxisAssignment`), re-exported at the crate root.

Axis clustering recovers the **two global grid-direction centres**
`{Θ₀, Θ₁}` (≈ 90° apart) from a set of features that each carry two
undirected local lattice axes (e.g. [ChESS corners](algo_chess_corners.md)).
It is the orientation-prior stage every grid pipeline runs before
building a lattice: the two centres are the only global axis hint handed
to the [topological grid finder](algo_topological_grid.md), and they are
reused later for booster recovery so the clustering runs once.

The module is **pure direction-clustering math** — no image types, no
target vocabulary. Which features are eligible to vote, and how the
canonical/swapped assignment maps onto a caller's own label type, stays
caller-side.

## Input / output

- **Input:** a slice of `AxisFeature`, each carrying its two
  `AxisObservation`s `(angle, sigma)` and a detector `strength`. Axes
  whose `sigma` is the no-info sentinel (`≥ π`) or non-finite are
  skipped; callers pre-filter to the features they want to vote (the
  chessboard passes only its `Strong` corners).
- **Output:**
  - `AxisClusterCenters { theta0, theta1 }` in `[0, π)` with
    `theta0 ≤ theta1`.
  - A per-feature `AxisAssignment` — `Canonical` (axes[0] matches Θ₀),
    `Swapped` (axes[0] matches Θ₁), or `NoCluster` (neither axis is
    close enough to either centre).

## The algorithm

1. **Circular histogram.** Build a smoothed histogram on `[0, π)` with
   `num_bins` bins. For every feature and every axis `k ∈ {0, 1}`, add a
   vote at `wrap_pi(axes[k].angle)` weighted by
   `strength / (1 + axes[k].sigma)` — stronger, more-certain axes vote
   harder.
2. **Smoothing.** Convolve with a `[1, 4, 6, 4, 1] / 16` circular kernel
   so single-bin noise does not masquerade as a peak.
3. **Plateau-aware peak picking.** Find local maxima; keep peaks whose
   total weight is at least `min_peak_weight_fraction × total`; pick the
   two strongest peaks separated by at least `peak_min_separation_rad`.
   "Plateau-aware" matters for a perfectly rectilinear board whose two
   axes land exactly on histogram-bin boundaries — a naive argmax would
   split one true peak into two adjacent bins.
4. **Double-angle 2-means refinement.** Refine the two peak centres with
   k-means (k = 2) in **double-angle space** — each axis angle is mapped
   to `(cos 2θ, sin 2θ)` before clustering, and the cluster means are
   halved back into `[0, π)`. This is the same undirected-mean discipline
   the whole workspace uses; it makes the refinement stable across the
   0°/180° seam.

## Per-feature slot assignment

Once `{Θ₀, Θ₁}` are fixed, each feature is scored against the two
possible slot assignments:

- **Canonical** — cost `d(axes[0], Θ₀) + d(axes[1], Θ₁)`.
- **Swapped** — cost `d(axes[0], Θ₁) + d(axes[1], Θ₀)`.

The cheaper assignment wins; a feature whose worse axis exceeds the
caller's tolerance is labelled `NoCluster` and excluded from voting on
edges. All distances `d` are angular and computed modulo π.

## Why double-angle, not naive circular mean

Axes are undirected, so a histogram vote at `θ` is equally a vote at
`θ + π`. A naive circular mean over raw `(cos θ, sin θ)` of two votes
180° apart sums to **zero** — the mean is undefined exactly where it
matters most. Doubling the angle folds `θ` and `θ + π` onto the same
point on the unit circle, so the mean is well-defined; halving the result
recovers the undirected direction. This contract is mandatory anywhere
the workspace averages axis angles.

## Cross-references

- [ChESS corner detection](algo_chess_corners.md) — the source of the
  dual-axis votes, and the `DiskFit` slot-flip the chessboard repairs
  *after* clustering.
- [Topological grid finder](algo_topological_grid.md) — the consumer of
  the two centres (as an optional per-corner usability gate).
- [Recovery & validation](algo_recovery_validation.md) — reuses the same
  `(features, centres)` pair for booster recovery.
