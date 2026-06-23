//! Per-image chessboard detector runner.

use std::path::Path;
use std::time::Instant;

use calib_targets::chessboard::{ChessCorner, Detector, DetectorParams};
use calib_targets::detect::{detect_corners, DetectorConfig};
use calib_targets_core::axis_estimate_to_next;
use image::{imageops::FilterType, GenericImageView, GrayImage, ImageReader};
use projective_grid::{
    detect_grid_all, DetectionParams, DetectionRequest, Evidence, GridSolution, LatticeKind,
    OrientedFeature, PointFeature,
};

use crate::baseline::{BaselineCorner, BaselineImage};
use crate::dataset::DatasetEntry;

/// Which detection engine the bench drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// Full chessboard production pipeline ([`Detector::detect`]): clustering,
    /// topological grid build + recovery boosters + geometry check.
    Pipeline,
    /// Raw [`detect_grid_all`] on the corner cloud, bypassing the chessboard
    /// recovery stages. Feeds the per-corner ChESS axes as
    /// [`Evidence::Oriented2`] and applies the `projective-grid` validate/fit
    /// directly, so it isolates the grid-builder layer from the chessboard
    /// recovery stages.
    Grid,
}

/// Outcome of detecting on a single (logical) sub-snap.
#[derive(Clone, Debug)]
pub struct RunOutcome {
    /// Logical label, e.g. `privatedata/.../target_15.png#3` for the 4th
    /// sub-snap of a stitched composite. Used as the baseline JSON key.
    pub label: String,
    /// Source-image-relative pixel position of the snap's top-left corner.
    /// `(0, 0)` for non-stitched entries.
    pub snap_origin_in_source: (u32, u32),
    /// Bilinear upscale that was applied before detection. Corner positions
    /// in `detection` are in **upscaled** pixel coordinates.
    pub upscale: u32,
    /// Wall-clock time for `detect_chessboard` only (does NOT include
    /// upscale resampling).
    pub elapsed_ms: f64,
    /// Detector output (`None` if no chessboard was found).
    pub detection: Option<BaselineImage>,
    /// The actual image fed to the detector (post-crop, post-upscale).
    /// Held so the overlay renderer can draw on the same coordinate space
    /// as the detector saw.
    pub fed_image: GrayImage,
}

/// Decode an image from disk, apply the entry's stitched/upscale spec,
/// and return one [`RunOutcome`] per sub-snap.
///
/// `chess_cfg` controls the low-level ChESS corner detector (e.g.
/// `orientation_method`). Pass `&default_chess_config()` for the standard
/// workspace defaults.
pub fn run_entry(
    image_path: &Path,
    entry: &DatasetEntry,
    params: &DetectorParams,
    chess_cfg: &DetectorConfig,
    engine: Engine,
) -> Result<Vec<RunOutcome>, std::io::Error> {
    let img = load_entry_image(image_path)?;
    let snap_count = entry.snap_count();
    let mut out = Vec::with_capacity(snap_count as usize);
    for k in 0..snap_count {
        out.push(run_snap(&img, entry, k, params, chess_cfg, engine)?);
    }
    Ok(out)
}

/// Decode an entry's source image (the full stitched composite for stitched
/// entries) into a grayscale buffer ready for [`run_snap`].
pub fn load_entry_image(image_path: &Path) -> Result<GrayImage, std::io::Error> {
    Ok(ImageReader::open(image_path)
        .map_err(|e| std::io::Error::other(format!("open {}: {e}", image_path.display())))?
        .decode()
        .map_err(|e| std::io::Error::other(format!("decode {}: {e}", image_path.display())))?
        .to_luma8())
}

/// Crop one sub-snap out of an already-decoded source image, apply the
/// entry's upscale, and return the fed image (post-crop, post-upscale)
/// without running detection. This is the exact image every detection
/// coordinate refers to.
pub fn fed_image(
    img: &GrayImage,
    entry: &DatasetEntry,
    k: u32,
) -> Result<GrayImage, std::io::Error> {
    let (snap, _) = extract_snap(img, entry, k)?;
    Ok(upscale_snap(snap, entry.upscale))
}

