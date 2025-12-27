# calib-targets (facade)

The `calib-targets` crate is intended to be the unified entry point that re-exports common types and offers higher-level, ergonomic APIs. Today it mainly hosts examples and brings together dependencies for those examples.

## Current contents

- Examples under `crates/calib-targets/examples/`.
- Optional `tracing` feature for richer logging in examples.

## Planned direction

- Re-export commonly used types (`Corner`, `TargetDetection`, params structs).
- Provide pre-wired detector builders for common pipelines.
- Stabilize error types and configuration structs.
- Offer minimal integration utilities (for example, adapting common corner detector outputs).

See the roadmap for the planned milestones.
