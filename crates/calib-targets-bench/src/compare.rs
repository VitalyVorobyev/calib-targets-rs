//! Per-family comparison of two `bench run` JSON reports.
//!
//! This is a **pure aggregator**: it joins two already-written [`RunReport`]s
//! (one per graph-build algorithm) by target-substrate family and emits a
//! side-by-side recall + precision table. It re-runs no detection, so the
//! campaign stays read-only on detector behaviour and the two input reports
//! remain the archived evidence.
//!
//! "Family" here is the target *substrate* of the source images, not a decode
//! result — the bench only ever exercises the chessboard grid builder, so every
//! number is grid quality. There is no marker family in the dataset; the
//! charuco rows measure grid quality on charuco *images*, not the charuco
//! decode (the decode-level comparison is blocked by the topological guard and
//! deferred).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::report::{PerImageReport, RunReport};

/// Canonical family render order. A family is only emitted when at least one
/// snap in either report maps to it.
const FAMILY_ORDER: [&str; 4] = ["chessboard", "charuco", "puzzle", "marker"];

/// One aggregated row: the metrics for a single `(family, algorithm)` cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyComparison {
    /// Target-substrate family (`chessboard` / `charuco` / `puzzle`).
    pub family: String,
    /// Graph-build algorithm slug taken from the source report's `config_id`.
    pub algorithm: String,
    /// Snaps in this family.
    pub snaps: usize,
    /// Snaps where the detector returned no labelled corners.
    pub zero_detection_snaps: usize,
    /// Median labelled-corner count over the family (recall).
    pub labelled_median: usize,
    /// Minimum labelled-corner count over the family (worst-case recall).
    pub labelled_min: usize,
    /// Sum of `duplicate_run_positions` across the family (precision).
    pub duplicate_positions_total: usize,
    /// Sum of `structural_precision.overlong_edges` (precision).
    pub overlong_edges_total: usize,
    /// Sum of `structural_precision.collapsed_pairs` (precision).
    pub collapsed_pairs_total: usize,
}

/// The full comparison: two algorithm rows per present family.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// `config_id` of the first (A) report.
    pub a_config_id: String,
    /// `config_id` of the second (B) report.
    pub b_config_id: String,
    /// Aggregated rows, family-major (A then B within each family).
    pub rows: Vec<FamilyComparison>,
}

/// Map a snap label (baseline key / image path, with any `#k` suffix) to a
/// coarse target-substrate family. Matches the on-disk layout in
/// `datasets.toml`; unrecognised paths fall back to `chessboard`.
pub fn family_of(label: &str) -> &'static str {
    let base = label.split('#').next().unwrap_or(label);
    if base.contains("/puzzleboard_reference/") || base.contains("/130x130_puzzle/") {
        "puzzle"
    } else if base.contains("/3536119669/")
        || base.ends_with("/large.png")
        || is_charuco_small(base)
    {
        "charuco"
    } else {
        // testdata/mid.png + testdata/02-topo-grid/* + anything unrecognised.
        "chessboard"
    }
}

/// `testdata/smallN.png` (the tilted-lens ChArUco set) → true.
fn is_charuco_small(base: &str) -> bool {
    let name = base.rsplit('/').next().unwrap_or(base);
    name.strip_prefix("small")
        .and_then(|rest| rest.strip_suffix(".png"))
        .map(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false)
}

/// The graph-build algorithm slug carried in a `config_id`
/// (`engine.algorithm.orientation_method`); falls back to the whole id when it
/// isn't in that shape.
fn algorithm_token(config_id: &str) -> &str {
    config_id.split('.').nth(1).unwrap_or(config_id)
}

/// Join two run reports into a per-family comparison.
pub fn build_comparison(a: &RunReport, b: &RunReport) -> ComparisonReport {
    let a_alg = algorithm_token(&a.config_id).to_string();
    let b_alg = algorithm_token(&b.config_id).to_string();

    let present: std::collections::HashSet<&str> = a
        .per_image
        .iter()
        .chain(b.per_image.iter())
        .map(|r| family_of(&r.image))
        .collect();

    let mut rows = Vec::new();
    for family in FAMILY_ORDER.iter().filter(|f| present.contains(**f)) {
        rows.push(aggregate(family, &a_alg, &a.per_image));
        rows.push(aggregate(family, &b_alg, &b.per_image));
    }

    ComparisonReport {
        a_config_id: a.config_id.clone(),
        b_config_id: b.config_id.clone(),
        rows,
    }
}

fn aggregate(family: &str, algorithm: &str, rows: &[PerImageReport]) -> FamilyComparison {
    let mut labelled: Vec<usize> = Vec::new();
    let mut zero_detection_snaps = 0usize;
    let mut duplicate_positions_total = 0usize;
    let mut overlong_edges_total = 0usize;
    let mut collapsed_pairs_total = 0usize;

    for r in rows.iter().filter(|r| family_of(&r.image) == family) {
        labelled.push(r.labelled_count);
        if r.labelled_count == 0 {
            zero_detection_snaps += 1;
        }
        duplicate_positions_total += r.diff_vs_baseline.duplicate_run_positions.len();
        overlong_edges_total += r.structural_precision.overlong_edges;
        collapsed_pairs_total += r.structural_precision.collapsed_pairs;
    }

    labelled.sort_unstable();
    let snaps = labelled.len();
    let labelled_median = if snaps == 0 { 0 } else { labelled[snaps / 2] };
    let labelled_min = labelled.first().copied().unwrap_or(0);

    FamilyComparison {
        family: family.to_string(),
        algorithm: algorithm.to_string(),
        snaps,
        zero_detection_snaps,
        labelled_median,
        labelled_min,
        duplicate_positions_total,
        overlong_edges_total,
        collapsed_pairs_total,
    }
}

