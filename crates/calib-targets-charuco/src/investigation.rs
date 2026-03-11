use crate::{
    io::StripAcceptanceMetrics, CharucoBoardSpec, ImageCropRect, MarkerLayout, StripCoverageMetrics,
};
use calib_targets_aruco::builtins;
use calib_targets_core::LabeledCorner;
use serde::Deserialize;
use std::path::Path;

pub const COMPOSITE_STRIP_COUNT: usize = 6;
pub const DEFAULT_MIN_CORNER_COUNT: usize = 40;

#[derive(thiserror::Error, Debug)]
pub enum InvestigationConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("dataset target configuration is missing")]
    MissingTarget,
    #[error("dataset target acquisition AOI is missing")]
    MissingStripSize,
    #[error("unknown dictionary {name}")]
    UnknownDictionary { name: String },
    #[error("image width {width} is not divisible by {strips}")]
    NonDivisibleCompositeWidth { width: u32, strips: usize },
    #[error("composite image size {width}x{height} does not match expected strip size {strip_width}x{strip_height} x {strips}")]
    UnexpectedCompositeSize {
        width: u32,
        height: u32,
        strip_width: u32,
        strip_height: u32,
        strips: usize,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetConfig {
    pub target: Option<DatasetTargetConfig>,
    pub sensor: Option<DatasetSensorConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetTargetConfig {
    pub nrows: u32,
    pub ncols: u32,
    pub cellsize_mm: f32,
    pub marker_scale: f32,
    pub dict: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetSensorConfig {
    pub target_acquisition: Option<DatasetAcquisitionConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetAcquisitionConfig {
    pub aoi: Option<DatasetAoiConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetAoiConfig {
    pub width: u32,
    pub height: u32,
}

impl DatasetConfig {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, InvestigationConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn board_spec(&self) -> Result<CharucoBoardSpec, InvestigationConfigError> {
        let target = self
            .target
            .as_ref()
            .ok_or(InvestigationConfigError::MissingTarget)?;
        let dict_name = normalize_dictionary_name(&target.dict);
        let dictionary = builtins::builtin_dictionary(&dict_name).ok_or_else(|| {
            InvestigationConfigError::UnknownDictionary {
                name: dict_name.clone(),
            }
        })?;
        Ok(CharucoBoardSpec {
            rows: target.nrows,
            cols: target.ncols,
            cell_size: target.cellsize_mm,
            marker_size_rel: target.marker_scale,
            dictionary,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
    }

    pub fn strip_size(&self) -> Result<(u32, u32), InvestigationConfigError> {
        let sensor = self
            .sensor
            .as_ref()
            .ok_or(InvestigationConfigError::MissingStripSize)?;
        let acquisition = sensor
            .target_acquisition
            .as_ref()
            .ok_or(InvestigationConfigError::MissingStripSize)?;
        let aoi = acquisition
            .aoi
            .as_ref()
            .ok_or(InvestigationConfigError::MissingStripSize)?;
        Ok((aoi.width, aoi.height))
    }
}

pub fn normalize_dictionary_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.starts_with("DICT_") {
        trimmed.to_string()
    } else {
        format!("DICT_{trimmed}")
    }
}

pub fn split_composite_rects(
    width: u32,
    height: u32,
    expected_strip_size: Option<(u32, u32)>,
) -> Result<Vec<ImageCropRect>, InvestigationConfigError> {
    if width % COMPOSITE_STRIP_COUNT as u32 != 0 {
        return Err(InvestigationConfigError::NonDivisibleCompositeWidth {
            width,
            strips: COMPOSITE_STRIP_COUNT,
        });
    }
    let strip_width = width / COMPOSITE_STRIP_COUNT as u32;
    let strip_height = height;

    if let Some((expected_width, expected_height)) = expected_strip_size {
        if strip_width != expected_width || strip_height != expected_height {
            return Err(InvestigationConfigError::UnexpectedCompositeSize {
                width,
                height,
                strip_width: expected_width,
                strip_height: expected_height,
                strips: COMPOSITE_STRIP_COUNT,
            });
        }
    }

    Ok((0..COMPOSITE_STRIP_COUNT)
        .map(|idx| ImageCropRect {
            x: strip_width * idx as u32,
            y: 0,
            width: strip_width,
            height: strip_height,
        })
        .collect())
}

pub fn compute_strip_coverage(corners: &[LabeledCorner], image_width: u32) -> StripCoverageMetrics {
    let mut counts = vec![0usize; COMPOSITE_STRIP_COUNT];
    let bin_width = if image_width > 0 {
        image_width as f32 / COMPOSITE_STRIP_COUNT as f32
    } else {
        1.0
    };
    let mut ys = Vec::with_capacity(corners.len());

    for corner in corners {
        let x = corner
            .position
            .x
            .clamp(0.0, image_width.saturating_sub(1) as f32);
        let bin = ((x / bin_width).floor() as usize).min(COMPOSITE_STRIP_COUNT - 1);
        counts[bin] += 1;
        ys.push(corner.position.y);
    }

    let empty_bin_count = counts.iter().filter(|&&count| count == 0).count();
    let min_bin_count = counts.iter().copied().min().unwrap_or(0);
    ys.sort_by(|a, b| a.total_cmp(b));

    StripCoverageMetrics {
        x_bin_counts: counts,
        empty_bin_count,
        min_bin_count,
        y_min: ys.first().copied(),
        y_p10: percentile(&ys, 0.10),
        y_median: percentile(&ys, 0.50),
        y_p90: percentile(&ys, 0.90),
        y_max: ys.last().copied(),
    }
}

pub fn build_strip_acceptance(
    corner_count: usize,
    coverage: &StripCoverageMetrics,
    min_corner_count: usize,
) -> StripAcceptanceMetrics {
    let passes_corner_count = corner_count >= min_corner_count;
    let passes_x_coverage = coverage.empty_bin_count == 0;
    StripAcceptanceMetrics {
        min_corner_count,
        passes_corner_count,
        passes_x_coverage,
        passes_all: passes_corner_count && passes_x_coverage,
    }
}

pub fn spread_gate_limit(counts: &[usize]) -> Option<usize> {
    let median = median(counts)?;
    Some(((median as f64) * 0.2).ceil() as usize).map(|v| v.max(10))
}

pub fn passes_spread_gate(counts: &[usize]) -> bool {
    let Some(limit) = spread_gate_limit(counts) else {
        return false;
    };
    let Some(min_count) = counts.iter().min().copied() else {
        return false;
    };
    let Some(max_count) = counts.iter().max().copied() else {
        return false;
    };
    max_count.saturating_sub(min_count) <= limit
}

pub fn median(values: &[usize]) -> Option<usize> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    })
}

fn percentile(values: &[f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let q = q.clamp(0.0, 1.0);
    let pos = q * (values.len() - 1) as f32;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return Some(values[lo]);
    }
    let w = pos - lo as f32;
    Some(values[lo] * (1.0 - w) + values[hi] * w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StripCoverageMetrics;
    use calib_targets_core::GridCoords;
    use nalgebra::Point2;

    fn corner(x: f32, y: f32) -> LabeledCorner {
        LabeledCorner {
            position: Point2::new(x, y),
            grid: Some(GridCoords { i: 0, j: 0 }),
            id: Some(0),
            target_position: Some(Point2::new(0.0, 0.0)),
            score: 1.0,
        }
    }

    #[test]
    fn split_composite_rects_even_six_way() {
        let rects = split_composite_rects(4320, 540, Some((720, 540))).expect("rects");
        assert_eq!(rects.len(), COMPOSITE_STRIP_COUNT);
        assert_eq!(
            rects[0],
            ImageCropRect {
                x: 0,
                y: 0,
                width: 720,
                height: 540,
            }
        );
        assert_eq!(rects[5].x, 3600);
    }

    #[test]
    fn split_composite_rects_rejects_invalid_width() {
        let err = split_composite_rects(4319, 540, None).expect_err("should fail");
        assert!(matches!(
            err,
            InvestigationConfigError::NonDivisibleCompositeWidth { .. }
        ));
    }

    #[test]
    fn compute_strip_coverage_counts_bins() {
        let coverage = compute_strip_coverage(
            &[
                corner(10.0, 100.0),
                corner(130.0, 101.0),
                corner(250.0, 102.0),
                corner(370.0, 103.0),
                corner(490.0, 104.0),
                corner(610.0, 105.0),
            ],
            720,
        );
        assert_eq!(coverage.x_bin_counts, vec![1, 1, 1, 1, 1, 1]);
        assert_eq!(coverage.empty_bin_count, 0);
        assert_eq!(coverage.min_bin_count, 1);
    }

    #[test]
    fn build_strip_acceptance_uses_corner_and_bin_gates() {
        let coverage = StripCoverageMetrics {
            x_bin_counts: vec![2, 2, 2, 2, 2, 2],
            empty_bin_count: 0,
            min_bin_count: 2,
            y_min: None,
            y_p10: None,
            y_median: None,
            y_p90: None,
            y_max: None,
        };
        let acceptance = build_strip_acceptance(42, &coverage, DEFAULT_MIN_CORNER_COUNT);
        assert!(acceptance.passes_all);
    }

    #[test]
    fn spread_gate_matches_plan_formula() {
        let counts = [41usize, 43, 44, 42, 40, 45];
        assert_eq!(spread_gate_limit(&counts), Some(10));
        assert!(passes_spread_gate(&counts));
        let bad = [40usize, 40, 40, 40, 40, 60];
        assert!(!passes_spread_gate(&bad));
    }

    #[test]
    fn dataset_config_parses_dictionary_mapping() {
        let raw = r#"{
            "target": {
                "nrows": 22,
                "ncols": 22,
                "cellsize_mm": 5.2,
                "marker_scale": 0.75,
                "dict": "4X4_1000"
            },
            "sensor": {
                "target_acquisition": {
                    "aoi": {
                        "width": 720,
                        "height": 540
                    }
                }
            }
        }"#;
        let cfg: DatasetConfig = serde_json::from_str(raw).expect("config");
        let board = cfg.board_spec().expect("board");
        assert_eq!(board.rows, 22);
        assert_eq!(board.cols, 22);
        assert_eq!(board.dictionary.name, "DICT_4X4_1000");
        assert_eq!(cfg.strip_size().expect("size"), (720, 540));
    }
}
