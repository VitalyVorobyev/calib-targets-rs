# ChESS corner detection

> Front-end crate: [`chess-corners`](https://crates.io/crates/chess-corners)
> (external). This page documents the **feature-input contract** the
> workspace builds on — not an algorithm this workspace re-implements.

Every detector in the workspace starts from a cloud of **ChESS
X-junction corners** — the sub-pixel saddle points where four
chessboard squares meet. Corner finding itself is out of scope here:
the workspace consumes a corner cloud and recovers structure from it.
What matters downstream is the precise *shape* of each corner, because
the [axis clustering](algo_axis_clustering.md) and
[topological grid](algo_topological_grid.md) stages read it directly.

## The per-corner contract

Each detected corner carries:

- **Sub-pixel position** — image-frame pixel coordinates, origin
  top-left, x right, y down. Not rounded to integer pixels.
- **Two undirected local axes** — `axes: [AxisEstimate; 2]`, the two
  orthogonal grid directions visible in the corner's immediate
  neighbourhood. Each `AxisEstimate` is an `angle ∈ [0, π)` plus a 1σ
  angular uncertainty (`sigma`).
- **Quality scalars** — `strength` (the ChESS response magnitude),
  `contrast` (local light/dark separation), and `fit_rms` (the residual
  of the corner-model fit). These feed the prefilter: a corner is kept
  when `strength ≥ min_corner_strength` **and**
  `fit_rms ≤ max_fit_rms_ratio · contrast`.

In the workspace's shared types this corner is
`calib_targets_core::AxisEstimate` carried on a detector-specific input
type (e.g. `calib_targets_chessboard::ChessCorner`).

## Orientation is axes-only

This is the load-bearing contract for everything downstream:

- There is **no single-orientation field**. `Corner::orientation` was
  removed workspace-wide and must never be reintroduced. The only
  orientation signal is the two-axis pair.
- The two axes are **undirected**: an angle `θ` and `θ + π` denote the
  same direction, so all axis comparisons work modulo π.
- The axes are stored in fixed slots (`axes[0]`, `axes[1]`). The slot
  ordering encodes a local parity that adjacent chessboard corners
  flip — the [topological grid finder](algo_topological_grid.md) and the
  chessboard wrapper's parity discipline both depend on it.
- A default-constructed / no-information axis carries `sigma = π` (the
  no-info sentinel) and is filtered out before it can vote.

**Circular means over axis angles** must therefore accumulate
`(cos 2θ, sin 2θ)` and halve the resulting `atan2`. Doubling the angle
wraps `θ` and `θ + π` onto the same point, so the mean is stable across
the 0°/180° seam; naive `(cos θ, sin θ)` averaging collapses to zero
when votes straddle the wrap.

## Orientation modes (DiskFit / RingFit)

The upstream detector exposes two axis-fitting modes, both still
selectable through the facade / Studio / bench:

- **`RingFit`** orders the two axis slots consistently by construction.
- **`DiskFit`** can uniformly pick the wrong antipodal dark sector,
  reversing a corner's `(axes[0], axes[1])` slot ordering relative to the
  board. The chessboard pipeline detects and repairs this globally in its
  [axis-clustering stage](algo_axis_clustering.md) (the *slot-coherence
  repair*), so a `DiskFit` swap never breaks the parity invariant.

## Why the workspace stops at "corners in"

Corner *finding* is image processing; lattice *recovery* is geometry.
Keeping the boundary here lets the geometry crates stay image-free and
reusable: anything that can supply oriented point features — a different
corner detector, a blob detector with a local-orientation estimate, a
laser-dot extractor — can drive the same grid recovery. See
[The Grid Model](projective_grid.md) for the generic feature shapes.

## Cross-references

- [Axis clustering](algo_axis_clustering.md) — the first consumer of the
  dual-axis signal.
- [The Grid Model](projective_grid.md) — the generic `OrientedFeature`
  shapes ChESS corners are adapted into.
- [Conventions](conventions.md) — the coordinate / orientation
  conventions in full.
