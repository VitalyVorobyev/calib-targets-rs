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
/// pixels, and long enough axially to reach the measured decode floor.
///
/// The axial count is not free. A pole of `A` squares has `A + 1` corner
/// columns, of which only the interior `A - 1` carry an X-junction — the two
/// end columns have squares on one side and blank paper on the other. The
/// measured floor wants 8 axial corners, so `A >= 9`.
fn test_pole() -> PuzzlePoleSpec {
    PuzzlePoleSpec::new(24, 12, 20.0).expect("24 is a supported circumference")
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

/// Does the grid builder survive a curved surface?
///
/// This is the measure-first step Gap 24 in
/// `docs/algorithms/algorithmic_gaps.md` asks for, and the answer measured here
/// is **yes** — which is not what the analysis predicted.
///
/// The concern is specific: `projective-grid`'s validation fits **straight**
/// total-least-squares lines to every grid row and column, and on a cylinder
/// the axial lines are generatrices and stay straight while the circumferential
/// ones lie on circles and project to conics. If that check fires it drops
/// genuine corners, and it is mandatory on the chessboard path.
///
/// It does not fire. At every geometry where corners are plentiful the grid
/// builds in a **single component** and keeps essentially all of them, at both
/// the smallest and the largest shipped circumference and at axis tilts to 45
/// degrees. The one row that yields nothing is `p = 48` at 45 degrees, where
/// only three corners are visible at all — a visibility limit, not a validator
/// failure, and excluded by the `detected >= 12` guard below rather than by
/// special-casing it.
///
/// The numbers are printed because the trend is the finding; the assertion only
/// catches a regression into outright failure.
#[test]
fn the_grid_builder_survives_a_cylinder() {
    use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};

    // Two parameters strain a straight-line prior, and both are swept.
    //
    // **Elevation.** At 0 the axis lies in the image plane and the
    // circumferential rows project to arcs of near-zero sagitta -- the *easy*
    // case. Tilting the axis out of the image plane is what makes them bow.
    //
    // **Circumference.** A row's sagitta in *cell* units goes as the radius in
    // cells, which is `p / 2pi`. So a 48-piece pole bows twice as hard per
    // visible arc as a 24-piece one, and is the worst case the crate ships.
    let mut failures: Vec<(u32, f32, usize, usize)> = Vec::new();
    for period in [24u32, 48] {
        let spec = PuzzlePoleSpec::new(period, 8, 20.0).expect("a supported circumference");
        for elevation in [0.0f32, 15.0, 30.0, 45.0] {
            let camera =
                Camera::looking_at_pole_from(0.0, elevation, 420.0, 80.0, 900, 700, 1400.0);
            let img = render(&spec, &camera);
            let expected = visible_corners(&spec, &camera, FAIR_FORESHORTENING);
            let found = detect_corners(&img);
            let (detected, _) = recall(&expected, &found, 2.0);

            let corners: Vec<_> =
                ChessDetector::new(default_chess_config().with_detection(|d| d.nms_radius = 3))
                    .expect("build ChESS detector")
                    .detect(&img)
                    .expect("ChESS detection")
                    .iter()
                    .map(|c| {
                        calib_targets_chessboard::ChessCorner::new(
                            nalgebra::Point2::new(c.x, c.y),
                            c.axes
                                .map(|a| {
                                    [
                                        calib_targets_core::AxisEstimate {
                                            angle: a[0].angle,
                                            sigma: a[0].sigma,
                                        },
                                        calib_targets_core::AxisEstimate {
                                            angle: a[1].angle,
                                            sigma: a[1].sigma,
                                        },
                                    ]
                                })
                                .expect("orientation fit enabled"),
                            c.response,
                        )
                    })
                    .collect();

            let mut params = ChessboardParams::default();
            params.min_corner_strength = 33.0;
            let grids = ChessboardDetector::new(params)
                .expect("chessboard detector")
                .detect_all(&corners);
            let labelled: usize = grids.iter().map(|g| g.corners.len()).sum();

            println!(
                "p={period:2} elevation={elevation:4.0} deg: {} expected, \
             {detected} ChESS-detected, {labelled} labelled across {} component(s)",
                expected.len(),
                grids.len()
            );

            // Collected rather than asserted per row, so one failure does not
            // hide the trend -- and the trend is the finding here.
            if detected >= 12 && (grids.is_empty() || labelled * 2 < detected) {
                failures.push((period, elevation, detected, labelled));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "grid assembly collapsed where corners were plentiful \
         (period, elevation, detected, labelled): {failures:?}"
    );
}

/// **The end-to-end claim.** Render a pole through a known camera, detect it,
/// and check every corner it returns is the corner that is actually there.
///
/// This is what the whole feature is for, and it is the only test that
/// exercises the cyclic decode against pixels rather than against synthetic
/// observations. Ground truth is the projection of `object_position`, so a
/// wrong ID shows up as a corner reported far from where its index says it
/// should be.
#[test]
fn a_rendered_pole_decodes_to_the_right_corners() {
    use calib_targets_puzzleboard::{PuzzlePoleDetector, PuzzlePoleParams};

    let spec = test_pole();
    for theta in [0.0f32, 41.0, 137.0, 250.0] {
        let camera = Camera::looking_at_pole(theta, 420.0, 80.0, 900, 700, 1400.0);
        let img = render(&spec, &camera);
        let expected = visible_corners(&spec, &camera, FAIR_FORESHORTENING);

        let view = calib_targets_core::GrayImageView {
            width: img.width() as usize,
            height: img.height() as usize,
            data: img.as_raw(),
        };
        let detector = PuzzlePoleDetector::new(PuzzlePoleParams::for_pole(spec)).expect("detector");
        let found = detector
            .detect(&view)
            .unwrap_or_else(|e| panic!("at {theta} deg the pole did not decode: {e}"));

        assert!(
            !found.corners.is_empty(),
            "at {theta} deg the decode returned no corners"
        );

        let mut errors: Vec<(i32, i32, f32)> = Vec::new();
        // **Precision.** Every returned corner must sit where its own indices
        // say it does. A wrong ID is the one failure this workspace's contract
        // does not allow, so this is an assertion about all of them, not most.
        for corner in &found.corners {
            let truth = camera
                .project(spec.object_position(corner.grid.u as u32, corner.grid.v as u32))
                .expect("a decoded corner is in front of the camera");
            let err = ((corner.position.x - truth.0).powi(2)
                + (corner.position.y - truth.1).powi(2))
            .sqrt();
            errors.push((corner.grid.u, corner.grid.v, err));
        }
        let bad: Vec<_> = errors.iter().filter(|(_, _, e)| *e > 3.0).collect();
        let worst = errors.iter().map(|(_, _, e)| *e).fold(0.0f32, f32::max);
        println!(
            "theta={theta:5.0}: {} corners, {} beyond 3 px, worst {worst:.2}",
            errors.len(),
            bad.len()
        );
        for (u, v, e) in bad.iter().take(8) {
            println!("    axial {u:3} cyclic {v:3}  {e:8.2} px");
        }
        assert!(bad.is_empty(), "wrong labels at {theta} deg");

        // **Recall.** Enough of what was visible came back to be useful.
        let hit = expected
            .iter()
            .filter(|e| {
                found
                    .corners
                    .iter()
                    .any(|c| c.grid.u == e.axial as i32 && c.grid.v == e.cyclic as i32)
            })
            .count();
        assert!(
            hit * 2 >= expected.len(),
            "at {theta} deg only {hit} of {} visible corners were decoded",
            expected.len()
        );
    }
}

/// **A fragment below the floor must fail, not guess.**
///
/// This is the contract, and it is the half that matters: a miss is
/// recoverable, a wrong corner ID is not. A view too tight to carry enough code
/// has to be refused, and refused by a *named* reason rather than by producing
/// nothing for an unclear cause.
#[test]
fn a_fragment_below_the_floor_is_refused() {
    use calib_targets_puzzleboard::{PuzzlePoleDetectError, PuzzlePoleDetector, PuzzlePoleParams};

    let spec = test_pole();
    // A long lens on a near pole: a handful of pieces fill the frame, so the
    // fragment cannot span the measured floor however clean the corners are.
    let camera = Camera::looking_at_pole(0.0, 420.0, 80.0, 420, 340, 7000.0);
    let img = render(&spec, &camera);

    let view = calib_targets_core::GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };
    let detector = PuzzlePoleDetector::new(PuzzlePoleParams::for_pole(spec)).expect("detector");

    match detector.detect(&view) {
        Ok(found) => panic!(
            "a fragment below the floor decoded to {} corners instead of being refused",
            found.corners.len()
        ),
        Err(
            PuzzlePoleDetectError::FragmentTooSmall { .. }
            | PuzzlePoleDetectError::NotEnoughEdges { .. }
            | PuzzlePoleDetectError::NotEnoughLogicalBits { .. }
            | PuzzlePoleDetectError::DecodeFailed
            | PuzzlePoleDetectError::ChessboardNotDetected,
        ) => {}
        Err(other) => panic!("refused for an unexpected reason: {other}"),
    }
}

/// The seam is decoded, not merely tolerated.
///
/// At azimuth 0 the seam faces the camera, so the visible fragment straddles
/// it — corners on both sides of the joint are in one view, and a decoder that
/// could not close the ring would either fail or split them across two
/// incompatible origins. This asserts the ring closed: indices from both ends
/// of the circumference range are present, and every one of them is where its
/// index says.
#[test]
fn a_seam_crossing_view_decodes_as_one_ring() {
    use calib_targets_puzzleboard::{PuzzlePoleDetector, PuzzlePoleParams};

    let spec = test_pole();
    let camera = Camera::looking_at_pole(0.0, 420.0, 80.0, 900, 700, 1400.0);
    let img = render(&spec, &camera);
    let view = calib_targets_core::GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };
    let found = PuzzlePoleDetector::new(PuzzlePoleParams::for_pole(spec))
        .expect("detector")
        .detect(&view)
        .expect("the seam view must decode");

    let period = spec.circumference_squares as i32;
    let cyclic: std::collections::BTreeSet<i32> = found.corners.iter().map(|c| c.grid.v).collect();
    assert!(
        cyclic.contains(&0),
        "the seam row itself was not decoded: {cyclic:?}"
    );
    assert!(
        cyclic.iter().any(|&k| k >= period - 3),
        "no corner from the far side of the seam was decoded: {cyclic:?}"
    );
    // ...and the fragment is a short arc, not most of the pole -- which is what
    // a bounding extent would report for indices at both ends of the range.
    assert!(
        cyclic.len() < period as usize,
        "a single view decoded the entire circumference"
    );
}
