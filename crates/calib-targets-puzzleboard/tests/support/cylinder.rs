//! A deterministic analytic renderer for PuzzlePole targets.
//!
//! There is no cylindrical fixture to photograph and no renderer in the
//! workspace that can produce one, so the end-to-end tests need their own. This
//! is that renderer: for each output pixel it casts a ray, intersects the
//! cylinder, and evaluates the pattern **analytically** at the surface point —
//! no texture, no mesh, no resampling, so there is no interpolation error to
//! confuse with a detector error.
//!
//! It is deliberately not photo-realistic. Blur, noise and vignetting would
//! make a failure ambiguous — was it the detector or the noise? — and the
//! existing `tools/synth_puzzleboard_photo.py` already covers that question for
//! the planar case. What this renderer provides is *geometry*: a cylinder, a
//! known camera, and exact ground truth for where every corner should land.
//!
//! # The pattern is evaluated the way it is drawn
//!
//! `calib-targets-print` draws a PuzzleBoard as checkerboard squares plus a
//! disc at every interior edge midpoint. This evaluates the same thing as a
//! function of continuous `(master_row, master_col)`, so the two cannot
//! disagree about where a dot sits. The one thing it must get right that the
//! flat renderer need not is the wrap: a pole's row coordinate is taken modulo
//! the circumference before it indexes the code.

#![allow(dead_code)]

use calib_targets_puzzleboard::code_maps::{horizontal_edge_bit, vertical_edge_bit};
use calib_targets_puzzleboard::PuzzlePoleSpec;
use nalgebra::{Matrix3, Point3, Vector3};

/// Samples per pixel side. 4 × 4 is enough that a corner's sub-pixel position
/// is not quantised by the render, and cheap at test resolutions.
const SUPERSAMPLE: u32 = 4;

/// A pinhole camera looking at a pole.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
    pub width: u32,
    pub height: u32,
    /// Rotation taking pole-frame directions into camera-frame directions.
    pub r_cam_pole: Matrix3<f32>,
    /// Pole-frame origin, expressed in the camera frame.
    pub t_cam_pole: Vector3<f32>,
}

impl Camera {
    /// A camera at `distance` from the pole's axis, on the azimuth `theta_deg`,
    /// looking at the point `look_at_z` up the pole from an elevation of
    /// `elevation_deg`.
    ///
    /// `theta_deg` is in the pole's own frame, so `0` looks straight at the
    /// seam — which is how a test asks for a seam-crossing view.
    ///
    /// **Elevation is the interesting parameter.** At `0` the camera is level
    /// with its target and the cylinder axis lies in the image plane, which is
    /// the *easy* case for a validator that assumes straight grid lines: the
    /// circumferential rows project to arcs of near-zero sagitta. Raise the
    /// elevation and the axis tilts out of the image plane, the rows bow, and a
    /// straight-line prior starts to strain. A test that only ever passes `0`
    /// is not testing curvature at all.
    pub fn looking_at_pole(
        theta_deg: f32,
        distance_mm: f32,
        look_at_z: f32,
        width: u32,
        height: u32,
        fx: f32,
    ) -> Self {
        Self::looking_at_pole_from(theta_deg, 0.0, distance_mm, look_at_z, width, height, fx)
    }

    /// As [`Camera::looking_at_pole`], with an explicit elevation.
    pub fn looking_at_pole_from(
        theta_deg: f32,
        elevation_deg: f32,
        distance_mm: f32,
        look_at_z: f32,
        width: u32,
        height: u32,
        fx: f32,
    ) -> Self {
        let theta = theta_deg.to_radians();
        let elevation = elevation_deg.to_radians();
        let target = Point3::new(0.0, 0.0, look_at_z);
        let eye = Point3::new(
            distance_mm * elevation.cos() * theta.cos(),
            distance_mm * elevation.cos() * theta.sin(),
            look_at_z + distance_mm * elevation.sin(),
        );

        let forward = (target - eye).normalize();
        // World up is the pole's own axis; the camera's "down" is whatever is
        // left after removing the forward component, so the image stays level.
        let world_up = Vector3::new(0.0, 0.0, 1.0);
        let right = forward.cross(&world_up).normalize();
        let down = forward.cross(&right);

        // Rows of R are the camera axes in pole coordinates, which is exactly
        // the map from pole directions to camera directions.
        let r_cam_pole =
            Matrix3::from_rows(&[right.transpose(), down.transpose(), forward.transpose()]);
        let t_cam_pole = -r_cam_pole * eye.coords;

        Self {
            fx,
            fy: fx,
            cx: width as f32 / 2.0,
            cy: height as f32 / 2.0,
            width,
            height,
            r_cam_pole,
            t_cam_pole,
        }
    }

