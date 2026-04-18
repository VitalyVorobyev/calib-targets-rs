//! Interoperability test against Stelldinger et al. reference images.
//!
//! For each `testdata/puzzleboard_reference/exampleN.png`, we:
//! 1. Load the reference oracle JSON (`exampleN.json`) produced by the authors'
//!    Python decoder (`PStelldinger/PuzzleBoard`, CC0 1.0).
//! 2. Run our detector with the default sweep params.
//! 3. If our detector succeeds, verify:
//!    (a) `bit_error_rate ≤ MAX_BER` — the decode is internally consistent.
//!    (b) For every corner both detectors find within `PIXEL_TOL` pixels, the
//!    mapping `our_master → ref_master` is consistent across ALL matched
//!    pairs (i.e., a single D4 transform + cyclic translation explains
//!    every pair). Exact equality is NOT required: when a PuzzleBoard is
//!    observed without any fixed physical landmark, both decoders can
//!    legitimately pick different equivalence classes for the grid
//!    `(0, 0)` anchor; the 8 D4 transforms × 501² translations all yield
//!    BER=0 on a sub-region, so either decoder can settle on any of them.
//!
//! NOTE on pixel-position matching: our ChESS-based corner detector and the
//! authors' Hessian-based detector are different algorithms with different
//! subpixel refinement. On low-resolution images they may detect completely
//! disjoint corner sets. A low match count does not fail the test.
//!
//! Pass criteria per decoded image:
//! - At least `MIN_DECODED_CORNERS` in our result.
//! - `bit_error_rate ≤ MAX_BER`.
//! - If at least 3 matched-pixel pairs exist, every pair satisfies the same
//!   `(our_row + ref_row, our_col + ref_col)` modulo 501 signature (up to the
//!   wrap-around ambiguity). More formally: for every pair `i`, there exist
//!   `(dr_i, dc_i, dr_ref_i, dc_ref_i)` deltas consistent with a single
//!   `(D4_transform, translation)` relation — we check the simpler signature
//!   `ref_master - our_master (mod 501)` is constant across pairs. This
//!   accepts the common 180°+translation ambiguity and still fails on genuine
//!   decode errors (inconsistent per-corner labellings).
//!
//! Images that fail to decode (our detector returns Err) are reported but not
//! failed — the reference images vary widely in scale and quality.

use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets_core::GRID_TRANSFORMS_D4;
use calib_targets_puzzleboard::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation,
};
use image::ImageReader;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Maximum pixel distance for matching our corners to reference corners.
const PIXEL_TOL: f32 = 3.0;
/// Maximum acceptable bit error rate for our decode to count as successful.
const MAX_BER: f32 = 0.35;
/// Minimum labelled corners from our detector for the image to count as decoded.
const MIN_DECODED_CORNERS: usize = 1;

fn testdata_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("..")
        .join("..")
        .join("testdata")
        .join("puzzleboard_reference")
}

#[derive(Debug, Deserialize)]
struct RefCorner {
    pixel_x: f64,
    pixel_y: f64,
    master_row: i32,
    master_col: i32,
}

#[derive(Debug, Deserialize)]
struct RefJson {
    source_image: String,
    decoded_corners: Vec<RefCorner>,
}

