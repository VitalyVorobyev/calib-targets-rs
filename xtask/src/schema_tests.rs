//! The generated schemas must accept exactly the documents the library itself
//! writes and reads.
//!
//! These tests validate real library output (the values behind every wasm
//! `default_*` / `*_sweep_*` function, presets, constructor results and the
//! checked-in printable documents) against the schemas, and hand-built invalid
//! documents against the same schemas, so a serde/schema mismatch shows up as
//! a failing test rather than a form that silently rejects valid configs.

use crate::emit_schemas::{render, schemas, strip_intra_doc_links, tidy, SCHEMA_DIR};
use calib_targets_aruco::builtins::{builtin_dictionary, BUILTIN_DICTIONARY_NAMES};
use calib_targets_aruco::Dictionary;
use calib_targets_charuco::{
    CharucoAdvancedTuning, CharucoBoard, CharucoBoardSpec, CharucoDetector, CharucoParams,
    MarkerLayout,
};
use calib_targets_chessboard::{ChessboardAdvancedTuning, ChessboardParams};
use calib_targets_core::{default_chess_config, DetectorConfig};
use calib_targets_marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
use calib_targets_print::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, PageOrientation, PageSize,
    PageSpec, PrintableTargetDocument, PuzzleBoardTargetSpec, PuzzlePoleTargetSpec, RenderOptions,
    TargetSpec,
};
use calib_targets_puzzleboard::{
    PuzzleBoardAdvancedTuning, PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
    PuzzleBoardSpec, PuzzleBoardSymmetryMode,
};
use jsonschema::Validator;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

fn schema(name: &str) -> Value {
    schemas()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no schema named {name}"))
        .schema
}

fn validator(name: &str) -> Validator {
    jsonschema::validator_for(&schema(name))
        .unwrap_or_else(|e| panic!("{name} must be a valid schema: {e}"))
}

fn errors(v: &Validator, instance: &Value) -> Vec<String> {
    v.iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path))
        .collect()
}

/// Serialize `value` and require the schema `name` to accept it.
fn assert_valid<T: Serialize>(name: &str, what: &str, value: &T) {
    let instance = serde_json::to_value(value).expect("serialize");
    let errs = errors(&validator(name), &instance);
    assert!(
        errs.is_empty(),
        "{what} must validate against {name}: {errs:#?}\n{instance}"
    );
}

fn assert_rejected(name: &str, what: &str, instance: &Value) {
    assert!(
        !validator(name).is_valid(instance),
        "{what} must be rejected by {name}: {instance}"
    );
}

// ── Helpers: the boards the wasm defaults are built from ─────────────────────

const DICTS: [&str; 3] = ["DICT_4X4_50", "DICT_5X5_250", "DICT_6X6_1000"];

fn charuco_board(rows: u32, cols: u32, rel: f32, dict: &str) -> CharucoBoardSpec {
    CharucoBoardSpec::new(
        rows,
        cols,
        1.0,
        rel,
        builtin_dictionary(dict).expect("dict"),
    )
}

fn puzzle_board(rows: u32, cols: u32) -> PuzzleBoardSpec {
    PuzzleBoardSpec::new(rows, cols, 1.0).expect("spec")
}

// ── Schemas are well-formed and shipped ──────────────────────────────────────

#[test]
fn every_schema_compiles_and_is_object_rooted() {
    for file in schemas() {
        jsonschema::validator_for(&file.schema)
            .unwrap_or_else(|e| panic!("{} does not compile: {e}", file.name));
        assert_eq!(file.schema["type"], "object", "{}", file.name);
        assert!(file.schema["properties"].is_object(), "{}", file.name);
    }
}

