# Accuracy Report: [Task ID]

- **Date:** YYYY-MM-DD
- **Workflow:** `.ai/workflows/performance-optimization.md` (Phase 3)
- **Runbook:** `.ai/workflows/perf-validation-suite-runbook.md`
- **Baseline label:** [example: perf004]
- **After label:** [example: perf005]
- **Baseline commit:** [hash or n/a]
- **After commit:** [hash or n/a]

## Commands

| Gate | Baseline command | After command |
|------|------------------|---------------|
| Blur-3 synth eval (`n=10`) | `bash tools/run_blur3_benchmark.sh && rm -rf tools/out/eval_[baseline]_blur3 && cp -R tools/out/eval_blur3_post_pipeline tools/out/eval_[baseline]_blur3` | `bash tools/run_blur3_benchmark.sh && rm -rf tools/out/eval_[after]_blur3 && cp -R tools/out/eval_blur3_post_pipeline tools/out/eval_[after]_blur3` |
| Reference benchmark | `bash tools/run_reference_benchmark.sh` | `bash tools/run_reference_benchmark.sh` |
| Distortion benchmark | `bash tools/run_distortion_benchmark.sh` | `bash tools/run_distortion_benchmark.sh` |

## Artifact Paths

| Gate | Baseline artifact | After artifact |
|------|-------------------|----------------|
| Blur-3 synth eval | `tools/out/eval_[baseline]_blur3/det/aggregate.json` | `tools/out/eval_[after]_blur3/det/aggregate.json` |
| Reference benchmark | `tools/out/reference_benchmark_post_pipeline_[baseline].summary.json` | `tools/out/reference_benchmark_post_pipeline_[after].summary.json` |
| Distortion benchmark | `tools/out/r4_benchmark_distorted_threeway_v4_post_pipeline_[baseline].summary.json` | `tools/out/r4_benchmark_distorted_threeway_v4_post_pipeline_[after].summary.json` |

## Gate A: Blur-3 Synth Eval Metrics

| Metric | Baseline | After | Delta | Status |
|--------|----------|-------|-------|--------|
| Precision | | | | |
| Recall | | | | |
| F1 | | | | |
| Decode success rate | | | | |
| Center mean (px) | | | | |
| Center p50 (px, avg per-image) | | | | |
| Center p95 (px, avg per-image) | | | | |
| Center max (px, worst image) | | | | |
| Homography self mean (px) | | | | |
| Homography self p95 (px, avg per-image) | | | | |
| Homography vs-GT mean (px) | | | | |
| Homography vs-GT p95 (px, avg per-image) | | | | |

## Gate A: Frame Consistency Check

| Field set | Baseline | After | Status |
|-----------|----------|-------|--------|
| `center_gt_frame / pred_center_frame / homography_self_error.eval_frame / homography_error_vs_gt.gt_frame / homography_error_vs_gt.pred_h_frame` | | | |

## Gate B: Reference Benchmark Summary

| Mode | Metric | Baseline | After | Delta | Status |
|------|--------|----------|-------|-------|--------|
| `none__none` | Precision | | | | |
| `none__none` | Recall | | | | |
| `none__none` | Center mean (px) | | | | |
| `none__none` | Homography self mean (px) | | | | |
| `none__none` | Homography vs-GT mean (px) | | | | |
| `projective_center__none` | Precision | | | | |
| `projective_center__none` | Recall | | | | |
| `projective_center__none` | Center mean (px) | | | | |
| `projective_center__none` | Homography self mean (px) | | | | |
| `projective_center__none` | Homography vs-GT mean (px) | | | | |

## Gate C: Distortion Benchmark Summary

| Mode | Metric | Baseline | After | Delta | Status |
|------|--------|----------|-------|-------|--------|
| `projective_center__none` | Precision | | | | |
| `projective_center__none` | Recall | | | | |
| `projective_center__none` | Center mean (px) | | | | |
| `projective_center__none` | Homography self mean (px) | | | | |
| `projective_center__none` | Homography vs-GT mean (px) | | | | |
| `projective_center__external` | Precision | | | | |
| `projective_center__external` | Recall | | | | |
| `projective_center__external` | Center mean (px) | | | | |
| `projective_center__external` | Homography self mean (px) | | | | |
| `projective_center__external` | Homography vs-GT mean (px) | | | | |
| `projective_center__self_undistort` | Precision | | | | |
| `projective_center__self_undistort` | Recall | | | | |
| `projective_center__self_undistort` | Center mean (px) | | | | |
| `projective_center__self_undistort` | Homography self mean (px) | | | | |
| `projective_center__self_undistort` | Homography vs-GT mean (px) | | | | |

## Threshold Evaluation

| Rule | Result | Status |
|------|--------|--------|
| Blur-3 center mean delta `<= +0.01 px` | | |
| Blur-3 homography self mean delta `<= +0.02 px` | | |
| Blur-3 homography vs-GT mean delta `<= +0.02 px` | | |
| Reference benchmark precision/recall deltas recorded | | |
| Distortion benchmark precision/recall deltas recorded | | |

## Verdict

- **Overall:** [Pass / Fail / Conditional-pass]
- **Escalate to Algorithm Engineer:** [yes/no + why]
- **Escalate to Pipeline Architect:** [yes/no + why]
- **Notes:** [short summary]