/// Run detection on one sub-snap of an already-decoded source image.
/// `img` must be the entry's full source image (see [`load_entry_image`]);
/// `k` selects the sub-snap (`0` for non-stitched entries).
pub fn run_snap(
    img: &GrayImage,
    entry: &DatasetEntry,
    k: u32,
    params: &DetectorParams,
    chess_cfg: &DetectorConfig,
    engine: Engine,
) -> Result<RunOutcome, std::io::Error> {
    let (snap, origin) = extract_snap(img, entry, k)?;
    let upscaled = upscale_snap(snap, entry.upscale);

    let started = Instant::now();
    let corners = detect_corners(&upscaled, chess_cfg);
    // The stable `ChessboardDetection` carries `cell_size`, so the pipeline
    // baseline reads it straight off the hot `detect()` path — no
    // `DebugFrame` needed here (overlays still use one separately).
    let baseline_image = match engine {
        Engine::Pipeline => run_pipeline_engine(params, &corners),
        Engine::Grid => run_grid_engine(params, &corners),
    };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1e3;

    Ok(RunOutcome {
        label: entry.snap_label(k),
        snap_origin_in_source: origin,
        upscale: entry.upscale,
        elapsed_ms,
        detection: baseline_image,
        fed_image: upscaled,
    })
}

fn upscale_snap(snap: GrayImage, upscale: u32) -> GrayImage {
    if upscale > 1 {
        let (w, h) = snap.dimensions();
        image::imageops::resize(&snap, w * upscale, h * upscale, FilterType::Triangle)
    } else {
        snap
    }
}

/// Full chessboard production pipeline → [`BaselineImage`].
fn run_pipeline_engine(params: &DetectorParams, corners: &[ChessCorner]) -> Option<BaselineImage> {
    let d = Detector::new(params.clone()).ok()?.detect(corners)?;
    let cell_size_px = d.cell_size.unwrap_or(0.0);
    let mut out: Vec<BaselineCorner> = d
        .corners
        .iter()
        .map(|lc| BaselineCorner {
            i: lc.grid.u,
            j: lc.grid.v,
            x: lc.position.x,
            y: lc.position.y,
            id: None,
            score: lc.score,
        })
        .collect();
    out.sort_by_key(|c| (c.j, c.i));
    Some(BaselineImage {
        labelled_count: out.len(),
        cell_size_px,
        corners: out,
    })
}

/// Raw `projective-grid` grid builder → [`BaselineImage`], bypassing the
/// chessboard recovery stages. Feeds the per-corner ChESS axes as
/// [`Evidence::Oriented2`] over the corner cloud and returns the largest
/// labelled component (one board per frame, matching the pipeline engine's
/// best-detection semantics).
fn run_grid_engine(_params: &DetectorParams, corners: &[ChessCorner]) -> Option<BaselineImage> {
    let grid_params = DetectionParams::default();
    let feats: Vec<OrientedFeature<2>> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| {
            OrientedFeature::new(
                PointFeature::new(i, c.position),
                [
                    axis_estimate_to_next(c.axes[0]),
                    axis_estimate_to_next(c.axes[1]),
                ],
            )
        })
        .collect();
    let report = detect_grid_all(DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&feats),
        None,
        grid_params,
    ));
    let best = report
        .ok()?
        .solutions
        .into_iter()
        .max_by_key(|s| s.grid.entries.len())?;
    if best.grid.entries.is_empty() {
        return None;
    }
    Some(grid_solution_to_baseline(&best))
}

/// Adapt a `projective-grid` [`GridSolution`] into the bench's
/// [`BaselineImage`] shape so overlays + the wrong-label audit reuse unchanged.
/// `image_position` is the pixel-center position; `(coord.u, coord.v)` are the
/// lattice labels. The grid engine carries no per-corner score or cell size, so
/// both are left at `0.0` (the head-to-head metric is `labelled_count`).
fn grid_solution_to_baseline(sol: &GridSolution) -> BaselineImage {
    let mut out: Vec<BaselineCorner> = sol
        .grid
        .entries
        .iter()
        .map(|e| BaselineCorner {
            i: e.coord.u,
            j: e.coord.v,
            x: e.image_position.x,
            y: e.image_position.y,
            id: None,
            score: 0.0,
        })
        .collect();
    out.sort_by_key(|c| (c.j, c.i));
    BaselineImage {
        labelled_count: out.len(),
        cell_size_px: 0.0,
        corners: out,
    }
}

fn extract_snap(
    full: &GrayImage,
    entry: &DatasetEntry,
    k: u32,
) -> Result<(GrayImage, (u32, u32)), std::io::Error> {
    if let Some(spec) = entry.stitched.as_ref() {
        let x0 = k * spec.snap_width;
        let (full_w, full_h) = full.dimensions();
        if x0 + spec.snap_width > full_w || spec.snap_height > full_h {
            return Err(std::io::Error::other(format!(
                "stitched snap #{k} (origin {}, {}, size {}x{}) does not fit in {}x{}",
                x0, 0, spec.snap_width, spec.snap_height, full_w, full_h
            )));
        }
        let view = full
            .view(x0, 0, spec.snap_width, spec.snap_height)
            .to_image();
        Ok((view, (x0, 0)))
    } else {
        Ok((full.clone(), (0, 0)))
    }
}