#[test]
fn schema_file_names_are_unique_and_json() {
    let names: Vec<&str> = schemas().iter().map(|f| f.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert!(names.iter().all(|n| n.ends_with(".json")));
}

// ── DetectorConfig (chess_config.json) ───────────────────────────────────────

#[test]
fn chess_config_accepts_workspace_default_and_presets() {
    assert_valid(
        "chess_config.json",
        "default_chess_config()",
        &default_chess_config(),
    );
    for (name, cfg) in [
        ("DetectorConfig::default", DetectorConfig::default()),
        ("chess", DetectorConfig::chess()),
        ("chess_multiscale", DetectorConfig::chess_multiscale()),
        ("radon", DetectorConfig::radon()),
        ("radon_multiscale", DetectorConfig::radon_multiscale()),
    ] {
        assert_valid("chess_config.json", name, &cfg);
    }
}

#[test]
fn chess_config_rejects_out_of_range() {
    for bad in [
        json!({"threshold": -1.0}),
        json!({"upscale": {"fixed": 7}}),
        json!({"strategy": {"sobel": {}}}),
    ] {
        assert_rejected("chess_config.json", "bad chess config", &bad);
    }
}

// ── ChessboardParams ─────────────────────────────────────────────────────────

#[test]
fn chessboard_defaults_and_sweep_validate() {
    // wasm `default_chessboard_params` / `chessboard_sweep_default`.
    assert_valid(
        "chessboard_params.json",
        "default",
        &ChessboardParams::default(),
    );
    let sweep = ChessboardParams::sweep_default();
    assert_eq!(sweep.len(), 3);
    for (i, params) in sweep.iter().enumerate() {
        assert_valid("chessboard_params.json", &format!("sweep[{i}]"), params);
    }
    let with_advanced =
        ChessboardParams::default().with_advanced(ChessboardAdvancedTuning::default());
    assert_valid(
        "chessboard_params.json",
        "default + advanced",
        &with_advanced,
    );
}

#[test]
fn chessboard_schema_matches_deny_unknown_fields() {
    let good = json!({"min_labeled_corners": 8, "max_components": 3, "min_corner_strength": 33.0});
    assert!(validator("chessboard_params.json").is_valid(&good));
    assert!(serde_json::from_value::<ChessboardParams>(good.clone()).is_ok());

    // `deny_unknown_fields`: both sides reject a stray key.
    let mut stray = good.clone();
    stray["graph_build_algorithm"] = json!("topological");
    assert_rejected("chessboard_params.json", "unknown key", &stray);
    assert!(serde_json::from_value::<ChessboardParams>(stray).is_err());

    // The stable core has no serde defaults: a missing key is rejected by both.
    let missing = json!({"max_components": 3, "min_corner_strength": 33.0});
    assert_rejected(
        "chessboard_params.json",
        "missing min_labeled_corners",
        &missing,
    );
    assert!(serde_json::from_value::<ChessboardParams>(missing).is_err());

    let mut unknown_advanced = good;
    unknown_advanced["advanced"] = json!({"not_a_knob": 1});
    assert_rejected(
        "chessboard_params.json",
        "unknown advanced key",
        &unknown_advanced,
    );
}

#[test]
fn chessboard_schema_rejects_out_of_range() {
    let base = || {
        serde_json::to_value(
            ChessboardParams::default().with_advanced(ChessboardAdvancedTuning::default()),
        )
        .unwrap()
    };
    let mutate = |path: &[&str], value: Value| {
        let mut doc = base();
        let mut cur = &mut doc;
        for key in &path[..path.len() - 1] {
            cur = &mut cur[*key];
        }
        cur[path[path.len() - 1]] = value;
        doc
    };
    assert!(validator("chessboard_params.json").is_valid(&base()));
    for (what, doc) in [
        (
            "negative corner strength",
            mutate(&["min_corner_strength"], json!(-1.0)),
        ),
        (
            "negative min_labeled_corners",
            mutate(&["min_labeled_corners"], json!(-2)),
        ),
        (
            "num_bins below 4",
            mutate(&["advanced", "num_bins"], json!(3)),
        ),
        (
            "negative cluster_tol_deg",
            mutate(&["advanced", "cluster_tol_deg"], json!(-1.0)),
        ),
        (
            "peak weight fraction above 1",
            mutate(&["advanced", "min_peak_weight_fraction"], json!(1.5)),
        ),
        (
            "line_min_members below 2",
            mutate(&["advanced", "line_min_members"], json!(1)),
        ),
        (
            "non-positive axis_align_tol_rad",
            mutate(
                &["advanced", "topological", "axis_align_tol_rad"],
                json!(0.0),
            ),
        ),
        (
            "opposing_edge_ratio_max below 1",
            mutate(
                &["advanced", "topological", "opposing_edge_ratio_max"],
                json!(0.5),
            ),
        ),
        (
            "min_corners_for_component below 4",
            mutate(
                &["advanced", "topological", "min_corners_for_component"],
                json!(3),
            ),
        ),
        (
            "negative merge tolerance",
            mutate(
                &["advanced", "component_merge", "position_tol_rel"],
                json!(-0.1),
            ),
        ),
    ] {
        assert_rejected("chessboard_params.json", what, &doc);
    }
}

// ── Dictionary ───────────────────────────────────────────────────────────────

#[test]
fn dictionary_schema_lists_exactly_the_builtin_names() {
    let schema = schema("charuco_params.json");
    let dict = &schema["$defs"]["Dictionary"];
    assert_eq!(dict["type"], "string");
    let listed: Vec<&str> = dict["enum"]
        .as_array()
        .expect("enum list")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert_eq!(listed, BUILTIN_DICTIONARY_NAMES);
    assert!(
        listed.len() >= 20,
        "expected the full builtin table, got {}",
        listed.len()
    );
}

#[test]
fn every_listed_dictionary_name_round_trips() {
    for name in BUILTIN_DICTIONARY_NAMES {
        let dict: Dictionary = serde_json::from_value(json!(name))
            .unwrap_or_else(|e| panic!("`{name}` is listed but does not deserialize: {e}"));
        assert_eq!(dict.name(), *name);
        assert_eq!(serde_json::to_value(dict).unwrap(), json!(name));
        // ...and a board using it validates in every place a dictionary appears.
        let board = CharucoBoardSpec::new(5, 7, 1.0, 0.75, dict);
        assert_valid(
            "charuco_params.json",
            name,
            &CharucoParams::for_board(board),
        );
        assert_valid(
            "printable_target_document.json",
            name,
            &PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec::new(
                5, 7, 10.0, 0.75, dict,
            ))),
        );
    }
}

