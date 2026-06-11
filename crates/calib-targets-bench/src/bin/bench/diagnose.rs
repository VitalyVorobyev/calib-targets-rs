//! The `diagnose` subcommand: per-stage corner-count breakdown plus the
//! per-stage / topological diagnostic overlays. This is the "why is this
//! corner missing?" tool — run before changing detector code.

use std::path::Path;
use std::process::ExitCode;

use calib_targets::chessboard::{
    diagnostics::CornerStage, AdvancedTuning, Detector, GraphBuildAlgorithm,
};
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::overlay::{render_diagnose_overlay, render_diagnose_overlay_with_axes};
use calib_targets_bench::workspace_root;
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader};

use super::cli::{AlgorithmArg, DiagnoseArgs};
use super::load_chessboard_config;

pub(crate) fn cmd_diagnose(args: DiagnoseArgs) -> ExitCode {
    let dataset = match Dataset::load_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load datasets.toml: {e}");
            return ExitCode::from(2);
        }
    };
    // Parse `path#k` form.
    let (base_path, sub_idx): (&str, Option<u32>) = match args.image.rsplit_once('#') {
        Some((b, s)) => (b, s.parse().ok()),
        None => (args.image.as_str(), None),
    };

    // Find the matching dataset entry; if absent, build a default one for the path.
    let entry = match dataset.find(base_path) {
        Some(e) => e.clone(),
        None => DatasetEntry {
            path: base_path.to_string(),
            kind: ImageKind::Public,
            note: String::new(),
            upscale: 1,
            stitched: None,
        },
    };
    let abs = entry.absolute();
    if !abs.exists() {
        eprintln!("file not found: {}", abs.display());
        return ExitCode::from(2);
    }

    let img = match ImageReader::open(&abs).and_then(|r| r.decode().map_err(std::io::Error::other))
    {
        Ok(d) => d.to_luma8(),
        Err(e) => {
            eprintln!("decode {}: {e}", abs.display());
            return ExitCode::from(2);
        }
    };

    let snap = if let (Some(spec), Some(k)) = (entry.stitched.as_ref(), sub_idx) {
        let x0 = k * spec.snap_width;
        img.view(x0, 0, spec.snap_width, spec.snap_height)
            .to_image()
    } else {
        img
    };
    let upscaled = if entry.upscale > 1 {
        let (w, h) = snap.dimensions();
        image::imageops::resize(
            &snap,
            w * entry.upscale,
            h * entry.upscale,
            FilterType::Triangle,
        )
    } else {
        snap
    };

    let mut chess_cfg = default_chess_config();
    chess_cfg.orientation_method = args.orientation_method.into();
    let corners = detect_corners(&upscaled, &chess_cfg);
    if args.algorithm == AlgorithmArg::Topological {
        return diagnose_topological(&args, &upscaled, &corners);
    }
    let detector_params = match load_chessboard_config(args.chessboard_config.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("load --chessboard-config: {e}");
            return ExitCode::from(2);
        }
    };
    let detector = calib_targets::chessboard::Detector::new(detector_params.clone())
        .expect("valid detector params");
    let frame = detector.detect_with_diagnostics(&corners);

    print_stage_summary(&args.image, &frame);

    // Also probe how many components `detect_all` recovers — useful when a
    // ChArUco split produces several disjoint chessboard subgraphs that the
    // single-best `detect()` call hides.
    let detector_for_all =
        calib_targets::chessboard::Detector::new(detector_params).expect("valid detector params");
    let all_frames = detector_for_all.detect_all_with_diagnostics(&corners);
    if all_frames.len() > 1 {
        println!("\n  --- detect_all_with_diagnostics ---");
        for (k, f) in all_frames.iter().enumerate() {
            let labelled = f.detection.as_ref().map(|d| d.corners.len()).unwrap_or(0);
            println!("  component {k}: labelled={labelled}");
        }
    }

    let label = if sub_idx.is_some() {
        args.image.clone()
    } else {
        base_path.to_string()
    };
    let dst = args.out.as_deref().map_or_else(
        || {
            workspace_root()
                .join("preview/diagnose")
                .join(diagnose_filename(&label))
        },
        |p| workspace_root().join(p),
    );
    let render_result = if args.draw_axes {
        render_diagnose_overlay_with_axes(&upscaled, &frame, &dst)
    } else {
        render_diagnose_overlay(&upscaled, &frame, &dst)
    };
    if let Err(e) = render_result {
        eprintln!("render diagnose overlay: {e}");
        return ExitCode::from(2);
    }
    println!(
        "\nwrote diagnose overlay → {}",
        dst.strip_prefix(workspace_root()).unwrap_or(&dst).display()
    );

    if let Some(dump_path) = args.dump_frame.as_deref() {
        let dump_dst = workspace_root().join(dump_path);
        if let Some(parent) = dump_dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("create dump-frame parent dir: {e}");
                return ExitCode::from(2);
            }
        }
        let json = match serde_json::to_string_pretty(&frame) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("serialize debug frame: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = std::fs::write(&dump_dst, json) {
            eprintln!("write debug frame to {}: {e}", dump_dst.display());
            return ExitCode::from(2);
        }
        println!(
            "wrote debug frame → {}",
            dump_dst
                .strip_prefix(workspace_root())
                .unwrap_or(&dump_dst)
                .display()
        );
    }

    ExitCode::SUCCESS
}