/// Read a [`RunReport`] from a JSON file written by `bench run`.
pub fn load_report(path: &Path) -> std::io::Result<RunReport> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Render the comparison as a GitHub-flavoured markdown table.
pub fn render_markdown(report: &ComparisonReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Grid-builder A/B comparison — grid quality by family-substrate"
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Chessboard grid-builder output only. \"Family\" is the target *substrate* of the \
         source images, not a decode result. Recall = labelled-corner count; precision = \
         baseline-free structural signals. Charuco rows measure grid quality on charuco \
         *images*, not the charuco decode."
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "- **A**: `{}`", report.a_config_id);
    let _ = writeln!(s, "- **B**: `{}`", report.b_config_id);
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| Family | Algorithm | Snaps | Zero-det | Labelled median | Labelled min | Duplicates | Overlong | Collapsed |"
    );
    let _ = writeln!(s, "|---|---|--:|--:|--:|--:|--:|--:|--:|");
    for row in &report.rows {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.family,
            row.algorithm,
            row.snaps,
            row.zero_detection_snaps,
            row.labelled_median,
            row.labelled_min,
            row.duplicate_positions_total,
            row.overlong_edges_total,
            row.collapsed_pairs_total,
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::BaselineDiff;
    use crate::precision::StructuralPrecision;
    use crate::report::Summary;

    fn row(image: &str, labelled: usize) -> PerImageReport {
        PerImageReport {
            image: image.to_string(),
            passed: true,
            has_baseline: true,
            elapsed_ms: 1.0,
            labelled_count: labelled,
            diff_vs_baseline: BaselineDiff::default(),
            structural_precision: StructuralPrecision::default(),
        }
    }

    fn report(config_id: &str, rows: Vec<PerImageReport>) -> RunReport {
        RunReport {
            schema: crate::SCHEMA_VERSION,
            detector: "chessboard".to_string(),
            config_id: config_id.to_string(),
            summary: Summary {
                images_total: rows.len(),
                images_passed: rows.len(),
                images_failed: 0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                max_ms: 0.0,
            },
            per_image: rows,
        }
    }

    #[test]
    fn family_of_maps_known_paths() {
        assert_eq!(family_of("testdata/mid.png"), "chessboard");
        assert_eq!(
            family_of("testdata/02-topo-grid/GeminiChess1.png"),
            "chessboard"
        );
        assert_eq!(family_of("testdata/large.png"), "charuco");
        assert_eq!(family_of("testdata/small3.png"), "charuco");
        assert_eq!(
            family_of("privatedata/3536119669/target_7.png#0"),
            "charuco"
        );
        assert_eq!(
            family_of("testdata/puzzleboard_reference/example1.png"),
            "puzzle"
        );
        assert_eq!(
            family_of("privatedata/130x130_puzzle/target_15.png#3"),
            "puzzle"
        );
    }

    #[test]
    fn algorithm_token_extracts_second_segment() {
        assert_eq!(
            algorithm_token("pipeline.topological.ring_fit.chess_axes"),
            "topological"
        );
        assert_eq!(algorithm_token("weird"), "weird");
    }

    #[test]
    fn aggregate_reproduces_labelled_counts() {
        let a = report(
            "pipeline.topological.ring_fit.chess_axes",
            vec![
                row("testdata/mid.png", 77),
                row("testdata/large.png", 300),
                row("testdata/small0.png", 100),
            ],
        );
        let b = report(
            "pipeline.seed_and_grow.ring_fit.chess_axes",
            vec![
                row("testdata/mid.png", 70),
                row("testdata/large.png", 280),
                row("testdata/small0.png", 0), // lost detection
            ],
        );
        let cmp = build_comparison(&a, &b);
        // chessboard (mid only) then charuco (large, small0): 2 algo rows each.
        assert_eq!(cmp.rows.len(), 4);

        let chess_a = cmp
            .rows
            .iter()
            .find(|r| r.family == "chessboard" && r.algorithm == "topological")
            .unwrap();
        assert_eq!(chess_a.snaps, 1);
        assert_eq!(chess_a.labelled_median, 77);
        assert_eq!(chess_a.labelled_min, 77);

        let charuco_b = cmp
            .rows
            .iter()
            .find(|r| r.family == "charuco" && r.algorithm == "seed_and_grow")
            .unwrap();
        assert_eq!(charuco_b.snaps, 2);
        assert_eq!(charuco_b.zero_detection_snaps, 1);
        assert_eq!(charuco_b.labelled_min, 0);
        // sorted [0, 280] → median index 1 → 280.
        assert_eq!(charuco_b.labelled_median, 280);

        // No puzzle/marker snaps present → not emitted.
        assert!(cmp.rows.iter().all(|r| r.family != "puzzle"));
        assert!(cmp.rows.iter().all(|r| r.family != "marker"));
    }
}