#[test]
fn unknown_dictionary_names_are_rejected_by_schema_and_serde() {
    for bad in ["DICT_NOPE", "dict_4x4_50", "4X4_50", ""] {
        assert!(
            serde_json::from_value::<Dictionary>(json!(bad)).is_err(),
            "{bad}"
        );
        let mut doc = serde_json::to_value(CharucoParams::for_board(charuco_board(
            5,
            7,
            0.75,
            "DICT_4X4_50",
        )))
        .unwrap();
        doc["board"]["dictionary"] = json!(bad);
        assert_rejected("charuco_params.json", bad, &doc);
    }
}

// ── CharucoParams ────────────────────────────────────────────────────────────

#[test]
fn charuco_defaults_and_sweeps_validate() {
    // wasm `default_charuco_params` / `charuco_sweep_for_board`.
    for dict in DICTS {
        for (rows, cols, rel) in [(5, 7, 0.75), (8, 11, 0.5), (3, 3, 1.0)] {
            let board = charuco_board(rows, cols, rel, dict);
            let what = format!("{rows}x{cols} {dict}");
            assert_valid(
                "charuco_params.json",
                &what,
                &CharucoParams::for_board(board),
            );
            for (i, params) in CharucoParams::sweep_for_board(&board).iter().enumerate() {
                assert_valid("charuco_params.json", &format!("{what} sweep[{i}]"), params);
            }
        }
    }
    let board = charuco_board(5, 7, 0.75, "DICT_4X4_50")
        .with_marker_layout(MarkerLayout::OpenCvCharuco)
        .with_border_bits(2);
    let params = CharucoParams::for_board(board).with_advanced(CharucoAdvancedTuning::new());
    assert_valid("charuco_params.json", "advanced + border_bits", &params);
}

