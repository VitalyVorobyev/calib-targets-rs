//! ChESS config conversion and corner adaptation.
//!
//! These functions mirror `crates/calib-targets/src/detect.rs` to avoid
//! depending on the facade crate (which pulls in `image` codecs and `rayon`
//! via the `image` feature).

use calib_targets_core::{
    AxisEstimate, ChessConfig, Corner, DescriptorMode, DetectorMode, RefinementMethod,
    RefinerConfig, ThresholdMode, UpscaleConfig, UpscaleMode,
};
use nalgebra::Point2;

pub fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> Corner {
    // Derive the legacy single-axis orientation for chessboard / puzzleboard
    // consumers from 0.6's axes[0]; see calib-targets::adapt_chess_corner for
    // the rationale behind the −π/4 shift.
    let orientation =
        (c.axes[0].angle - std::f32::consts::FRAC_PI_4).rem_euclid(std::f32::consts::PI);
    Corner {
        position: Point2::new(c.x, c.y),
        orientation,
        orientation_cluster: None,
        axes: [
            AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}

pub fn to_chess_corners_config(cfg: &ChessConfig) -> chess_corners::ChessConfig {
    let mut out = chess_corners::ChessConfig::default();
    out.detector_mode = to_detector_mode(cfg.detector_mode);
    out.descriptor_mode = to_descriptor_mode(cfg.descriptor_mode);
    out.threshold_mode = to_threshold_mode(cfg.threshold_mode);
    out.threshold_value = cfg.threshold_value;
    out.nms_radius = cfg.nms_radius;
    out.min_cluster_size = cfg.min_cluster_size;
    out.refiner = to_refiner_config(&cfg.refiner);
    out.pyramid_levels = cfg.pyramid_levels;
    out.pyramid_min_size = cfg.pyramid_min_size;
    out.refinement_radius = cfg.refinement_radius;
    out.merge_radius = cfg.merge_radius;
    out.upscale = to_upscale_config(cfg.upscale);
    out
}

fn to_upscale_config(cfg: UpscaleConfig) -> chess_corners::UpscaleConfig {
    match cfg.mode {
        UpscaleMode::Disabled => chess_corners::UpscaleConfig::disabled(),
        UpscaleMode::Fixed => chess_corners::UpscaleConfig::fixed(cfg.factor),
        _ => unimplemented!("unknown UpscaleMode variant"),
    }
}

fn to_detector_mode(mode: DetectorMode) -> chess_corners::DetectorMode {
    match mode {
        DetectorMode::Canonical => chess_corners::DetectorMode::Canonical,
        DetectorMode::Broad => chess_corners::DetectorMode::Broad,
        _ => unimplemented!("unknown DetectorMode variant"),
    }
}

fn to_descriptor_mode(mode: DescriptorMode) -> chess_corners::DescriptorMode {
    match mode {
        DescriptorMode::FollowDetector => chess_corners::DescriptorMode::FollowDetector,
        DescriptorMode::Canonical => chess_corners::DescriptorMode::Canonical,
        DescriptorMode::Broad => chess_corners::DescriptorMode::Broad,
        _ => unimplemented!("unknown DescriptorMode variant"),
    }
}

fn to_threshold_mode(mode: ThresholdMode) -> chess_corners::ThresholdMode {
    match mode {
        ThresholdMode::Relative => chess_corners::ThresholdMode::Relative,
        ThresholdMode::Absolute => chess_corners::ThresholdMode::Absolute,
        _ => unimplemented!("unknown ThresholdMode variant"),
    }
}

fn to_refinement_method(method: RefinementMethod) -> chess_corners::RefinementMethod {
    match method {
        RefinementMethod::CenterOfMass => chess_corners::RefinementMethod::CenterOfMass,
        RefinementMethod::Forstner => chess_corners::RefinementMethod::Forstner,
        RefinementMethod::SaddlePoint => chess_corners::RefinementMethod::SaddlePoint,
        _ => unimplemented!("unknown RefinementMethod variant"),
    }
}

fn to_refiner_config(refiner: &RefinerConfig) -> chess_corners::RefinerConfig {
    chess_corners::RefinerConfig {
        kind: to_refinement_method(refiner.kind),
        center_of_mass: chess_corners::CenterOfMassConfig {
            radius: refiner.center_of_mass.radius,
        },
        forstner: chess_corners::ForstnerConfig {
            radius: refiner.forstner.radius,
            min_trace: refiner.forstner.min_trace,
            min_det: refiner.forstner.min_det,
            max_condition_number: refiner.forstner.max_condition_number,
            max_offset: refiner.forstner.max_offset,
        },
        saddle_point: chess_corners::SaddlePointConfig {
            radius: refiner.saddle_point.radius,
            det_margin: refiner.saddle_point.det_margin,
            max_offset: refiner.saddle_point.max_offset,
            min_abs_det: refiner.saddle_point.min_abs_det,
        },
    }
}
