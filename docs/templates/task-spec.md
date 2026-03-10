# Task: [FEAT|BUG|PERF|ALGO]-NNN: [Title]

- **Type:** feature | bug | perf | algo
- **Priority:** P0 | P1 | P2 | P3
- **Requesting role:** [who identified this]
- **Assigned workflow:** feature-development | bug-fix | performance-optimization | algorithm-improvement

## Problem Statement

What needs to change and why?

## Affected Pipeline Stages

Which of the 10 stages are impacted? List by number and name.

1. [ ] Proposal
2. [ ] Outer Estimate
3. [ ] Outer Fit
4. [ ] Decode
5. [ ] Inner Fit
6. [ ] Dedup
7. [ ] Projective Center (once per marker)
8. [ ] Global Filter
9. [ ] Completion (+ projective center for new markers)
10. [ ] Final H Refit

## Affected Modules

List file paths under `crates/ringgrid/src/`:

-

## Public API Impact

- [ ] No API changes
- [ ] New public types (list them)
- [ ] Changed type signatures (list them)
- [ ] New config fields (list them, must have `Default`)
- [ ] New `Detector` methods (justify)

## Acceptance Criteria

- [ ] [Specific, measurable criterion]
- [ ] `cargo test --workspace --all-features` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean
- [ ] Synthetic eval metrics: [specify thresholds]

## Accuracy Constraints

- Center error target: [e.g., "p95 < 0.15 px" or "no regression from baseline"]
- Decode success rate: [e.g., "≥ 99% on synth eval"]
- Homography reprojection: [e.g., "mean < 0.5 px"]

## Performance Constraints

- Latency budget: [e.g., "< 5% regression on proposal_1280x1024 bench"]
- Allocation limits: [e.g., "no new per-candidate allocations"]

## Python Tooling Changes

- [ ] No Python changes needed
- [ ] New scoring metric in `score_detect.py`
- [ ] New visualization in `viz_detect_debug.py`
- [ ] New synthetic generation option in `gen_synth.py`
- [ ] Other: [describe]

## Notes

Additional context, references, related ADRs.