#[test]
fn charuco_scan_border_bits_is_derived_from_the_board() {
    // The schema marks `scan.border_bits` readOnly because the detector
    // discards it; prove that, so the annotation cannot go stale.
    let field =
        &schema("charuco_params.json")["$defs"]["ScanDecodeConfig"]["properties"]["border_bits"];
    assert_eq!(field["readOnly"], json!(true));
    let board = charuco_board(5, 7, 0.75, "DICT_4X4_50").with_border_bits(2);
    let mut params = CharucoParams::for_board(board);
    params.scan.border_bits = 5;
    let detector = CharucoDetector::new(params).expect("valid board");
    assert_eq!(detector.params().scan.border_bits, 2);
    // No other scan field is read-only.
    let scan = &schema("charuco_params.json")["$defs"]["ScanDecodeConfig"]["properties"];
    for (name, prop) in scan.as_object().unwrap() {
        assert_eq!(
            prop.get("readOnly").is_some(),
            name == "border_bits",
            "{name}"
        );
    }
}

#[test]
fn charuco_schema_rejects_invalid_boards() {
    let base = serde_json::to_value(CharucoParams::for_board(charuco_board(
        5,
        7,
        0.75,
        "DICT_4X4_50",
    )))
    .unwrap();
    assert!(validator("charuco_params.json").is_valid(&base));
    let set = |field: &str, value: Value| {
        let mut doc = base.clone();
        doc["board"][field] = value;
        doc
    };
    for (what, doc) in [
        ("rows below 2", set("rows", json!(1))),
        ("zero cell_size", set("cell_size", json!(0.0))),
        ("marker_size_rel 0", set("marker_size_rel", json!(0.0))),
        (
            "marker_size_rel above 1",
            set("marker_size_rel", json!(1.5)),
        ),
        ("border_bits 0", set("border_bits", json!(0))),
        (
            "unknown marker layout",
            set("marker_layout", json!("legacy")),
        ),
    ] {
        assert_rejected("charuco_params.json", what, &doc);
    }
    // The library rejects the same boards (the bounds are enforced, not invented).
    for (rows, cell, rel, border) in [
        (1, 1.0, 0.75, 1),
        (5, 0.0, 0.75, 1),
        (5, 1.0, 1.5, 1),
        (5, 1.0, 0.75, 0),
    ] {
        let spec = charuco_board(rows, 7, rel, "DICT_4X4_50");
        let spec = CharucoBoardSpec::new(
            spec.rows,
            spec.cols,
            cell,
            spec.marker_size_rel,
            spec.dictionary,
        )
        .with_border_bits(border);
        assert!(
            CharucoBoard::new(spec).is_err(),
            "{rows} {cell} {rel} {border}"
        );
    }

    let mut stray = base.clone();
    stray["not_a_field"] = json!(1);
    assert_rejected("charuco_params.json", "unknown key", &stray);

    let mut no_board = base;
    no_board.as_object_mut().unwrap().remove("board");
    assert_rejected("charuco_params.json", "missing board", &no_board);
}

// ── MarkerBoardParams ────────────────────────────────────────────────────────