    /// Project a pole-frame point into the image. `None` if it is behind the
    /// camera.
    pub fn project(&self, p: Point3<f32>) -> Option<(f32, f32)> {
        let cam = self.r_cam_pole * p.coords + self.t_cam_pole;
        if cam.z <= 1e-3 {
            return None;
        }
        Some((
            self.fx * cam.x / cam.z + self.cx,
            self.fy * cam.y / cam.z + self.cy,
        ))
    }

    /// Is this pole-frame surface point facing the camera?
    ///
    /// A cylinder self-occludes, so half its corners project into the image
    /// while sitting on the far side. Ground truth has to exclude them or a
    /// test would demand corners that are not there.
    pub fn sees_surface_point(&self, p: Point3<f32>) -> bool {
        let eye = -self.r_cam_pole.transpose() * self.t_cam_pole;
        let outward = Vector3::new(p.x, p.y, 0.0);
        let to_eye = eye - p.coords;
        outward.dot(&to_eye) > 0.0
    }
}

/// Render a pole. Returns a grayscale image, white background where no ray hits.
pub fn render(spec: &PuzzlePoleSpec, camera: &Camera) -> image::GrayImage {
    let radius = spec.radius_mm();
    let z_max = spec.axial_extent_mm();
    let mut img = image::GrayImage::from_pixel(camera.width, camera.height, image::Luma([255u8]));

    let inv_r = camera.r_cam_pole.transpose();
    let eye = -inv_r * camera.t_cam_pole;
    let step = 1.0 / SUPERSAMPLE as f32;

    for py in 0..camera.height {
        for px in 0..camera.width {
            let mut acc = 0.0f32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    // Pixel centres at +0.5, the workspace-wide convention.
                    let u = px as f32 + (sx as f32 + 0.5) * step;
                    let v = py as f32 + (sy as f32 + 0.5) * step;
                    let dir_cam = Vector3::new(
                        (u - camera.cx) / camera.fx,
                        (v - camera.cy) / camera.fy,
                        1.0,
                    );
                    let dir = inv_r * dir_cam;
                    acc += match hit_cylinder(eye, dir, radius, z_max) {
                        Some((theta, z)) => surface_value(spec, theta, z),
                        None => 1.0,
                    };
                }
            }
            let mean = acc / (SUPERSAMPLE * SUPERSAMPLE) as f32;
            img.put_pixel(px, py, image::Luma([(mean * 255.0).round() as u8]));
        }
    }
    img
}

/// Nearest intersection of a ray with the *outside* of a finite cylinder about
/// the Z axis, as `(theta, z)`.
fn hit_cylinder(
    origin: Vector3<f32>,
    dir: Vector3<f32>,
    radius: f32,
    z_max: f32,
) -> Option<(f32, f32)> {
    let a = dir.x * dir.x + dir.y * dir.y;
    if a < 1e-9 {
        return None;
    }
    let b = 2.0 * (origin.x * dir.x + origin.y * dir.y);
    let c = origin.x * origin.x + origin.y * origin.y - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    // The nearer positive root is the surface facing the camera; the farther is
    // the inside of the far wall, which is occluded.
    for s in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
        if s <= 1e-4 {
            continue;
        }
        let p = origin + dir * s;
        if p.z >= 0.0 && p.z <= z_max {
            return Some((p.y.atan2(p.x), p.z));
        }
        break;
    }
    None
}

/// The pattern's value at a surface point, `0.0` black to `1.0` white.
fn surface_value(spec: &PuzzlePoleSpec, theta: f32, z: f32) -> f32 {
    let period = spec.circumference_squares as f32;
    // Continuous corner coordinates. The row wraps at the period -- this is the
    // whole difference from a flat board.
    let k = theta.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * period;
    let row = spec.start_row as f32 + k;
    let col = spec.axial_start_col as f32 + z / spec.cell_size_mm;

    // Dots first: they are drawn over the squares.
    let dot_radius = 0.5 / 3.0;
    if let Some(bit) = nearest_dot(spec, row, col, dot_radius) {
        return if bit == 1 { 1.0 } else { 0.0 };
    }
    // Checkerboard. `build_puzzleboard` paints black where `(r + c)` is even.
    let square = row.floor() as i32 + col.floor() as i32;
    if square.rem_euclid(2) == 0 {
        0.0
    } else {
        1.0
    }
}

