//! Render a PuzzlePole the way a camera would see it, then decode it.
//!
//! There is no cylindrical target in this workspace's image corpus to
//! photograph, and no mesh renderer to build one with. So this synthesises the
//! view analytically: every pixel casts a ray at a cylinder and the pattern is
//! evaluated as a continuous function of the surface point it lands on. No
//! texture, no mesh, no resampling — which means the picture is a statement
//! about *geometry* and nothing else, and a corner that lands in the wrong
//! place is the detector's doing, not the renderer's.
//!
//! It shares the renderer with the end-to-end tests, so what you are looking at
//! is the same image those assertions are made against.
//!
//! ```bash
//! # a view straight at the seam, from slightly above
//! cargo run -p calib-targets-puzzleboard --example render_puzzlepole
//!
//! # anywhere else on the circle, at any elevation
//! cargo run -p calib-targets-puzzleboard --example render_puzzlepole -- out 90 25
//! ```
//!
//! Writes `puzzlepole_view.png` (what the camera sees) and
//! `puzzlepole_detected.png` (the same frame with the decode drawn on it).

#[path = "../tests/support/cylinder.rs"]
mod cylinder;

use calib_targets_puzzleboard::{
    GrayImageView, PuzzlePoleDetector, PuzzlePoleParams, PuzzlePoleSpec,
};
use cylinder::Camera;
use image::{Rgb, RgbImage};
use nalgebra::Point2;
use std::path::Path;

/// Distance from the camera to the cylinder's *axis*, in millimetres.
const DISTANCE_MM: f32 = 520.0;
/// Focal length in pixels, chosen so the whole pole fits the frame.
const FX: f32 = 1050.0;
const WIDTH: u32 = 520;
const HEIGHT: u32 = 700;
/// The seam's ink. Not a pattern colour and not a corner colour.
const SEAM: Rgb<u8> = Rgb([51, 198, 227]);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "target/puzzlepole".into());
    let azimuth: f32 = parse(args.next(), 0.0)?;
    let elevation: f32 = parse(args.next(), 18.0)?;
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    // 24 pieces around, 12 along, 20 mm pieces. The circumference has to be a
    // supported period; the diameter is then a consequence, not a choice.
    let spec = PuzzlePoleSpec::new(24, 12, 20.0)?;
    println!(
        "pole: {} x {} pieces of {} mm -> {:.1} mm across, {:.0} mm tall",
        spec.circumference_squares,
        spec.axial_squares,
        spec.cell_size_mm,
        spec.diameter_mm(),
        spec.axial_extent_mm(),
    );
    println!(
        "view: azimuth {azimuth} deg, elevation {elevation} deg (azimuth 0 looks at the seam)"
    );

    let camera = Camera::looking_at_pole_from(
        azimuth,
        elevation,
        DISTANCE_MM,
        spec.axial_extent_mm() / 2.0,
        WIDTH,
        HEIGHT,
        FX,
    );
    let gray = cylinder::render(&spec, &camera);
    let view_path = out.join("puzzlepole_view.png");
    gray.save(&view_path)?;
    println!("{}", view_path.display());

    let detector = PuzzlePoleDetector::new(PuzzlePoleParams::for_pole(spec))?;
    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };
    let found = detector.detect(&view)?;

    let cols = spec.axial_corner_cols();
    let mut rings = vec![false; spec.circumference_squares as usize];
    for corner in &found.corners {
        rings[(corner.id / cols) as usize] = true;
    }
    println!(
        "decoded {} corners over {} of {} rings, mean confidence {:.3}, bit error {:.3}",
        found.corners.len(),
        rings.iter().filter(|seen| **seen).count(),
        spec.circumference_squares,
        found.decode.mean_confidence,
        found.decode.bit_error_rate,
    );

    let mut overlay = tint(&gray);

    // The seam first, so the corner marks land on top of it. It is drawn as a
    // line rather than merely coloured differently because it is the answer to
    // the question a reader actually has — *did it decode across the join* —
    // and a line is visible at a glance where a hue among twenty-four is not.
    let mut seam: Vec<_> = found
        .corners
        .iter()
        .filter(|corner| corner.id / cols == 0)
        .map(|corner| (corner.id % cols, corner.position))
        .collect();
    seam.sort_by_key(|(axial, _)| *axial);
    for pair in seam.windows(2) {
        draw_line(&mut overlay, pair[0].1, pair[1].1, SEAM);
    }

    for corner in &found.corners {
        // The id packs the pair, so unpacking it recovers where on the cylinder
        // this corner sits. Colour runs round the circumference, which makes
        // the wrap visible: follow the ramp and it closes on itself.
        let cyclic = corner.id / cols;
        let hue = f32::from(u16::try_from(cyclic).unwrap_or(u16::MAX))
            / spec.circumference_squares as f32;
        let (x, y) = (corner.position.x, corner.position.y);
        draw_disc(&mut overlay, x, y, 3.5, Rgb([255, 255, 255]));
        draw_disc(&mut overlay, x, y, 2.5, hue_rgb(hue));
    }
    let detected_path = out.join("puzzlepole_detected.png");
    overlay.save(&detected_path)?;
    println!("{}", detected_path.display());

    Ok(())
}

fn parse(arg: Option<String>, fallback: f32) -> Result<f32, std::num::ParseFloatError> {
    arg.map_or(Ok(fallback), |value| value.parse())
}

/// Fade the render toward white so the annotation layer reads on top of it.
///
/// The pattern is maximally black-and-white by design, which is exactly what
/// makes drawing on it hard: full-strength ink is indistinguishable from a
/// square. Washing the photograph out is the standard fix and costs nothing —
/// the picture underneath is not the evidence, the marks are.
fn tint(gray: &image::GrayImage) -> RgbImage {
    RgbImage::from_fn(gray.width(), gray.height(), |x, y| {
        let v = f32::from(gray.get_pixel(x, y).0[0]);
        let faded = (v + (255.0 - v) * 0.55).round() as u8;
        Rgb([faded, faded, faded])
    })
}

fn draw_disc(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, color: Rgb<u8>) {
    let r = radius.ceil() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if (dx * dx + dy * dy) as f32 > radius * radius {
                continue;
            }
            let (x, y) = (cx.round() as i32 + dx, cy.round() as i32 + dy);
            if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                img.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

/// A 1-pixel line, walked in whichever axis it spans further.
fn draw_line(img: &mut RgbImage, from: Point2<f32>, to: Point2<f32>, color: Rgb<u8>) {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0);
    for step in 0..=steps as i32 {
        let t = f32::from(u16::try_from(step).unwrap_or(u16::MAX)) / steps;
        draw_disc(img, from.x + dx * t, from.y + dy * t, 1.0, color);
    }
}

/// A fully-saturated colour at `hue` turns round the wheel.
fn hue_rgb(hue: f32) -> Rgb<u8> {
    let h = hue.rem_euclid(1.0) * 6.0;
    let sector = h.floor() as i32;
    let f = h - h.floor();
    let (up, down) = ((f * 255.0) as u8, ((1.0 - f) * 255.0) as u8);
    match sector {
        0 => Rgb([255, up, 0]),
        1 => Rgb([down, 255, 0]),
        2 => Rgb([0, 255, up]),
        3 => Rgb([0, down, 255]),
        4 => Rgb([up, 0, 255]),
        _ => Rgb([255, 0, down]),
    }
}