#[test]
fn marker_board_defaults_and_sweeps_validate() {
    // wasm `default_marker_board_params` / `marker_board_sweep_for_board`.
    assert_valid(
        "marker_board_params.json",
        "default",
        &MarkerBoardParams::default(),
    );
    assert_valid(
        "marker_board_spec.json",
        "default spec",
        &MarkerBoardSpec::default(),
    );

    let custom = MarkerBoardSpec::new(
        9,
        12,
        [
            MarkerCircleSpec::new(CellCoords { i: 4, j: 4 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 5, j: 4 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 5, j: 5 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(12.5)
    .with_circle_diameter_rel(0.4);
    assert_valid("marker_board_spec.json", "custom spec", &custom);
    for (i, params) in MarkerBoardParams::sweep_for_board(&custom)
        .iter()
        .enumerate()
    {
        assert_valid("marker_board_params.json", &format!("sweep[{i}]"), params);
    }
    let mut with_roi = MarkerBoardParams::for_board(custom);
    with_roi.roi_cells = Some([0, 0, 8, 8]);
    assert_valid("marker_board_params.json", "roi_cells", &with_roi);
}

#[test]
fn marker_board_schema_rejects_invalid_documents() {
    let base = serde_json::to_value(MarkerBoardParams::default()).unwrap();
    assert!(validator("marker_board_params.json").is_valid(&base));
    let with = |f: &dyn Fn(&mut Value)| {
        let mut doc = base.clone();
        f(&mut doc);
        doc
    };
    for (what, doc) in [
        (
            "two circles",
            with(&|d| {
                d["board"]["circles"].as_array_mut().unwrap().pop();
            }),
        ),
        (
            "bad polarity",
            with(&|d| d["board"]["circles"][0]["polarity"] = json!("grey")),
        ),
        (
            "roi with 3 entries",
            with(&|d| d["roi_cells"] = json!([0, 0, 1])),
        ),
        (
            "zero circle diameter",
            with(&|d| d["board"]["circle_diameter_rel"] = json!(0.0)),
        ),
        (
            "zero samples",
            with(&|d| d["circle_score"]["samples"] = json!(0)),
        ),
        (
            "contrast above 255",
            with(&|d| d["circle_score"]["min_contrast"] = json!(300.0)),
        ),
        ("unknown key", with(&|d| d["bogus"] = json!(true))),
        (
            "unknown circle_score key",
            with(&|d| d["circle_score"]["bogus"] = json!(true)),
        ),
    ] {
        assert_rejected("marker_board_params.json", what, &doc);
    }
}

// ── PuzzleBoardParams ────────────────────────────────────────────────────────

#[test]
fn puzzleboard_defaults_sweeps_and_every_mode_validate() {
    // wasm `default_puzzleboard_params` / `puzzleboard_sweep_for_board`.
    for (rows, cols) in [(10, 10), (4, 4), (12, 20), (501, 501)] {
        let spec = puzzle_board(rows, cols);
        assert_valid(
            "puzzleboard_params.json",
            &format!("{rows}x{cols}"),
            &PuzzleBoardParams::for_board(spec),
        );
        let sweep = PuzzleBoardParams::sweep_for_board(&spec);
        assert_eq!(sweep.len(), 6);
        for (i, params) in sweep.iter().enumerate() {
            assert_valid(
                "puzzleboard_params.json",
                &format!("{rows}x{cols} sweep[{i}]"),
                params,
            );
        }
    }
    let master = PuzzleBoardSpec::master(2.0).unwrap();
    let origin = PuzzleBoardSpec::with_origin(20, 30, 1.5, 100, 200).unwrap();
    for board in [master, origin] {
        for search in [
            PuzzleBoardSearchMode::Full,
            PuzzleBoardSearchMode::FixedBoard,
        ] {
            for scoring in [
                PuzzleBoardScoringMode::HardWeighted,
                PuzzleBoardScoringMode::SoftLogLikelihood,
            ] {
                for symmetry in [
                    PuzzleBoardSymmetryMode::Rotations,
                    PuzzleBoardSymmetryMode::RotationsAndReflections,
                ] {
                    let mut params = PuzzleBoardParams::for_board(board);
                    params.decode.search_mode = search;
                    params.decode.scoring_mode = scoring;
                    params.decode.symmetry_mode = symmetry;
                    params.decode = params
                        .decode
                        .with_advanced(PuzzleBoardAdvancedTuning::new());
                    assert_valid(
                        "puzzleboard_params.json",
                        &format!("{search:?}/{scoring:?}/{symmetry:?}"),
                        &params,
                    );
                }
            }
        }
    }
}

#[test]
fn puzzleboard_schema_rejects_invalid_documents() {
    let base = serde_json::to_value(PuzzleBoardParams::for_board(puzzle_board(10, 10))).unwrap();
    assert!(validator("puzzleboard_params.json").is_valid(&base));
    let with = |f: &dyn Fn(&mut Value)| {
        let mut doc = base.clone();
        f(&mut doc);
        doc
    };
    for (what, doc) in [
        ("rows below 4", with(&|d| d["board"]["rows"] = json!(3))),
        (
            "rows above master",
            with(&|d| d["board"]["rows"] = json!(502)),
        ),
        (
            "zero cell_size",
            with(&|d| d["board"]["cell_size"] = json!(0.0)),
        ),
        (
            "zero px_per_square",
            with(&|d| d["px_per_square"] = json!(0.0)),
        ),
        (
            "BER above 1",
            with(&|d| d["decode"]["max_bit_error_rate"] = json!(1.5)),
        ),
        (
            "unknown search mode",
            with(&|d| d["decode"]["search_mode"] = json!({"kind": "bogus"})),
        ),
        (
            "untagged search mode",
            with(&|d| d["decode"]["search_mode"] = json!("full")),
        ),
        (
            "unknown decode key",
            with(&|d| d["decode"]["bogus"] = json!(1)),
        ),
        ("unknown key", with(&|d| d["bogus"] = json!(1))),
    ] {
        assert_rejected("puzzleboard_params.json", what, &doc);
    }
    // The library rejects the same specs.
    assert!(PuzzleBoardSpec::new(3, 10, 1.0).is_err());
    assert!(PuzzleBoardSpec::new(502, 10, 1.0).is_err());
    assert!(PuzzleBoardSpec::new(10, 10, 0.0).is_err());
}

// ── PrintableTargetDocument ──────────────────────────────────────────────────

fn printable_examples() -> Vec<(&'static str, PrintableTargetDocument)> {
    let dict = builtin_dictionary("DICT_4X4_250").unwrap();
    let circles = MarkerBoardTargetSpec::default_circles(6, 8);
    let custom_page = PageSpec::default()
        .with_size(PageSize::Custom {
            width_mm: 300.0,
            height_mm: 200.0,
        })
        .with_orientation(PageOrientation::Landscape)
        .with_margin_mm(5.0);
    vec![
        (
            "chessboard",
            PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec::new(
                6, 8, 20.0,
            ))),
        ),
        (
            "chessboard inset",
            PrintableTargetDocument::new(TargetSpec::Chessboard(
                ChessboardTargetSpec::new(6, 8, 20.0).with_inner_square_rel(0.5),
            )),
        ),
        (
            "charuco",
            PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec::new(
                8, 11, 15.0, 0.75, dict,
            ))),
        ),
        (
            "charuco border_bits",
            PrintableTargetDocument::new(TargetSpec::Charuco(
                CharucoTargetSpec::new(5, 7, 15.0, 0.75, dict)
                    .with_border_bits(2)
                    .with_inner_square_rel(0.3),
            )),
        ),
        (
            "marker_board",
            PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec::new(
                6, 8, 20.0, circles,
            ))),
        ),
        (
            "puzzleboard",
            PrintableTargetDocument::new(TargetSpec::PuzzleBoard(
                PuzzleBoardTargetSpec::new(10, 12, 12.0)
                    .with_origin(5, 7)
                    .with_dot_diameter_rel(0.4),
            )),
        ),
        (
            "puzzlepole",
            PrintableTargetDocument::new(TargetSpec::PuzzlePole(
                PuzzlePoleTargetSpec::new(12, 8, 10.0)
                    .expect("supported period")
                    .with_axial_start_col(3),
            )),
        ),
        (
            "custom page + render",
            PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec::new(
                4, 4, 10.0,
            )))
            .with_page(custom_page)
            .with_render(
                RenderOptions::default()
                    .with_debug_annotations(true)
                    .with_png_dpi(600),
            ),
        ),
        (
            "letter landscape",
            PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec::new(
                4, 4, 10.0,
            )))
            .with_page(
                PageSpec::default()
                    .with_size(PageSize::Letter)
                    .with_orientation(PageOrientation::Landscape),
            ),
        ),
    ]
}