/// The bit of the edge dot covering `(row, col)`, if any.
///
/// Mirrors `calib-targets-print`'s placement exactly: a horizontal edge dot for
/// master edge `(mr, mc)` is centred at `(mr + 1, mc + 0.5)` and a vertical one
/// at `(mr + 0.5, mc + 1)`.
fn nearest_dot(spec: &PuzzlePoleSpec, row: f32, col: f32, dot_radius: f32) -> Option<u8> {
    let period = spec.circumference_squares as i32;
    let wrap_row = |mr: i32| {
        let local = mr - spec.start_row as i32;
        spec.start_row as i32 + local.rem_euclid(period)
    };

    // Horizontal edge: centre row is an integer, centre col a half-integer.
    let mr = (row - 1.0).round() as i32;
    let mc = col.floor() as i32;
    let (dr, dc) = (row - (mr as f32 + 1.0), col - (mc as f32 + 0.5));
    if dr * dr + dc * dc < dot_radius * dot_radius {
        return Some(horizontal_edge_bit(wrap_row(mr), mc));
    }

    // Vertical edge: centre row a half-integer, centre col an integer.
    let mr = row.floor() as i32;
    let mc = (col - 1.0).round() as i32;
    let (dr, dc) = (row - (mr as f32 + 0.5), col - (mc as f32 + 1.0));
    if dr * dr + dc * dc < dot_radius * dot_radius {
        return Some(vertical_edge_bit(wrap_row(mr), mc));
    }

    None
}

/// A corner the camera should see: its pole indices and where it projects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExpectedCorner {
    /// Axial corner index, along +Z.
    pub axial: u32,
    /// Cyclic corner index, around the circumference from the seam.
    pub cyclic: u32,
    /// Where it lands in the image.
    pub pixel: (f32, f32),
}

/// Every corner of the pole a camera could reasonably resolve. This is the
/// ground truth a detection is scored against.
///
/// `min_foreshortening` is the fraction of its face-on size a piece must still
/// project to. It is the cosine of the angle between the outward surface normal
/// and the direction to the camera, so `1.0` is dead-on and `0.0` is the
/// silhouette — and near the silhouette a piece is compressed to sub-pixel
/// width, where no corner detector can work.
///
/// Making this a parameter rather than a constant matters: recall measured
/// against *every* geometrically front-facing corner is measuring the limb, not
/// the detector, and would quietly punish a renderer for physics. A test states
/// the foreshortening it considers fair and is then judged on that.
pub fn visible_corners(
    spec: &PuzzlePoleSpec,
    camera: &Camera,
    min_foreshortening: f32,
) -> Vec<ExpectedCorner> {
    let eye = -camera.r_cam_pole.transpose() * camera.t_cam_pole;
    let mut out = Vec::new();
    // Interior corners only, in the axial direction. A corner on the pattern's
    // top or bottom edge has squares on one side and blank paper on the other,
    // so there is no X-junction there for a corner detector to find -- exactly
    // as for a flat board, whose resolved points are also interior-only.
    //
    // The circumference has no such edge: it wraps. That asymmetry is the whole
    // point of the target, and it is why this loop is not symmetric.
    for axial in 1..spec.axial_squares {
        for cyclic in 0..spec.circumference_squares {
            let p = spec.object_position(axial, cyclic);
            let outward = Vector3::new(p.x, p.y, 0.0).normalize();
            let to_eye = (eye - p.coords).normalize();
            if outward.dot(&to_eye) < min_foreshortening {
                continue;
            }
            let Some((u, v)) = camera.project(p) else {
                continue;
            };
            // A margin, because a corner on the image border has no
            // neighbourhood for the detector to work with.
            let m = 8.0;
            if u >= m && v >= m && u < camera.width as f32 - m && v < camera.height as f32 - m {
                out.push(ExpectedCorner {
                    axial,
                    cyclic,
                    pixel: (u, v),
                });
            }
        }
    }
    out
}
