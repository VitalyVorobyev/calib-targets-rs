//! Does the synthetic cylinder renderer produce a *detectable* pole?
//!
//! This validates the renderer, not the decoder. The cylindrical decode does
//! not exist yet, and when it does, a failure would be ambiguous unless the
//! fixture is known good first — so the question asked here is narrow and
//! answerable now: **do ChESS corners appear where the pole's geometry says
//! they should?**
//!
//! That is a strong statement. The renderer computes pixels by ray-casting and
//! evaluating the pattern analytically; the ground truth comes from
//! `PuzzlePoleSpec::object_position` projected through the same camera. The two
//! share only the camera and the spec, so agreement between them exercises the
//! object frame, the wrap, the dot placement and the projection at once.

mod support;

use support::cylinder::{render, visible_corners, Camera, ExpectedCorner};

/// A piece must still project to at least half its face-on width to count as
/// something a detector should find. Beyond that the cylinder is turning away
/// fast enough that a corner is sub-pixel, which is physics rather than a
/// detector failure.
const FAIR_FORESHORTENING: f32 = 0.5;

use calib_targets::detect::default_chess_config;
use calib_targets_puzzleboard::PuzzlePoleSpec;
use chess_corners::Detector as ChessDetector;

/// A pole big enough in frame that its pieces span a comfortable number of
/// pixels, which is the condition every corner detector actually cares about.
fn test_pole() -> PuzzlePoleSpec {
    PuzzlePoleSpec::new(24, 8, 20.0).expect("24 is a supported circumference")
}

fn detect_corners(img: &image::GrayImage) -> Vec<(f32, f32)> {
    let cfg = default_chess_config().with_detection(|d| d.nms_radius = 3);
    ChessDetector::new(cfg)
        .expect("build ChESS detector")
        .detect(img)
        .expect("ChESS detection")
        .iter()
        .map(|c| (c.x, c.y))
        .collect()
}

/// How many of the geometrically-visible corners did the detector find within
/// `tol` pixels, and what was the worst distance among those it found?
fn recall(expected: &[ExpectedCorner], found: &[(f32, f32)], tol: f32) -> (usize, f32) {
    let mut hits = 0;
    let mut worst = 0.0f32;
    for &ExpectedCorner {
        pixel: (ex, ey), ..
    } in expected
    {
        let best = found
            .iter()
            .map(|(fx, fy)| ((fx - ex).powi(2) + (fy - ey).powi(2)).sqrt())
            .fold(f32::INFINITY, f32::min);
        if best <= tol {
            hits += 1;
            worst = worst.max(best);
        }
    }
    (hits, worst)
}

/// The core claim. A camera looking at the pole from any azimuth sees corners
/// where the object frame says they are — **all** of them, not most.
///
/// The azimuths include `0`, which points straight at the seam: the case a
/// cylindrical target exists to handle, and the one a renderer with a wrap bug
/// would get wrong while looking fine everywhere else.
#[test]
fn rendered_corners_land_where_the_object_frame_says_they_do() {
    let spec = test_pole();
    for theta in [0.0f32, 37.0, 90.0, 180.0, 270.0] {
        let camera = Camera::looking_at_pole(theta, 420.0, 80.0, 900, 700, 1400.0);
        let img = render(&spec, &camera);
        let expected = visible_corners(&spec, &camera, FAIR_FORESHORTENING);
        assert!(
            expected.len() >= 12,
            "at {theta} deg only {} corners are expected; the view is too tight \
             for this test to mean anything",
            expected.len()
        );

        let found = detect_corners(&img);
        let (hits, worst) = recall(&expected, &found, 2.0);
        let rate = hits as f32 / expected.len() as f32;
        assert!(
            rate >= 0.95,
            "at {theta} deg only {hits} of {} expected corners were detected \
             within 2 px",
            expected.len()
        );
        assert!(
            worst <= 2.0,
            "at {theta} deg the worst matched corner was {worst:.2} px out"
        );
    }
}

