//! End-to-end square-grid recovery pipeline.
//!
//! Composes the four geometric stages — seed → grow → extend → fill,
//! plus optional post-grow validation — under a single user-facing
//! function, [`detect_square_grid`]. Each stage is an existing
//! pattern-agnostic primitive; this module wires them together and
//! returns one [`SquareGridDetection`] carrying the labelled map plus
//! per-stage diagnostics.
//!
//! ## Pattern hooks
//!
//! The caller supplies two validators:
//!
//! - [`SeedQuadValidator`] decides which 2×2 cells are admissible
//!   seeds (per-pattern parity, axis-slot rules, etc.).
//! - [`GrowValidator`] decides which corners are admissible
//!   attachments during BFS grow, boundary extension, and hole fill.
//!
//! Everything else (KD-tree neighbour search, edge-ratio gate,
//! parallelogram closure, homography fit, line residual check) is
//! pattern-agnostic and runs identically across chessboard, ChArUco,
//! puzzleboard, and any other square-lattice target.

use std::collections::HashMap;
use std::collections::HashSet;

use nalgebra::{Point2, Vector2};

use crate::square::extension::{
    extend_via_global_homography, extend_via_local_homography, ExtensionParams, ExtensionStats,
    LocalExtensionParams,
};
use crate::square::fill::{fill_grid_holes, FillParams, FillStats};
use crate::square::grow::{
    bfs_grow, Admit, FillEdgeCtx, GrowParams, GrowResult, GrowValidator, LabelledNeighbour,
};
use crate::square::seed::{Seed, SeedOutput};
use crate::square::seed_finder::{find_quad, SeedQuadParams, SeedQuadValidator};
use crate::square::validate::{validate, LabelledEntry, ValidationParams, ValidationResult};
use crate::topological::AxisEstimate;

/// Boundary-extension strategy for [`detect_square_grid`].
///
/// Selects which (if any) homography-extension pass runs after BFS
/// grow. The two functional variants wrap the pipeline's two
/// homography-extension strategies — see
/// [`extend_via_global_homography`] and [`extend_via_local_homography`]
/// for the precision / recall trade-off; [`Self::Disabled`] skips the
/// stage entirely.
///
/// `#[non_exhaustive]`: future strategies may be added.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum ExtensionStrategy {
    /// Skip boundary extension. The labelled set returned by BFS-grow
    /// stands as-is.
    Disabled,
    /// Fit one global homography over the whole labelled set and
    /// extrapolate from it. See [`extend_via_global_homography`].
    Global(ExtensionParams),
    /// Fit a per-candidate local homography from the nearest labelled
    /// corners. See [`extend_via_local_homography`].
    Local(LocalExtensionParams),
}

impl Default for ExtensionStrategy {
    fn default() -> Self {
        // Matches the historical default — a single global-H pass with
        // the upstream module's own defaults.
        ExtensionStrategy::Global(ExtensionParams::default())
    }
}

/// Combined tuning knobs for [`detect_square_grid`].
///
/// The post-grow fill and validate sub-stages are each gated on their
/// `Option<>` field (passing `None` skips them); boundary extension is
/// selected by the [`ExtensionStrategy`] enum. Default constructor runs
/// every stage with the upstream module's own defaults.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct SquareGridParams {
    /// Seed-finder gates. A 2×2 seed must be found before any
    /// other stage runs.
    pub seed: SeedQuadParams,
    /// BFS-grow tuning. See [`GrowParams`].
    pub grow: GrowParams,
    /// Boundary-extension strategy. See [`ExtensionStrategy`];
    /// [`ExtensionStrategy::Disabled`] skips the stage.
    pub extension: ExtensionStrategy,
    /// Interior-hole + line-extrapolation fill pass. `None` skips
    /// the stage.
    pub fill: Option<FillParams>,
    /// Post-grow line + local-H residual checks. When `Some`, every
    /// corner flagged by [`validate`] is dropped from the final
    /// labelled set. `None` skips the stage and returns the raw
    /// post-fill labels.
    pub validate: Option<ValidationParams>,
}

