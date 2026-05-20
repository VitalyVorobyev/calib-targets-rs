//! Golden DXF snapshot for the chrome-on-glass photolithography handoff.
//!
//! Re-renders a canonical 3×3 ChArUco board (DICT_4X4_50, `marker_size_rel
//! = 0.7`, default A4 page) and compares the produced DXF byte-for-byte
//! against the checked-in reference at `tests/golden/charuco_3x3_dict4x4_50.dxf`.
//!
//! The reference was bootstrapped by running this test with the
//! environment variable `UPDATE_GOLDEN=1`, which writes the rendered
//! DXF to the golden path instead of asserting equality. Use that
//! same path to regenerate the golden whenever the writer's output
//! is intentionally changed — be sure to inspect the diff afterwards
//! to confirm the change matches what the producer needs.

use std::fs;

use calib_targets_aruco::builtins::builtin_dictionary;
use calib_targets_charuco::MarkerLayout;
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, PrintableTargetDocument, TargetSpec,
};

const GOLDEN_PATH: &str = "tests/golden/charuco_3x3_dict4x4_50.dxf";

fn canonical_doc() -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
        rows: 3,
        cols: 3,
        square_size_mm: 12.0,
        marker_size_rel: 0.7,
        dictionary: builtin_dictionary("DICT_4X4_50").expect("DICT_4X4_50"),
        marker_layout: MarkerLayout::OpenCvCharuco,
        border_bits: 1,
    }))
}

#[test]
fn charuco_3x3_dxf_matches_golden() {
    let bundle = render_target_bundle(&canonical_doc()).expect("bundle");
    let actual = bundle.dxf_text;

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(GOLDEN_PATH, &actual).expect("write golden");
        eprintln!("UPDATE_GOLDEN=1 → wrote {GOLDEN_PATH}");
        return;
    }

    let expected = fs::read_to_string(GOLDEN_PATH).unwrap_or_else(|err| {
        panic!(
            "golden file missing or unreadable at {GOLDEN_PATH}: {err}. \
             Bootstrap with `UPDATE_GOLDEN=1 cargo test -p calib-targets-print \
             --test golden_dxf` and review the diff before committing."
        )
    });
    assert_eq!(
        actual, expected,
        "DXF output drifted from the golden snapshot. \
         Re-run with `UPDATE_GOLDEN=1 cargo test -p calib-targets-print \
         --test golden_dxf` to refresh, then inspect the diff before \
         committing."
    );
}