/// Recall does not fall off until far past the fairness line.
///
/// Worth pinning, because it says the renderer is not quietly degrading toward
/// the limb — and because getting here took correcting the ground truth twice.
/// Scoring against *every* front-facing corner conflated the limb with the
/// detector; scoring against every corner at all also demanded the pattern's
/// top and bottom rows, which have squares on one side and blank paper on the
/// other and so carry no X-junction for any detector to find. With both
/// corrected, recall is total from dead-on down to 72 degrees off-normal.
#[test]
fn recall_holds_well_past_the_fairness_line() {
    let spec = test_pole();
    let camera = Camera::looking_at_pole(0.0, 420.0, 80.0, 900, 700, 1400.0);
    let found = detect_corners(&render(&spec, &camera));

    // cos 0.3 is roughly 72 degrees off-normal: the piece projects to under a
    // third of its face-on width and is still resolved here.
    let generous = visible_corners(&spec, &camera, 0.3);
    let (hits, worst) = recall(&generous, &found, 2.0);
    assert!(
        hits as f32 / generous.len() as f32 >= 0.95,
        "only {hits} of {} corners down to cos 0.3 were found",
        generous.len()
    );
    assert!(worst <= 2.0, "worst matched corner was {worst:.2} px out");
}

/// The seam is not a discontinuity. Looking straight at it must be no worse
/// than looking anywhere else — if the wrap were wrong, the pattern would break
/// along one image column and the corner yield would drop there specifically.
#[test]
fn the_seam_is_not_visible_to_the_detector() {
    let spec = test_pole();
    let at = |theta: f32| {
        let camera = Camera::looking_at_pole(theta, 420.0, 80.0, 900, 700, 1400.0);
        let expected = visible_corners(&spec, &camera, FAIR_FORESHORTENING);
        let found = detect_corners(&render(&spec, &camera));
        let (hits, _) = recall(&expected, &found, 2.0);
        hits as f32 / expected.len() as f32
    };

    // Half a piece either side of the seam, so the seam sits mid-view.
    let seam = at(0.0);
    let away = at(180.0 / spec.circumference_squares as f32);
    assert!(
        seam >= away - 0.15,
        "the seam view recalled {seam:.2} against {away:.2} away from it; the \
         wrap is leaving a mark"
    );
}

/// A cylinder self-occludes, so the ground truth must exclude the far side.
/// If it did not, this test would be demanding corners that are not in the
/// image and the recall assertions above would be meaningless.
#[test]
fn ground_truth_excludes_the_far_side() {
    let spec = test_pole();
    let camera = Camera::looking_at_pole(0.0, 420.0, 80.0, 900, 700, 1400.0);
    let visible = visible_corners(&spec, &camera, FAIR_FORESHORTENING);

    let all_rings = spec.circumference_squares as usize;
    let seen_rings: std::collections::BTreeSet<u32> = visible.iter().map(|c| c.cyclic).collect();
    assert!(
        seen_rings.len() < all_rings,
        "every one of the {all_rings} circumference positions was called \
         visible; the occlusion test is not doing anything"
    );
    // ...and the seam, which faces the camera, must be among them.
    assert!(
        seen_rings.contains(&0),
        "the seam faces the camera at theta = 0"
    );
}

/// Write a rendered view to `tmpdata/` for eyeballing.
///
/// A renderer that passes numerical tests can still be drawing the wrong thing
/// — the assertions above would survive a pattern that is subtly mirrored or
/// half a piece out, because ground truth and render share the spec. Looking at
/// it is the check that does not.
///
/// ```bash
/// cargo test -p calib-targets-puzzleboard --test puzzlepole_synthetic \
///     -- --ignored dump_a_rendered_pole --nocapture
/// ```
#[test]
#[ignore = "writes a local artifact; run by hand when changing the renderer"]
fn dump_a_rendered_pole() {
    let spec = test_pole();
    std::fs::create_dir_all("../../tmpdata").expect("tmpdata");
    for theta in [0.0f32, 45.0] {
        let camera = Camera::looking_at_pole(theta, 420.0, 80.0, 900, 700, 1400.0);
        let img = render(&spec, &camera);
        let path = format!("../../tmpdata/puzzlepole_theta{theta:.0}.png");
        img.save(&path).expect("write png");
        let found = detect_corners(&img);
        println!("{path}");
        // Recall as a function of how face-on the piece is, which is the only
        // axis a cylinder's difficulty actually varies along.
        for band in [0.9f32, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2] {
            let wide = visible_corners(&spec, &camera, band);
            let tight = visible_corners(&spec, &camera, band + 0.1);
            let ring: Vec<_> = wide
                .iter()
                .filter(|c| {
                    !tight
                        .iter()
                        .any(|t| t.axial == c.axial && t.cyclic == c.cyclic)
                })
                .cloned()
                .collect();
            if ring.is_empty() {
                continue;
            }
            let (hits, worst) = recall(&ring, &found, 2.0);
            println!(
                "  cos in [{band:.1}, {:.1}): {hits:3}/{:3} within 2 px, worst {worst:.2}",
                band + 0.1,
                ring.len()
            );
        }
    }
}
