# Handoff: [Source Role] → [Target Role]

- **Task:** [FEAT|BUG|PERF|ALGO]-NNN: [Title]
- **Date:** YYYY-MM-DD
- **Branch:** [git branch name]

## Work Completed

- [What was done in this phase — bullet list]

## Key Findings

- [Important discoveries, surprises, or concerns]

## Files Changed

| File | Change |
|------|--------|
| `crates/ringgrid/src/...` | [one-line description] |

## Test Results

- **cargo test:** pass / fail ([count] tests)
- **cargo clippy:** clean / [N] warnings
- **cargo fmt:** clean / needs formatting

## Accuracy State

| Metric | Value |
|--------|-------|
| Center error (mean) | [px or "not measured"] |
| Center error (p50) | [px or "not measured"] |
| Center error (p95) | [px or "not measured"] |
| Decode success rate | [% or "not measured"] |
| Homography self-error (mean) | [px or "not measured"] |
| Homography vs-GT error (mean) | [px or "not measured"] |

## Performance State

| Benchmark | Result |
|-----------|--------|
| [bench_name] | [ns or "not measured"] |

## PERF Validation Gates (required for PERF tasks)

| Gate | Baseline Artifact | After Artifact | Center Mean Delta (px) | Recall Delta | H-Self Delta (px) | H-vs-GT Delta (px) | Status |
|------|-------------------|----------------|------------------------|--------------|-------------------|--------------------|--------|
| Blur-3 synth eval (`n=10`, `run_blur3_benchmark.sh`) | | | | | | | |
| Reference benchmark script (`run_reference_benchmark.sh`) | | | | | | | |
| Distortion benchmark script (`run_distortion_benchmark.sh`) | | | | | | | |

## Accuracy Report Artifact (required for PERF tasks)

- Path to filled report from `.ai/templates/accuracy-report.md`: `[path]`
- Threshold callout (`+0.01 px` center-mean gate): [pass/fail + short note]

## Open Questions

- [Anything the next role needs to decide or investigate]

## Recommended Next Steps

1. [Specific action for the target role]
2. [...]

## Blocking Issues

[Anything that prevents progress. "None" if clear.]