impl Default for SquareGridParams {
    fn default() -> Self {
        Self {
            seed: SeedQuadParams::default(),
            grow: GrowParams::default(),
            extension: ExtensionStrategy::default(),
            fill: Some(FillParams::default()),
            validate: Some(ValidationParams::default()),
        }
    }
}

/// Final outcome of [`detect_square_grid`].
///
/// `labelled` is the canonical product — every entry maps a `(i, j)`
/// grid cell to a corner index in the caller's `positions` slice.
/// The bounding box of the labelled set is rebased so the minimum
/// `(i, j)` is `(0, 0)`.
#[non_exhaustive]
#[derive(Debug)]
pub struct SquareGridDetection {
    /// `(i, j) → corner_idx` map for every recovered corner.
    pub labelled: HashMap<(i32, i32), usize>,
    /// Inverse map: `corner_idx → (i, j)`.
    pub by_corner: HashMap<usize, (i32, i32)>,
    /// Pixel-space unit vector along the grid's `i` direction,
    /// inferred from the seed quad. Useful for downstream overlays.
    pub grid_u: Vector2<f32>,
    /// Pixel-space unit vector along the grid's `j` direction.
    pub grid_v: Vector2<f32>,
    /// Cell size in pixels, taken from the seed quad's mean edge
    /// length. Approximate under non-uniform perspective; downstream
    /// metric work should refit a homography from the labelled set.
    pub cell_size: f32,
    /// Per-stage diagnostic counters.
    pub stats: SquareGridStats,
}

/// Per-stage counters returned alongside [`SquareGridDetection`].
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct SquareGridStats {
    /// Corner indices of the chosen seed quad, in `[A, B, C, D]`
    /// order. `None` indicates seed finding failed (in which case
    /// [`detect_square_grid`] returns `None` and this struct is not
    /// produced).
    pub seed: Option<[usize; 4]>,
    /// Number of corners attached during the BFS grow stage,
    /// excluding the four seed corners.
    pub grown: usize,
    /// Boundary-extension diagnostics. `None` when extension was
    /// disabled via `SquareGridParams::extension = None`.
    pub extension: Option<ExtensionStats>,
    /// Hole-fill diagnostics. `None` when fill was disabled or the
    /// labelled set was empty before fill ran.
    pub fill: Option<FillStats>,
    /// Validation outcome. `None` when validation was disabled.
    pub validation: Option<ValidationResult>,
    /// Number of corners dropped from `labelled` because validation
    /// flagged them. Always `0` when validation was disabled.
    pub dropped_by_validation: usize,
}