fn print_stage_summary(label: &str, frame: &calib_targets::chessboard::diagnostics::DebugFrame) {
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for aug in &frame.corners {
        let key: &'static str = match &aug.stage {
            CornerStage::Raw => "Raw",
            CornerStage::Strong => "Strong",
            CornerStage::NoCluster { .. } => "NoCluster",
            CornerStage::Clustered { .. } => "Clustered",
            CornerStage::AttachmentAmbiguous { .. } => "AttachmentAmbiguous",
            CornerStage::AttachmentFailedInvariants { .. } => "AttachmentFailedInvariants",
            CornerStage::Labeled { .. } => "Labeled",
            CornerStage::LabeledThenBlacklisted { .. } => "LabeledThenBlacklisted",
            _ => "Other",
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    println!("--- {label} ---");
    println!("  input corners: {}", frame.input_count);
    for (k, v) in &counts {
        println!("  {k:>30}: {v}");
    }
    if !frame.iterations.is_empty() {
        println!("  --- validation iterations ---");
        for it in &frame.iterations {
            println!(
                "  iter {}: labelled={} new_blacklist={} converged={}",
                it.iter,
                it.labelled_count,
                it.new_blacklist.len(),
                it.converged
            );
            if let Some(ext) = &it.extension {
                let med = ext
                    .h_residual_median_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                let max = ext
                    .h_residual_max_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                println!(
                    "    stage6: h_trusted={} median_res={} px max_res={} px iters={} attached={} \
                     rej(no_cand={} ambig={} label={} policy={} edge={})",
                    ext.h_trusted,
                    med,
                    max,
                    ext.iterations,
                    ext.attached,
                    ext.rejected_no_candidate,
                    ext.rejected_ambiguous,
                    ext.rejected_label,
                    ext.rejected_policy,
                    ext.rejected_edge,
                );
            }
            if let Some(rescue) = &it.rescue {
                let med = rescue
                    .h_residual_median_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                let max = rescue
                    .h_residual_max_px
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "—".to_string());
                println!(
                    "    stage6.5: h_trusted={} median_res={} px max_res={} px iters={} attached={} \
                     rej(no_cand={} ambig={} label={} policy={} edge={})",
                    rescue.h_trusted,
                    med,
                    max,
                    rescue.iterations,
                    rescue.attached,
                    rescue.rejected_no_candidate,
                    rescue.rejected_ambiguous,
                    rescue.rejected_label,
                    rescue.rejected_policy,
                    rescue.rejected_edge,
                );
            }
        }
    }
    if let Some(b) = &frame.boosters {
        println!("  boosters: {b:?}");
    }
    if let Some(d) = &frame.detection {
        println!(
            "  detection: {} labelled corners, cell_size = {:.2} px",
            d.corners.len(),
            frame.cell_size.unwrap_or(0.0)
        );
        // Print bbox of labelled set.
        let mut min_i = i32::MAX;
        let mut max_i = i32::MIN;
        let mut min_j = i32::MAX;
        let mut max_j = i32::MIN;
        for lc in &d.corners {
            min_i = min_i.min(lc.grid.i);
            max_i = max_i.max(lc.grid.i);
            min_j = min_j.min(lc.grid.j);
            max_j = max_j.max(lc.grid.j);
        }
        if min_i != i32::MAX {
            println!(
                "  labelled bbox: i ∈ [{min_i}, {max_i}], j ∈ [{min_j}, {max_j}]  ({}×{})",
                max_i - min_i + 1,
                max_j - min_j + 1,
            );
        }
    } else {
        println!("  detection: NONE");
    }
}

