//! Private adapters for calls into the legacy `projective-grid` crate.
//!
//! `calib-targets-core` now owns the shared public grid/geometry types. The
//! chessboard detector still uses several legacy algorithms while they are
//! being ported, so all type crossings live here instead of leaking legacy type
//! identity back into the workspace API.

use calib_targets_core::{AxisEstimate, GridAlignment, GridCoords, GridTransform, Homography};

pub(crate) fn axis_to_legacy(axis: AxisEstimate) -> projective_grid::AxisEstimate {
    projective_grid::AxisEstimate {
        angle: axis.angle,
        sigma: axis.sigma,
    }
}

pub(crate) fn axes_to_legacy(axes: [AxisEstimate; 2]) -> [projective_grid::AxisEstimate; 2] {
    [axis_to_legacy(axes[0]), axis_to_legacy(axes[1])]
}

#[allow(dead_code)]
pub(crate) fn coord_to_legacy(coord: GridCoords) -> projective_grid::GridCoords {
    projective_grid::GridCoords {
        i: coord.i,
        j: coord.j,
    }
}

#[allow(dead_code)]
pub(crate) fn coord_from_legacy(coord: projective_grid::GridCoords) -> GridCoords {
    GridCoords {
        i: coord.i,
        j: coord.j,
    }
}

#[allow(dead_code)]
pub(crate) fn transform_to_legacy(transform: GridTransform) -> projective_grid::GridTransform {
    projective_grid::GridTransform {
        a: transform.a,
        b: transform.b,
        c: transform.c,
        d: transform.d,
    }
}

#[allow(dead_code)]
pub(crate) fn transform_from_legacy(transform: projective_grid::GridTransform) -> GridTransform {
    GridTransform {
        a: transform.a,
        b: transform.b,
        c: transform.c,
        d: transform.d,
    }
}

#[allow(dead_code)]
pub(crate) fn alignment_to_legacy(alignment: GridAlignment) -> projective_grid::GridAlignment {
    projective_grid::GridAlignment {
        transform: transform_to_legacy(alignment.transform),
        translation: alignment.translation,
    }
}

#[allow(dead_code)]
pub(crate) fn alignment_from_legacy(alignment: projective_grid::GridAlignment) -> GridAlignment {
    GridAlignment {
        transform: transform_from_legacy(alignment.transform),
        translation: alignment.translation,
    }
}

#[allow(dead_code)]
pub(crate) fn homography_to_legacy(h: Homography) -> projective_grid::Homography {
    projective_grid::Homography::new(h.h)
}

#[allow(dead_code)]
pub(crate) fn homography_from_legacy(h: projective_grid::Homography) -> Homography {
    Homography::new(h.h)
}
