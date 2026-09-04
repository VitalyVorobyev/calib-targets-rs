//! Every `testdata/printable/*.json` fixture must deserialise into a
//! `PrintableTargetDocument` and render.
//!
//! These fixtures are the document schema the CLI reads and, since
//! `render_target_bundle_json`, the shape the WASM/JS surface accepts. That
//! makes them a published contract rather than example data, so a field rename
//! on the Rust side has to fail here rather than silently breaking every host
//! application that builds one of these by hand.
//!
//! The page assertions matter specifically: the fixed-arity `render_*_bundle`
//! helpers wrap a target in a page sized to fit it, so nothing else proves that
//! an *authored* `page` block reaches the rendered output at all.

use calib_targets_print::{
    render_target_bundle, PageOrientation, PageSize, PrintableTargetDocument,
};
use std::path::{Path, PathBuf};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/printable")
}

fn fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(fixture_dir())
        .expect("testdata/printable is readable")
        .map(|e| e.expect("readable dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no fixtures found in testdata/printable");
    out
}

/// Page dimensions the document asks for, in millimetres, after orientation.
fn expected_page_mm(doc: &PrintableTargetDocument) -> (f64, f64) {
    let (w, h) = doc
        .page
        .size
        .base_dimensions_mm()
        .expect("fixture page size is valid");
    match doc.page.orientation {
        PageOrientation::Landscape => (h, w),
        _ => (w, h),
    }
}

#[test]
fn every_fixture_deserialises_and_renders() {
    for path in fixtures() {
        let name = path.file_name().expect("fixture has a name").to_owned();
        let text = std::fs::read_to_string(&path).expect("fixture is readable");
        let doc: PrintableTargetDocument = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{name:?} does not deserialise: {e}"));

        assert_eq!(doc.schema_version, 1, "{name:?}: unexpected schema_version");

        let bundle =
            render_target_bundle(&doc).unwrap_or_else(|e| panic!("{name:?} failed to render: {e}"));

        assert!(
            bundle.svg_text.contains("<rect "),
            "{name:?}: SVG has no rects"
        );
        assert!(
            !bundle.png_bytes.is_empty(),
            "{name:?}: PNG bytes are empty"
        );
        assert!(
            bundle.dxf_text.contains("SECTION"),
            "{name:?}: DXF is not a DXF"
        );
        assert!(
            bundle.json_text.contains("\"schema_version\""),
            "{name:?}: round-tripped JSON lost schema_version"
        );

        // The authored page block must reach the rendered SVG. Without this,
        // a document whose `page` was silently ignored would still pass every
        // other assertion here.
        let (want_w, want_h) = expected_page_mm(&doc);
        let (got_w, got_h) = svg_page_mm(&bundle.svg_text);
        assert!(
            (got_w - want_w).abs() < 1e-6 && (got_h - want_h).abs() < 1e-6,
            "{name:?}: SVG page is {got_w}x{got_h} mm, document asks for {want_w}x{want_h} mm"
        );
    }
}

/// Millimetre page dimensions declared in the SVG root element.
///
/// Compared numerically rather than as formatted text: the renderer's float
/// formatting is its own business, and mirroring it here would make this test
/// fail on a cosmetic change (`8.5 in` is not exactly `215.9 mm` in f64).
fn svg_page_mm(svg: &str) -> (f64, f64) {
    fn attr(svg: &str, name: &str) -> f64 {
        let key = format!("{name}=\"");
        let start = svg.find(&key).expect("SVG root carries the attribute") + key.len();
        let rest = &svg[start..];
        let end = rest.find("mm\"").expect("attribute is in millimetres");
        rest[..end].parse().expect("attribute is a number")
    }
    (attr(svg, "width"), attr(svg, "height"))
}

#[test]
fn an_authored_page_overrides_the_default() {
    // A4 portrait is the default, so prove the page block is read by asking for
    // something the default would never produce.
    let text = std::fs::read_to_string(fixture_dir().join("chessboard_a4.json"))
        .expect("chessboard fixture is readable");
    let mut doc: PrintableTargetDocument =
        serde_json::from_str(&text).expect("chessboard fixture deserialises");

    doc.page.size = PageSize::Letter;
    doc.page.orientation = PageOrientation::Landscape;

    let bundle = render_target_bundle(&doc).expect("renders on Letter landscape");
    let (want_w, want_h) = expected_page_mm(&doc);
    let (got_w, got_h) = svg_page_mm(&bundle.svg_text);
    assert!(
        (got_w - want_w).abs() < 1e-6 && (got_h - want_h).abs() < 1e-6,
        "Letter landscape did not reach the SVG: got {got_w}x{got_h} mm, want {want_w}x{want_h} mm"
    );
    // ... and it is genuinely landscape, not the A4 portrait default.
    assert!(
        got_w > got_h,
        "expected a landscape page, got {got_w}x{got_h} mm"
    );
}