pub(crate) fn diagnose_filename(label: &str) -> String {
    let safe = label.replace(['/', '#'], "_");
    format!("{safe}.diagnose.png")
}

/// Topological-pipeline diagnostics: run the production detector path,
/// print per-component sizes, render an overlay showing labelled vs
/// unlabelled corners, and report which pre-filter step dropped the
/// unlabelled ones.
fn diagnose_topological(
    args: &DiagnoseArgs,
    upscaled: &image::GrayImage,
    corners: &[calib_targets::chessboard::ChessCorner],
) -> ExitCode {
    let mut detector_params = match load_chessboard_config(args.chessboard_config.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("load --chessboard-config: {e}");
            return ExitCode::from(2);
        }
    };
    detector_params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    detector_params.orientation_source = args.orientation_source.into();
    if let Some(deg) = args.axis_align_tol_deg {
        let mut advanced: AdvancedTuning = detector_params.effective_tuning().into_owned();
        advanced.topological.axis_align_tol_rad = deg.to_radians();
        detector_params = detector_params.with_advanced(advanced);
    }
    let tuning = detector_params.effective_tuning();
    let params = &tuning.topological;
    println!(
        "--- {} (topological) ---\n  input corners: {}\n  axis_align_tol_rad: {:.3} ({}°)  max_axis_sigma_rad: {:.3} ({}°)  cluster_axis_tol_rad: {:.3} ({}°)  edge_length_max_rel: {:.2}",
        args.image,
        corners.len(),
        params.axis_align_tol_rad,
        (params.axis_align_tol_rad.to_degrees() as i32),
        params.max_axis_sigma_rad,
        (params.max_axis_sigma_rad.to_degrees() as i32),
        params.cluster_axis_tol_rad,
        (params.cluster_axis_tol_rad.to_degrees() as i32),
        params.edge_length_max_rel,
    );

    // Pre-filter: at least one axis with sigma below threshold AND the
    // standard chessboard strength + fit-quality gates.
    let mut survives_strength = 0usize;
    let mut survives_fit = 0usize;
    let mut survives_axis = 0usize;
    for c in corners {
        let strong = c.strength >= detector_params.min_corner_strength;
        let fit_ok = !tuning.max_fit_rms_ratio.is_finite()
            || c.contrast <= 0.0
            || c.fit_rms <= tuning.max_fit_rms_ratio * c.contrast;
        let axis_ok = c.axes[0].sigma < params.max_axis_sigma_rad
            || c.axes[1].sigma < params.max_axis_sigma_rad;
        if strong {
            survives_strength += 1;
        }
        if strong && fit_ok {
            survives_fit += 1;
        }
        if strong && fit_ok && axis_ok {
            survives_axis += 1;
        }
    }
    println!(
        "  pre-filter: strength→{} fit→{} axis→{} (lost {} on axis sigma alone)",
        survives_strength,
        survives_fit,
        survives_axis,
        survives_fit - survives_axis,
    );

    let detections = Detector::new(detector_params)
        .expect("valid detector params")
        .detect_all(corners);
    let labelled_corner_set: std::collections::HashSet<usize> = detections
        .iter()
        .flat_map(|d| d.corners.iter().map(|c| c.input_index))
        .collect();
    println!(
        "  production components: {}  labelled corners (unique across components): {} / {} input",
        detections.len(),
        labelled_corner_set.len(),
        corners.len(),
    );
    for (k, detection) in detections.iter().enumerate() {
        let min_i = detection
            .corners
            .iter()
            .map(|c| c.grid.i)
            .min()
            .unwrap_or(0);
        let max_i = detection
            .corners
            .iter()
            .map(|c| c.grid.i)
            .max()
            .unwrap_or(0);
        let min_j = detection
            .corners
            .iter()
            .map(|c| c.grid.j)
            .min()
            .unwrap_or(0);
        let max_j = detection
            .corners
            .iter()
            .map(|c| c.grid.j)
            .max()
            .unwrap_or(0);
        println!(
            "  component {k}: labelled={} bbox=i[{min_i},{max_i}] j[{min_j},{max_j}] ({}×{})",
            detection.corners.len(),
            max_i - min_i + 1,
            max_j - min_j + 1,
        );
    }

    // Bin the unlabelled corner positions into quadrants so we can see
    // *where* the dropouts cluster (top-left, bottom-right, etc.).
    let (img_w, img_h) = upscaled.dimensions();
    let half_w = img_w as f32 * 0.5;
    let half_h = img_h as f32 * 0.5;
    let mut q_lab = [0usize; 4];
    let mut q_unl = [0usize; 4];
    let mut unlabelled_positions: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (k, c) in corners.iter().enumerate() {
        let qx = if c.position.x < half_w { 0 } else { 1 };
        let qy = if c.position.y < half_h { 0 } else { 1 };
        let q = qy * 2 + qx;
        if labelled_corner_set.contains(&k) {
            q_lab[q] += 1;
        } else {
            q_unl[q] += 1;
            unlabelled_positions.push((
                c.position.x,
                c.position.y,
                c.axes[0].sigma,
                c.axes[1].sigma,
            ));
        }
    }
    println!("\n  per-quadrant labelled / unlabelled (bottom-left = corners with x<W/2, y>H/2):");
    println!(
        "    TL: {:>4}/{:<4}    TR: {:>4}/{:<4}",
        q_lab[0], q_unl[0], q_lab[1], q_unl[1],
    );
    println!(
        "    BL: {:>4}/{:<4}    BR: {:>4}/{:<4}",
        q_lab[2], q_unl[2], q_lab[3], q_unl[3],
    );
    if !unlabelled_positions.is_empty() {
        println!("\n  unlabelled corner positions (x, y, axis0_sigma_deg, axis1_sigma_deg):");
        // Sort by y descending so bottom-of-image first; cap output.
        let mut sorted = unlabelled_positions.clone();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (i, (x, y, s0, s1)) in sorted.iter().take(20).enumerate() {
            println!(
                "    [{:>2}] ({:>6.1}, {:>6.1})  σ0={:>5.1}°  σ1={:>5.1}°",
                i,
                x,
                y,
                s0.to_degrees(),
                s1.to_degrees()
            );
        }
        if sorted.len() > 20 {
            println!("    ... ({} more)", sorted.len() - 20);
        }
    }

    // Render an overlay: green dots = labelled corners, red dots = corners
    // dropped by the pre-filter or classification.
    let label = args.image.clone();
    let dst = args.out.as_deref().map_or_else(
        || {
            workspace_root().join("preview/diagnose").join(format!(
                "{}.topological.png",
                diagnose_filename(&label).trim_end_matches(".png")
            ))
        },
        |p| workspace_root().join(p),
    );
    if let Err(e) = render_topological_overlay(upscaled, corners, &labelled_corner_set, &dst) {
        eprintln!("render topological overlay: {e}");
        return ExitCode::from(2);
    }
    println!(
        "\nwrote topological overlay → {}",
        dst.strip_prefix(workspace_root()).unwrap_or(&dst).display(),
    );
    ExitCode::SUCCESS
}

fn render_topological_overlay(
    base: &image::GrayImage,
    corners: &[calib_targets::chessboard::ChessCorner],
    labelled: &std::collections::HashSet<usize>,
    dst: &Path,
) -> std::io::Result<()> {
    use image::{Rgb, RgbImage};
    let (w, h) = base.dimensions();
    let mut rgb = RgbImage::new(w, h);
    for (x, y, p) in base.enumerate_pixels() {
        rgb.put_pixel(x, y, Rgb([p[0], p[0], p[0]]));
    }
    let stamp = |rgb: &mut RgbImage, cx: f32, cy: f32, color: [u8; 3]| {
        let r = 2i32;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                    continue;
                }
                rgb.put_pixel(x as u32, y as u32, Rgb(color));
            }
        }
    };
    for (k, c) in corners.iter().enumerate() {
        let color = if labelled.contains(&k) {
            [50, 220, 80] // green = labelled
        } else {
            [220, 50, 50] // red = dropped
        };
        stamp(&mut rgb, c.position.x, c.position.y, color);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    rgb.save(dst).map_err(std::io::Error::other)
}