/// User-facing entry point for the square-lattice grid pipeline.
///
/// Runs five stages end-to-end:
///
/// 1. **Seed** — [`find_quad`] picks a 2×2 cell whose four corners
///    pass every `seed_validator` gate plus the pattern-agnostic
///    geometric checks (axis alignment, edge-ratio match,
///    parallelogram closure, no midpoint violation). The seed's
///    mean edge length defines `cell_size` for the downstream
///    stages.
/// 2. **Grow** — [`bfs_grow`] walks the lattice from the seed,
///    attaching the nearest admissible corner at each unlabelled
///    cardinal neighbour. Pattern rules flow through
///    `grow_validator`.
/// 3. **Extend** — fits a homography to the labelled set and
///    extrapolates the labelled boundary outward. Selected by
///    `params.extension`: [`ExtensionStrategy::Global`] runs
///    [`extend_via_global_homography`],
///    [`ExtensionStrategy::Local`] runs
///    [`extend_via_local_homography`], and
///    [`ExtensionStrategy::Disabled`] skips the stage.
/// 4. **Fill** — [`fill_grid_holes`] sweeps the bounding box for
///    cells with ≥ 2 cardinal labelled neighbours and attaches
///    candidates that survive a per-cell predictor. Gated on
///    `params.fill` being `Some`.
/// 5. **Validate** — [`validate`] applies a line-fit + local-H
///    residual check. Corners in the resulting blacklist are
///    dropped from the labelled map. Gated on `params.validate`
///    being `Some`.
///
/// Returns `None` only when no seed is found; once a seed exists,
/// every later stage degrades gracefully (a too-small labelled set
/// causes extension and fill to no-op rather than fail).
///
/// # Example
///
/// See `crates/projective-grid/tests/square_pipeline_smoke.rs` for a
/// synthetic perspective-warped fixture end-to-end.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = positions.len()),
    )
)]
pub fn detect_square_grid<S, G>(
    positions: &[Point2<f32>],
    seed_validator: &S,
    grow_validator: &G,
    params: &SquareGridParams,
) -> Option<SquareGridDetection>
where
    S: SeedQuadValidator,
    G: GrowValidator,
{
    let mut stats = SquareGridStats::default();

    let SeedOutput { seed, cell_size } = find_quad(seed_validator, &params.seed)?;
    stats.seed = Some([seed.a, seed.b, seed.c, seed.d]);

    let mut grow_res: GrowResult =
        bfs_grow(positions, seed, cell_size, &params.grow, grow_validator);
    // 4 corners come from the seed; the rest are BFS attachments.
    stats.grown = grow_res.labelled.len().saturating_sub(4);

    match &params.extension {
        ExtensionStrategy::Disabled => {}
        ExtensionStrategy::Global(extension_params) => {
            let ext = extend_via_global_homography(
                positions,
                &mut grow_res,
                cell_size,
                extension_params,
                grow_validator,
            );
            stats.extension = Some(ext);
        }
        ExtensionStrategy::Local(extension_params) => {
            let ext = extend_via_local_homography(
                positions,
                &mut grow_res,
                cell_size,
                extension_params,
                grow_validator,
            );
            stats.extension = Some(ext);
        }
    }

    if let Some(fill_params) = params.fill.as_ref() {
        let fill = fill_grid_holes(
            positions,
            &mut grow_res,
            cell_size,
            fill_params,
            grow_validator,
        );
        stats.fill = Some(fill);
    }

    if let Some(validate_params) = params.validate.as_ref() {
        let entries: Vec<LabelledEntry> = grow_res
            .labelled
            .iter()
            .map(|(&(i, j), &idx)| LabelledEntry {
                idx,
                pixel: positions[idx],
                grid: (i, j),
            })
            .collect();
        let result = validate(&entries, cell_size, validate_params);
        for &idx in &result.blacklist {
            if let Some(&cell) = grow_res.by_corner.get(&idx) {
                grow_res.labelled.remove(&cell);
            }
            grow_res.by_corner.remove(&idx);
        }
        stats.dropped_by_validation = result.blacklist.len();
        stats.validation = Some(result);
    }

    Some(SquareGridDetection {
        labelled: grow_res.labelled,
        by_corner: grow_res.by_corner,
        grid_u: grow_res.grid_u,
        grid_v: grow_res.grid_v,
        cell_size,
        stats,
    })
}

/// Tuning knobs for [`detect_square_grid_all`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct MultiComponentParams {
    /// Maximum number of components to peel off before stopping.
    /// Default: `4`.
    pub max_components: usize,
    /// Stop once a component has fewer than this many labelled
    /// corners (typically because the remaining unconsumed corners
    /// form noise rather than a real component). Default: `4` —
    /// the seed quad's four corners are the floor.
    pub min_corners_per_component: usize,
}

impl Default for MultiComponentParams {
    fn default() -> Self {
        Self {
            max_components: 4,
            min_corners_per_component: 4,
        }
    }
}

