//! Seed-quad finders. Pattern hooks via `SeedQuadContext<F>`; the seed
//! pipeline emits typed events through [`DiagnosticSink`](crate::DiagnosticSink).

pub mod hex;
pub mod square;

pub use square::{
    find_quad, seed_has_midpoint_violation, MidpointCtx, Seed, SeedOutput, SeedQuad,
    SeedQuadContext, SeedQuadParams, SEED_QUAD_GRID,
};
