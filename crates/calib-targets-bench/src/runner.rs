//! Per-image chessboard detector runner.

use std::path::Path;
use std::time::Instant;

use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::detect_chessboard;
use image::{imageops::FilterType, GenericImageView, GrayImage, ImageReader};

use crate::baseline::{BaselineCorner, BaselineImage};
use crate::dataset::DatasetEntry;

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
pub fn run_entry(
    image_path: &Path,
    entry: &DatasetEntry,
    params: &DetectorParams,
) -> Result<Vec<RunOutcome>, std::io::Error> {
    let img = ImageReader::open(image_path)
        .map_err(|e| std::io::Error::other(format!("open {}: {e}", image_path.display())))?
        .decode()
        .map_err(|e| std::io::Error::other(format!("decode {}: {e}", image_path.display())))?
        .to_luma8();

    let snap_count = entry.snap_count();
    let mut out = Vec::with_capacity(snap_count as usize);
    for k in 0..snap_count {
        let (snap, origin) = extract_snap(&img, entry, k)?;
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

        let started = Instant::now();
        let detection = detect_chessboard(&upscaled, params);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1e3;

        let baseline_image = detection.as_ref().map(|d| {
            let mut corners: Vec<BaselineCorner> = d
                .target
                .corners
                .iter()
                .filter_map(|lc| {
                    let g = lc.grid?;
                    Some(BaselineCorner {
                        i: g.i,
                        j: g.j,
                        x: lc.position.x,
                        y: lc.position.y,
                        id: lc.id,
                        score: lc.score,
                    })
                })
                .collect();
            corners.sort_by_key(|c| (c.j, c.i));
            BaselineImage {
                labelled_count: corners.len(),
                cell_size_px: d.cell_size,
                corners,
            }
        });

        out.push(RunOutcome {
            label: entry.snap_label(k),
            snap_origin_in_source: origin,
            upscale: entry.upscale,
            elapsed_ms,
            detection: baseline_image,
            fed_image: upscaled,
        });
    }
    Ok(out)
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
