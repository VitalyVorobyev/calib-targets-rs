//! The `diagnose` subcommand: the topological labelled-vs-unlabelled
//! breakdown plus a diagnostic overlay. This is the "why is this corner
//! missing?" tool — run before changing detector code.

use std::path::Path;
use std::process::ExitCode;

use calib_targets::chessboard::{AdvancedTuning, GraphBuildAlgorithm};
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_bench::dataset::{Dataset, DatasetEntry, ImageKind};
use calib_targets_bench::diagnose::TopologicalDiagnosis;
use calib_targets_bench::workspace_root;
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader};

use super::cli::DiagnoseArgs;
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
        None => DatasetEntry::single(base_path.to_string(), ImageKind::Public),
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
    // The topological builder is the only builder; always run its diagnosis.
    let _ = (sub_idx, base_path);
    diagnose_topological(&args, &upscaled, &corners)
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
    if let Some(deg) = args.axis_align_tol_deg {
        let mut advanced: AdvancedTuning = detector_params.effective_tuning().into_owned();
        advanced.topological.axis_align_tol_rad = deg.to_radians();
        detector_params = detector_params.with_advanced(advanced);
    }
    let diagnosis: TopologicalDiagnosis =
        calib_targets_bench::diagnose::diagnose_topological(&detector_params, corners);
    let tols = &diagnosis.effective_tols;
    println!(
        "--- {} (topological) ---\n  input corners: {}\n  axis_align_tol_rad: {:.3} ({}°)  max_axis_sigma_rad: {:.3} ({}°)  cluster_axis_tol_rad: {:.3} ({}°)  edge_length_max_rel: {:.2}",
        args.image,
        diagnosis.input_count,
        tols.axis_align_tol_rad,
        (tols.axis_align_tol_rad.to_degrees() as i32),
        tols.max_axis_sigma_rad,
        (tols.max_axis_sigma_rad.to_degrees() as i32),
        tols.cluster_axis_tol_rad,
        (tols.cluster_axis_tol_rad.to_degrees() as i32),
        tols.edge_length_max_rel,
    );
    println!(
        "  pre-filter: strength→{} fit→{} axis→{} (lost {} on axis sigma alone)",
        diagnosis.prefilter.survives_strength,
        diagnosis.prefilter.survives_fit,
        diagnosis.prefilter.survives_axis,
        diagnosis.prefilter.survives_fit - diagnosis.prefilter.survives_axis,
    );
    println!(
        "  production components: {}  labelled corners (unique across components): {} / {} input",
        diagnosis.components.len(),
        diagnosis.labelled_indices.len(),
        diagnosis.input_count,
    );
    for (k, comp) in diagnosis.components.iter().enumerate() {
        let [min_i, max_i, min_j, max_j] = comp.bbox;
        println!(
            "  component {k}: labelled={} bbox=i[{min_i},{max_i}] j[{min_j},{max_j}] ({}×{})",
            comp.labelled,
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
    for c in &diagnosis.corners {
        let qx = if c.x < half_w { 0 } else { 1 };
        let qy = if c.y < half_h { 0 } else { 1 };
        let q = qy * 2 + qx;
        if c.labelled {
            q_lab[q] += 1;
        } else {
            q_unl[q] += 1;
            unlabelled_positions.push((c.x, c.y, c.sigma0, c.sigma1));
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
    let labelled_corner_set: std::collections::HashSet<usize> =
        diagnosis.labelled_indices.iter().copied().collect();

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