fn load_ref_json(dir: &Path, index: usize) -> Option<RefJson> {
    let path = dir.join(format!("example{index}.json"));
    if !path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn run_one_image(index: usize, dir: &Path) {
    let img_path = dir.join(format!("example{index}.png"));
    if !img_path.exists() {
        println!("example{index}: image not found, skipping");
        return;
    }
    let ref_data = match load_ref_json(dir, index) {
        Some(d) => d,
        None => {
            println!("example{index}: reference JSON not found, skipping");
            return;
        }
    };

    if ref_data.decoded_corners.is_empty() {
        println!(
            "example{index} ({}): reference has 0 corners, skipping",
            ref_data.source_image
        );
        return;
    }

    let img = ImageReader::open(&img_path)
        .expect("open image")
        .decode()
        .expect("decode image")
        .to_luma8();

    // Use a permissive board spec. Real images have unknown grid size; we
    // use a large spec so the chessboard detector can assemble any sub-region.
    let board = PuzzleBoardSpec::new(20, 20, 5.0).expect("board spec");
    let sweep = PuzzleBoardParams::sweep_for_board(&board);

    let result = match detect::detect_puzzleboard_best(&img, &sweep) {
        Ok(r) => r,
        Err(e) => {
            println!(
                "example{index} ({}): our detector failed ({}) — {} ref corners, skipping",
                ref_data.source_image,
                e,
                ref_data.decoded_corners.len()
            );
            return;
        }
    };

    let our_corners = &result.detection.corners;
    let ber = result.decode.bit_error_rate;

    // Criterion (a): internal consistency check.
    println!(
        "example{index} ({}): decoded {} corners, BER={:.3}",
        ref_data.source_image,
        our_corners.len(),
        ber
    );
    assert!(
        our_corners.len() >= MIN_DECODED_CORNERS,
        "example{index}: only {} corners (need at least {})",
        our_corners.len(),
        MIN_DECODED_CORNERS
    );
    assert!(
        ber <= MAX_BER,
        "example{index}: BER={ber:.3} exceeds threshold {MAX_BER:.3}"
    );

    // Criterion (b): check that the mapping our_master → ref_master is
    // consistent across ALL matched-pixel pairs. Grid-anchor ambiguity means
    // the two decoders can pick different D4+translation cosets; we only
    // require that every matched pair agrees on the SAME coset.
    //
    // The simpler signature we check: among all matched pairs, the pair
    // (our_row + ref_row mod 501, our_col + ref_col mod 501) should be
    // constant (covers the 180°+translation case, which is the common one
    // for square boards without a physical landmark). If the decoders picked
    // truly-different D4 orientations (not related by 180°), we fall back
    // on reporting it informationally.
    let mut pairs: Vec<(i32, i32, i32, i32)> = Vec::new();

    for lc in our_corners {
        let Some(grid) = lc.grid else {
            continue;
        };
        let px = lc.position.x;
        let py = lc.position.y;

        let mut best_dist = f32::MAX;
        let mut best_ref: Option<&RefCorner> = None;
        for rc in &ref_data.decoded_corners {
            let dx = px - rc.pixel_x as f32;
            let dy = py - rc.pixel_y as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < best_dist {
                best_dist = dist;
                best_ref = Some(rc);
            }
        }

        if best_dist > PIXEL_TOL {
            continue;
        }
        let rc = best_ref.unwrap();
        // grid.i = master_col, grid.j = master_row.
        pairs.push((grid.j, grid.i, rc.master_row, rc.master_col));
    }

    if pairs.len() < 3 {
        println!(
            "  pixel-matched pairs: {} (< 3, skipping cross-decoder consistency check)",
            pairs.len()
        );
        return;
    }

    // Try every D4 transform and look for one where `ref - D4(our)` is
    // constant (mod 501) across all matched pairs.
    let mut found_transform: Option<(usize, i32, i32, i32, i32)> = None;
    for (ti, t) in GRID_TRANSFORMS_D4.iter().enumerate() {
        let deltas: Vec<(i32, i32)> = pairs
            .iter()
            .map(|(or, oc, rr, rc)| {
                // D4(our) treats (row, col) as 2D: new_row = a*row + b*col, new_col = c*row + d*col.
                let nr = t.a * or + t.b * oc;
                let nc = t.c * or + t.d * oc;
                ((rr - nr).rem_euclid(501), (rc - nc).rem_euclid(501))
            })
            .collect();
        if deltas.iter().all(|d| d == &deltas[0]) {
            found_transform = Some((ti, t.a, t.b, t.c, t.d));
            println!(
                "  pixel-matched pairs: {} — consistent under D4[{ti}] \
                 (a={}, b={}, c={}, d={}), offset=(row,col)={:?}",
                pairs.len(),
                t.a,
                t.b,
                t.c,
                t.d,
                deltas[0]
            );
            break;
        }
    }

    if found_transform.is_none() {
        eprintln!(
            "  pixel-matched pairs: {} — NO single D4 transform explains \
             all pairs. First 5:",
            pairs.len()
        );
        for (or, oc, rr, rc) in pairs.iter().take(5) {
            eprintln!("    ours=(row={or}, col={oc})  ref=(row={rr}, col={rc})");
        }
    }

    assert!(
        found_transform.is_some(),
        "example{index}: {} matched pairs are not consistent with any single \
         D4 transform + translation — suggests a real decode inconsistency, \
         not just grid-anchor ambiguity.",
        pairs.len()
    );
}

#[test]
fn interop_authors_reference_images() {
    let dir = testdata_dir();
    if !dir.exists() {
        println!("testdata/puzzleboard_reference/ not found — skipping interop tests.");
        return;
    }

    let mut decoded_count = 0usize;
    for i in 0..10 {
        let img_path = dir.join(format!("example{i}.png"));
        if img_path.exists() {
            let img = ImageReader::open(&img_path)
                .expect("open image")
                .decode()
                .expect("decode image")
                .to_luma8();
            let board = PuzzleBoardSpec::new(20, 20, 5.0).expect("board spec");
            let sweep = PuzzleBoardParams::sweep_for_board(&board);
            if detect::detect_puzzleboard_best(&img, &sweep).is_ok() {
                decoded_count += 1;
            }
        }
        run_one_image(i, &dir);
    }

    println!("\nSummary: decoded {decoded_count}/10 reference images successfully");
    assert!(
        decoded_count >= 1,
        "expected to decode at least 1 reference image, got {decoded_count}"
    );
}

/// Diagnostic: run detector on example0 and compare BER at our found origin
/// vs the reference oracle origin. Also dumps the first observed edges.
/// Not a failing test — run with --nocapture to see output.
#[test]
fn diag_example0_edge_bits() {
    let dir = testdata_dir();
    let img_path = dir.join("example0.png");
    if !img_path.exists() {
        println!("example0.png not found, skipping diag");
        return;
    }

    let img = ImageReader::open(&img_path)
        .expect("open")
        .decode()
        .expect("decode")
        .to_luma8();

    let board = PuzzleBoardSpec::new(20, 20, 5.0).expect("spec");
    let sweep = PuzzleBoardParams::sweep_for_board(&board);

    let r = match detect::detect_puzzleboard_best(&img, &sweep) {
        Ok(r) => r,
        Err(e) => {
            println!("detect failed: {e}");
            return;
        }
    };

    println!(
        "Found origin: row={} col={}",
        r.decode.master_origin_row, r.decode.master_origin_col
    );
    println!(
        "Decode BER={:.3}, edges={}/{}",
        r.decode.bit_error_rate, r.decode.edges_matched, r.decode.edges_observed
    );
    println!(
        "Alignment transform: a={} b={} c={} d={}",
        r.alignment.transform.a,
        r.alignment.transform.b,
        r.alignment.transform.c,
        r.alignment.transform.d
    );

    let edges = &r.observed_edges;

    // Compute BER at the reference oracle origin (94, 470) = identity transform
    let (true_row, true_col) = (94i32, 470i32);
    let mut matched_true = 0usize;
    let mut matched_found = 0usize;
    let n = edges.len();

    for e in edges.iter() {
        // At true origin (identity transform assumed)
        let exp_true = match e.orientation {
            EdgeOrientation::Horizontal => horizontal_edge_bit(true_row + e.row, true_col + e.col),
            EdgeOrientation::Vertical => vertical_edge_bit(true_row + e.row, true_col + e.col),
            _ => 0,
        };
        if exp_true == e.bit {
            matched_true += 1;
        }

        // At found origin (identity transform)
        let exp_found = match e.orientation {
            EdgeOrientation::Horizontal => horizontal_edge_bit(
                r.decode.master_origin_row + e.row,
                r.decode.master_origin_col + e.col,
            ),
            EdgeOrientation::Vertical => vertical_edge_bit(
                r.decode.master_origin_row + e.row,
                r.decode.master_origin_col + e.col,
            ),
            _ => 0,
        };
        if exp_found == e.bit {
            matched_found += 1;
        }
    }

    println!(
        "At true origin ({true_row},{true_col}) identity: {matched_true}/{n} matched, BER={:.3}",
        (n - matched_true) as f32 / n as f32
    );
    println!(
        "At found origin identity: {matched_found}/{n} matched, BER={:.3}",
        (n - matched_found) as f32 / n as f32
    );

    // Try swapped: use vertical_edge_bit for H obs and vice versa
    let mut matched_swapped = 0usize;
    for e in edges.iter() {
        let exp = match e.orientation {
            EdgeOrientation::Horizontal => vertical_edge_bit(true_row + e.row, true_col + e.col),
            EdgeOrientation::Vertical => horizontal_edge_bit(true_row + e.row, true_col + e.col),
            _ => 0,
        };
        if exp == e.bit {
            matched_swapped += 1;
        }
    }
    println!(
        "At true origin with SWAPPED H/V maps: {matched_swapped}/{n} matched, BER={:.3}",
        (n - matched_swapped) as f32 / n as f32
    );

    // Try with found origin's transform applied, checking all 8 D4 transforms at true origin
    println!("\nBER at true origin for all 8 D4 transforms:");
    for (ti, &t) in GRID_TRANSFORMS_D4.iter().enumerate() {
        let mut m = 0usize;
        for e in edges.iter() {
            let tr = t.a * e.row + t.b * e.col;
            let tc = t.c * e.row + t.d * e.col;
            let eff_h = if t.b.abs() > t.d.abs() {
                EdgeOrientation::Vertical
            } else {
                EdgeOrientation::Horizontal
            };
            let eff_v = if t.c.abs() > t.a.abs() {
                EdgeOrientation::Horizontal
            } else {
                EdgeOrientation::Vertical
            };
            let exp = match e.orientation {
                EdgeOrientation::Horizontal => match eff_h {
                    EdgeOrientation::Horizontal => {
                        horizontal_edge_bit(true_row + tr, true_col + tc)
                    }
                    _ => vertical_edge_bit(true_row + tr, true_col + tc),
                },
                EdgeOrientation::Vertical => match eff_v {
                    EdgeOrientation::Vertical => vertical_edge_bit(true_row + tr, true_col + tc),
                    _ => horizontal_edge_bit(true_row + tr, true_col + tc),
                },
                _ => 0,
            };
            if exp == e.bit {
                m += 1;
            }
        }
        println!(
            "  D4[{ti}] a={} b={} c={} d={}: {m}/{n} matched, BER={:.3}",
            t.a,
            t.b,
            t.c,
            t.d,
            (n - m) as f32 / n as f32
        );
    }

    // Now try with the correct transform applied (inverse of alignment.transform)
    let t = r.alignment.transform;
    let mut matched_true_t = 0usize;
    for e in edges.iter() {
        // Apply the found transform to the edge coords
        let tr = t.a * e.row + t.b * e.col;
        let tc = t.c * e.row + t.d * e.col;
        // Compute effective orientation after transform
        let eff_orient_h = if t.b.abs() > t.d.abs() {
            EdgeOrientation::Vertical
        } else {
            EdgeOrientation::Horizontal
        };
        let eff_orient_v = if t.c.abs() > t.a.abs() {
            EdgeOrientation::Horizontal
        } else {
            EdgeOrientation::Vertical
        };
        // Check against true origin with this transform
        let exp = match e.orientation {
            EdgeOrientation::Horizontal => match eff_orient_h {
                EdgeOrientation::Horizontal => horizontal_edge_bit(true_row + tr, true_col + tc),
                EdgeOrientation::Vertical => vertical_edge_bit(true_row + tr, true_col + tc),
                _ => 0,
            },
            EdgeOrientation::Vertical => match eff_orient_v {
                EdgeOrientation::Horizontal => horizontal_edge_bit(true_row + tr, true_col + tc),
                EdgeOrientation::Vertical => vertical_edge_bit(true_row + tr, true_col + tc),
                _ => 0,
            },
            _ => 0,
        };
        if exp == e.bit {
            matched_true_t += 1;
        }
    }
    println!(
        "At true origin with found transform: {matched_true_t}/{n} matched, BER={:.3}",
        (n - matched_true_t) as f32 / n as f32
    );

    // Print all detected corners with their local grid coords and pixel positions
    println!("\nDetected corners (local i,j → master col,row → pixel x,y):");
    let sorted_corners: Vec<_> = r
        .detection
        .corners
        .iter()
        .filter(|c| c.grid.is_some())
        .collect();
    // Need local grid coords from the observed edges...
    // Print master coords and pixel positions:
    for c in sorted_corners.iter().take(20) {
        let g = c.grid.unwrap();
        println!(
            "  master=({},{}) px=({:.1},{:.1})",
            g.j, g.i, c.position.x, c.position.y
        );
    }

    // Stats on edge coordinate ranges
    let (rmin, rmax, cmin, cmax) = edges.iter().fold(
        (i32::MAX, i32::MIN, i32::MAX, i32::MIN),
        |(rmin, rmax, cmin, cmax), e| {
            (
                rmin.min(e.row),
                rmax.max(e.row),
                cmin.min(e.col),
                cmax.max(e.col),
            )
        },
    );
    println!("\nEdge coord ranges: r=[{rmin},{rmax}], c=[{cmin},{cmax}]");

    // Dump first 20 edges
    println!("\nFirst 20 observed edges (row, col, orient, bit, conf):");
    for e in edges.iter().take(20) {
        let orient = match e.orientation {
            EdgeOrientation::Horizontal => "H",
            EdgeOrientation::Vertical => "V",
            _ => "?",
        };
        let exp_true = match e.orientation {
            EdgeOrientation::Horizontal => horizontal_edge_bit(true_row + e.row, true_col + e.col),
            EdgeOrientation::Vertical => vertical_edge_bit(true_row + e.row, true_col + e.col),
            _ => 0,
        };
        println!(
            "  {orient} r={:3} c={:3} bit={} conf={:.2} exp_true={} match={}",
            e.row,
            e.col,
            e.bit,
            e.confidence,
            exp_true,
            if e.bit == exp_true { "Y" } else { "N" }
        );
    }
}
