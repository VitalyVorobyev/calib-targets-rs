//! Console rendering of the run report. The report types and the
//! compute/summary/serialization helpers live in the library
//! (`calib_targets_bench::report`).

use calib_targets::DetectionQuality;
use calib_targets_bench::report::RunReport;

pub(crate) fn print_summary(report: &RunReport) -> std::io::Result<()> {
    println!("\n--- per-image -----------------------------------------------------------");
    for r in &report.per_image {
        let d = &r.diff_vs_baseline;
        let status = if !r.has_baseline {
            "NO-BASELINE"
        } else if r.passed {
            if d.extra_labels.is_empty() {
                "PASS"
            } else {
                "PASS+"
            }
        } else {
            "FAIL"
        };
        let dup = d.duplicate_run_positions.len();
        let sp = &r.structural_precision;
        println!(
            "{status:<11} {:<50} {:>4} corners {:>7.1} ms  miss={:>3} extra={:>3} pos={:>3} id={:>3} dup={:>3} ov={:>3} col={:>3}{}{}",
            r.image,
            r.labelled_count,
            r.elapsed_ms,
            d.missing_labels.len(),
            d.extra_labels.len(),
            d.wrong_position.len(),
            d.wrong_id.len(),
            dup,
            sp.overlong_edges,
            sp.collapsed_pairs,
            if d.inconsistent_shift { "  SHIFT-INCONSISTENT" } else { "" },
            r.quality.as_ref().map(quality_cell).unwrap_or_default(),
        );
    }
    let improvements: usize = report
        .per_image
        .iter()
        .filter(|r| r.passed && !r.diff_vs_baseline.extra_labels.is_empty())
        .map(|r| r.diff_vs_baseline.extra_labels.len())
        .sum();
    println!("\n--- summary -------------------------------------------------------------");
    println!(
        "total={} passed={} failed={} improvements=+{}  p50={:.1} ms  p95={:.1} ms  max={:.1} ms",
        report.summary.images_total,
        report.summary.images_passed,
        report.summary.images_failed,
        improvements,
        report.summary.p50_ms,
        report.summary.p95_ms,
        report.summary.max_ms,
    );
    let q = &report.summary.quality;
    println!(
        "quality (median over {} images, report-only): noise={} (known {}) cov={} σ_med={} px  local_rms={} px  σ̂₀²={} ({} images)",
        q.images,
        opt(q.noise_sigma, 2),
        q.noise_known,
        opt(q.covariance_coverage, 2),
        opt(q.median_sigma_px, 3),
        opt(q.local_residual_rms_px, 3),
        opt(q.variance_factor, 2),
        q.variance_factor_images,
    );
    Ok(())
}

/// The per-image quality columns (report-only).
fn quality_cell(q: &DetectionQuality) -> String {
    format!(
        "  noise={} cov={} σ_med={} rms={} σ̂₀²={}",
        opt(q.noise_sigma, 2),
        opt(Some(q.covariance_coverage), 2),
        opt(q.median_sigma_px, 3),
        opt(q.local_residual_rms_px, 3),
        opt(q.variance_factor, 2),
    )
}

fn opt(v: Option<f32>, digits: usize) -> String {
    v.map_or_else(|| "-".to_string(), |v| format!("{v:.digits$}"))
}