#[test]
fn printable_examples_for_every_target_kind_validate() {
    let examples = printable_examples();
    for (what, doc) in &examples {
        doc.validate()
            .unwrap_or_else(|e| panic!("{what} is not a valid document: {e}"));
        assert_valid("printable_target_document.json", what, doc);
    }
    // Every target kind the library knows is covered (a new variant fails here).
    let mut kinds: Vec<&str> = examples.iter().map(|(_, d)| d.target.kind_name()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        [
            "charuco",
            "chessboard",
            "marker_board",
            "puzzleboard",
            "puzzlepole"
        ]
    );
}

#[test]
fn checked_in_printable_documents_validate() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/printable");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("testdata/printable exists") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let instance: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let errs = errors(&validator("printable_target_document.json"), &instance);
        assert!(
            errs.is_empty(),
            "{} must validate: {errs:#?}",
            path.display()
        );
        seen += 1;
    }
    assert!(seen >= 9, "expected the printable fixtures, found {seen}");
}

#[test]
fn printable_schema_rejects_invalid_documents() {
    let base = serde_json::to_value(&printable_examples()[2].1).unwrap(); // charuco
    assert!(validator("printable_target_document.json").is_valid(&base));
    let with = |f: &dyn Fn(&mut Value)| {
        let mut doc = base.clone();
        f(&mut doc);
        doc
    };
    let cases: Vec<(&str, Value)> = vec![
        (
            "unknown target kind",
            with(&|d| d["target"]["kind"] = json!("hexboard")),
        ),
        (
            "missing target kind",
            with(&|d| {
                d["target"].as_object_mut().unwrap().remove("kind");
            }),
        ),
        (
            "schema_version 2",
            with(&|d| d["schema_version"] = json!(2)),
        ),
        ("zero png_dpi", with(&|d| d["render"]["png_dpi"] = json!(0))),
        (
            "negative margin",
            with(&|d| d["page"]["margin_mm"] = json!(-1.0)),
        ),
        (
            "zero square size",
            with(&|d| d["target"]["square_size_mm"] = json!(0.0)),
        ),
        (
            "marker_size_rel above 1",
            with(&|d| d["target"]["marker_size_rel"] = json!(1.2)),
        ),
        (
            "zero border_bits",
            with(&|d| d["target"]["border_bits"] = json!(0)),
        ),
        (
            "unknown dictionary",
            with(&|d| d["target"]["dictionary"] = json!("DICT_NOPE")),
        ),
        (
            "unknown page kind",
            with(&|d| d["page"]["size"] = json!({"kind": "a0"})),
        ),
        (
            "custom page without size",
            with(&|d| d["page"]["size"] = json!({"kind": "custom"})),
        ),
        (
            "custom page zero width",
            with(&|d| {
                d["page"]["size"] = json!({"kind": "custom", "width_mm": 0.0, "height_mm": 10.0})
            }),
        ),
        (
            "bad orientation",
            with(&|d| d["page"]["orientation"] = json!("diagonal")),
        ),
        (
            "inner_square_rel 1.0",
            with(&|d| d["target"]["inner_square_rel"] = json!(1.0)),
        ),
    ];
    for (what, doc) in &cases {
        assert_rejected("printable_target_document.json", what, doc);
    }

    // The library rejects the out-of-range documents too: these bounds are
    // enforced by `PrintableTargetDocument::validate`, not invented here.
    let library_checked = [
        "schema_version 2",
        "zero png_dpi",
        "negative margin",
        "zero square size",
        "marker_size_rel above 1",
        "zero border_bits",
        "inner_square_rel 1.0",
    ];
    for (what, doc) in cases.iter().filter(|(w, _)| library_checked.contains(w)) {
        let rejected = serde_json::from_value::<PrintableTargetDocument>(doc.clone())
            .map(|d| d.validate().is_err())
            .unwrap_or(true);
        assert!(rejected, "library must reject {what}");
    }
}

