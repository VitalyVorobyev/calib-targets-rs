## Project
Unifying and elevating a three-crate Rust computer-vision metrology stack —
`calibration-targets` (ChArUco reconstruction) and `projective-grid` (grid
topology, published on crates.io), with `chess-corners` (X-corner detection)
consumed as an external dependency — toward a *measured* best-in-class claim for
calibration-target / grid-detection robustness.

## Scope guardrails
- Brownfield, not rewrite. The in-repo crates (`calibration-targets`,
  `projective-grid`) are both the proven kernels and the baseline I measure
  against. Do not propose greenfield rewrites; improvements are surgical and
  snapshot-guarded. The only greenfield work is net-new measurement
  infrastructure (harness, competitor adapters).
- `chess-corners` is an external dependency in a separate repo. Consume it
  through its public API; do NOT edit it, vendor-patch it, or propose changes to
  it from this repo. If measurement shows corner localization is the binding
  constraint, surface that as a finding for separate upstream work — do not act
  on it here.
- Classical scope. Best-in-class among efficient *classical* detectors. No
  learned/CNN detectors in the library or as the bar to beat.
- Pareto, not clean sweep. "Best-in-class" means dominating the
  robustness-vs-efficiency frontier, not winning every axis. Both measured from
  the start.