/// Multi-component variant of [`detect_square_grid`].
///
/// Peels off one component at a time: after each successful call
/// to [`detect_square_grid`], the indices of every labelled corner
/// are marked consumed, and the next iteration's validators see a
/// reduced eligibility set. Returns every component in detection
/// order.
///
/// Pass the result through [`crate::merge_components_local`] if
/// you want to reunite spatially-adjacent components into one
/// labelled grid (typical for partially-occluded boards).
///
/// # Stopping conditions
///
/// - No more seeds can be found in the unconsumed pool.
/// - `multi.max_components` reached.
/// - The most recent component had fewer than
///   `multi.min_corners_per_component` corners.
pub fn detect_square_grid_all<S, G>(
    positions: &[Point2<f32>],
    seed_validator: &S,
    grow_validator: &G,
    params: &SquareGridParams,
    multi: &MultiComponentParams,
) -> Vec<SquareGridDetection>
where
    S: SeedQuadValidator,
    G: GrowValidator,
{
    let mut consumed: HashSet<usize> = HashSet::new();
    let mut detections: Vec<SquareGridDetection> = Vec::new();

    while detections.len() < multi.max_components {
        let wrapped_seed = ExcludeConsumedSeed {
            inner: seed_validator,
            consumed: &consumed,
        };
        let wrapped_grow = ExcludeConsumedGrow {
            inner: grow_validator,
            consumed: &consumed,
        };
        let Some(det) = detect_square_grid(positions, &wrapped_seed, &wrapped_grow, params) else {
            break;
        };
        if det.labelled.len() < multi.min_corners_per_component {
            break;
        }
        for (_, &corner_idx) in det.labelled.iter() {
            consumed.insert(corner_idx);
        }
        detections.push(det);
    }

    detections
}

// ---------------------------------------------------------------------------
// Validator wrappers used by `detect_square_grid_all` to exclude already-
// consumed corner indices from a fresh detection pass.
// ---------------------------------------------------------------------------

struct ExcludeConsumedSeed<'a, S> {
    inner: &'a S,
    consumed: &'a HashSet<usize>,
}

impl<'a, S: SeedQuadValidator> SeedQuadValidator for ExcludeConsumedSeed<'a, S> {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.inner.position(idx)
    }
    fn axes(&self, idx: usize) -> [AxisEstimate; 2] {
        self.inner.axes(idx)
    }
    fn a_candidates(&self) -> Vec<usize> {
        self.inner
            .a_candidates()
            .into_iter()
            .filter(|i| !self.consumed.contains(i))
            .collect()
    }
    fn bc_candidates(&self) -> Vec<usize> {
        self.inner
            .bc_candidates()
            .into_iter()
            .filter(|i| !self.consumed.contains(i))
            .collect()
    }
    fn edge_ok(&self, from: usize, to: usize, axis_tol_rad: f32) -> bool {
        self.inner.edge_ok(from, to, axis_tol_rad)
    }
    fn has_midpoint_violation(&self, seed: Seed, cell_size: f32) -> bool {
        self.inner.has_midpoint_violation(seed, cell_size)
    }
}

struct ExcludeConsumedGrow<'a, G> {
    inner: &'a G,
    consumed: &'a HashSet<usize>,
}

impl<'a, G: GrowValidator> GrowValidator for ExcludeConsumedGrow<'a, G> {
    fn is_eligible(&self, idx: usize) -> bool {
        !self.consumed.contains(&idx) && self.inner.is_eligible(idx)
    }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        self.inner.required_label_at(i, j)
    }
    fn label_of(&self, idx: usize) -> Option<u8> {
        self.inner.label_of(idx)
    }
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit {
        if self.consumed.contains(&idx) {
            return Admit::Reject;
        }
        self.inner.accept_candidate(idx, at, prediction, neighbours)
    }
    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        at_candidate: (i32, i32),
        at_neighbour: (i32, i32),
    ) -> bool {
        self.inner
            .edge_ok(candidate_idx, neighbour_idx, at_candidate, at_neighbour)
    }
    fn eligible_for_fill(&self, idx: usize) -> bool {
        !self.consumed.contains(&idx) && self.inner.eligible_for_fill(idx)
    }
    fn fill_edge_ok(&self, ctx: FillEdgeCtx<'_>) -> bool {
        self.inner.fill_edge_ok(ctx)
    }
}