#[test]
fn printable_puzzleboard_and_pole_bounds_match_the_library() {
    let pb = |f: &dyn Fn(&mut Value)| {
        let mut doc = serde_json::to_value(&printable_examples()[5].1).unwrap();
        f(&mut doc);
        doc
    };
    for (what, doc) in [
        ("rows below 4", pb(&|d| d["target"]["rows"] = json!(3))),
        (
            "rows above master",
            pb(&|d| d["target"]["rows"] = json!(502)),
        ),
        (
            "zero dot diameter",
            pb(&|d| d["target"]["dot_diameter_rel"] = json!(0.0)),
        ),
    ] {
        assert_rejected("printable_target_document.json", what, &doc);
        let rejected = serde_json::from_value::<PrintableTargetDocument>(doc)
            .map(|d| d.validate().is_err())
            .unwrap_or(true);
        assert!(rejected, "library must reject {what}");
    }
    let pole = |f: &dyn Fn(&mut Value)| {
        let mut doc = serde_json::to_value(&printable_examples()[6].1).unwrap();
        f(&mut doc);
        doc
    };
    for (what, doc) in [
        (
            "axial_squares below 4",
            pole(&|d| d["target"]["axial_squares"] = json!(3)),
        ),
        (
            "zero square size",
            pole(&|d| d["target"]["square_size_mm"] = json!(0.0)),
        ),
    ] {
        assert_rejected("printable_target_document.json", what, &doc);
    }
}

// ── Emitter plumbing ─────────────────────────────────────────────────────────

#[test]
fn strips_intra_doc_links() {
    assert_eq!(
        strip_intra_doc_links("see [`Foo`](Self::foo) and [`Bar`] or [x] [`unclosed"),
        "see `Foo` and `Bar` or [x] [`unclosed"
    );
    assert_eq!(strip_intra_doc_links("plain"), "plain");
}

#[test]
fn f32_numbers_are_shortened() {
    let mut v = json!({"a": 0.001_f32 as f64, "b": 0.5, "c": 3});
    tidy(&mut v);
    assert_eq!(v, json!({"a": 0.001, "b": 0.5, "c": 3}));
}

#[test]
fn rendering_is_deterministic() {
    for file in schemas() {
        assert_eq!(
            render(&file.schema).unwrap(),
            render(&schema(file.name)).unwrap()
        );
    }
}

/// `true` when cargo's feature unification turned on `chess-corners/ml-refiner`
/// (e.g. `cargo test --workspace --features calib-targets/ml-refiner`), which
/// adds the `Ml` refiner variant to the derived schema. The shipped schemas are
/// always generated by `cargo xtask emit-schemas`, where that feature is off.
fn ml_refiner_compiled_in() -> bool {
    serde_json::from_value::<DetectorConfig>(json!({"strategy": {"chess": {"refiner": "ml"}}}))
        .is_ok()
}

#[test]
fn committed_schemas_are_current() {
    if ml_refiner_compiled_in() {
        eprintln!("skipped: ml-refiner is unified into this build; CI checks via `cargo xtask emit-schemas --check`");
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(SCHEMA_DIR);
    for file in schemas() {
        let on_disk = std::fs::read_to_string(dir.join(file.name)).unwrap_or_else(|_| {
            panic!(
                "{SCHEMA_DIR}/{} exists; run `cargo xtask emit-schemas`",
                file.name
            )
        });
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            render(&file.schema).unwrap(),
            "schema drift in {}; run `cargo xtask emit-schemas`",
            file.name
        );
    }
}
