//! Regenerate the book's generated figures.
//!
//! Every figure this emits is drawn from the *shipped* code maps through the
//! *real* renderer, so a figure cannot drift from the thing it illustrates. A
//! hand-drawn diagram of a code pattern is a claim; this is the pattern.
//!
//! Run it through `scripts/regen-book-figures.sh`, which is what documents the
//! public-data-only rule.
//!
//! ```bash
//! cargo run -p calib-targets --example book_figures -- book/src/img
//! ```

use calib_targets::printable::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzlePoleTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::pole::geometry;
use std::f32::consts::TAU;
use std::path::Path;

/// Ink and paper for the annotation layer.
///
/// Deliberately a small palette, and deliberately not the pattern's own black
/// and white: an annotation has to read as an annotation over a target whose
/// whole job is to be maximally black and white.
const ACCENT: &str = "#33C6E3";
const INK: &str = "#0B132B";
const WARN: &str = "#D2542F";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "book/src/img".to_string());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let wrap = out.join("puzzlepole_wrap_strip.svg");
    std::fs::write(&wrap, wrap_strip_figure()?)?;
    println!("{}", wrap.display());

    let frame = out.join("puzzlepole_frame.svg");
    std::fs::write(&frame, frame_figure())?;
    println!("{}", frame.display());

    Ok(())
}

/// Insert an annotation layer just before an SVG's closing tag.
fn annotate(svg: &str, layer: &str) -> String {
    let close = svg
        .rfind("</svg>")
        .expect("a rendered bundle is well-formed SVG");
    format!("{}{}{}", &svg[..close], layer, &svg[close..])
}

/// The printable wrap strip, with the trim lines and the overlap called out.
///
/// The two things a reader gets wrong unaided are exactly what is marked:
/// where to cut (through the *middle* of the end pieces, not at a boundary,
/// because a dot sits on the joint) and which piece is spare.
fn wrap_strip_figure() -> Result<String, Box<dyn std::error::Error>> {
    const PIECE_MM: f64 = 10.0;
    const AXIAL: u32 = 8;
    const PERIOD: u32 = 12;

    let spec = PuzzlePoleTargetSpec::new(PERIOD, AXIAL, PIECE_MM)
        .expect("period 12 is a supported circumference");
    let strip_pieces = spec.printed_strip_squares();
    let diameter = spec.diameter_mm();

    let target = TargetSpec::PuzzlePole(spec);
    let (board_w, board_h) = target.board_size_mm()?;
    // Room on both sides for the labels, and just enough above and below for a
    // title and a caption. The page centres the board in its printable area, so
    // the exact placement is read back from the layout rather than assumed.
    let side = 46.0_f64;
    let margin = 13.0_f64;
    let mut doc = PrintableTargetDocument::new(target);
    doc.page.size = PageSize::Custom {
        width_mm: board_w + 2.0 * side,
        height_mm: board_h + 2.0 * margin,
    };
    doc.page.margin_mm = margin;

    let layout = doc.resolve_layout()?;
    let x0 = layout.board_origin_mm[0];
    let y0 = layout.board_origin_mm[1];
    let x1 = x0 + layout.board_width_mm;
    let svg = render_target_bundle(&doc)?.svg_text;

    let half = PIECE_MM / 2.0;
    let top_trim = y0 + half;
    let bottom_trim = y0 + layout.board_height_mm - half;
    let overlap_top = bottom_trim - PIECE_MM;

    let mut layer =
        String::from("\n  <g id=\"annotations\" font-family=\"system-ui, sans-serif\">\n");
    // The spare piece, shaded. It is the material that ends up on top.
    layer.push_str(&format!(
        "    <rect x=\"{x0:.3}\" y=\"{overlap_top:.3}\" width=\"{:.3}\" height=\"{PIECE_MM:.3}\" \
         fill=\"{ACCENT}\" fill-opacity=\"0.30\"/>\n",
        x1 - x0
    ));
    // Trim lines. Mid-piece, not at a piece boundary — a dot sits on the joint.
    for y in [top_trim, bottom_trim] {
        layer.push_str(&format!(
            "    <line x1=\"{:.3}\" y1=\"{y:.3}\" x2=\"{:.3}\" y2=\"{y:.3}\" stroke=\"{WARN}\" \
             stroke-width=\"0.6\" stroke-dasharray=\"3 2\"/>\n\
                 <text x=\"{:.3}\" y=\"{:.3}\" font-size=\"4.2\" fill=\"{WARN}\">trim</text>\n",
            x0 - 7.0,
            x1 + 7.0,
            x1 + 9.0,
            y + 1.5
        ));
    }
    layer.push_str(&format!(
        "    <text x=\"{:.3}\" y=\"{:.3}\" font-size=\"4.2\" fill=\"{INK}\">spare</text>\n",
        x1 + 9.0,
        overlap_top + PIECE_MM * 0.66
    ));
    layer.push_str(&format!(
        "    <text x=\"{:.3}\" y=\"{:.3}\" font-size=\"4.6\" font-weight=\"600\" \
         fill=\"{INK}\">Trim on the dashed lines, wrap, lay the spare piece over the \
         first.</text>\n",
        x0 - 34.0,
        y0 - 5.0
    ));
    layer.push_str(&format!(
        "    <text x=\"{:.3}\" y=\"{:.3}\" font-size=\"4.0\" fill=\"{INK}\" \
         fill-opacity=\"0.65\">{strip_pieces} pieces printed · {PERIOD} wrap · 1 spare · \
         ⌀ {diameter:.1} mm</text>\n",
        x0 - 34.0,
        y0 + layout.board_height_mm + 8.0
    ));
    layer.push_str("  </g>\n");

    Ok(annotate(&svg, &layer))
}

/// The pole object frame: the cylinder, its axes, the seam, and the two indices.
///
/// Drawn from `geometry::object_position` under an orthographic projection, so
/// the corner ring in the picture is the corner ring the crate computes — the
/// figure cannot claim a convention the code does not implement.
fn frame_figure() -> String {
    const PERIOD: u32 = 12;
    const CELL: f32 = 30.0;
    const AXIAL_CELLS: u32 = 5;
    /// Axial index the labelled corner ring is drawn at.
    const RING_J: u32 = 2;

    let r = geometry::radius_mm(PERIOD, CELL);
    let height = AXIAL_CELLS as f32 * CELL;

    // Orthographic, tilted slightly down the +Y axis so a ring reads as an
    // ellipse rather than a line. Everything below is in projected units; the
    // viewBox is fitted to the content afterwards rather than guessed, which is
    // what stops the cylinder running off the top.
    let squash = 0.34_f32;
    let scale = 1.7_f32;
    let project = |p: nalgebra::Point3<f32>| -> (f32, f32) {
        (scale * p.x, -scale * p.z + scale * squash * p.y)
    };

    // Bounds of the solid itself. Labels sit in the padding.
    let (half_w, top, bottom) = (
        scale * r,
        -scale * height - scale * squash * r,
        scale * squash * r,
    );
    let pad_left = 24.0_f32;
    let pad_right = 210.0_f32; // the seam label is the widest thing on the page
    let pad_top = 46.0_f32;
    let pad_bottom = 34.0_f32;
    let ox = pad_left + half_w;
    let oy = pad_top - top;
    let width = ox + half_w + pad_right;
    let height_px = oy + bottom + pad_bottom;
    let at = |p: nalgebra::Point3<f32>| -> (f32, f32) {
        let (x, y) = project(p);
        (ox + x, oy + y)
    };

    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.0} {height_px:.0}\" \
         font-family=\"system-ui, -apple-system, sans-serif\">\n"
    );

    // Rings top and bottom, plus a faint one through the labelled corners so
    // they read as a ring rather than as scattered dots.
    let ring_path = |j_mm: f32| -> String {
        (0..=180)
            .map(|k| {
                let t = TAU * k as f32 / 180.0;
                let (x, y) = at(nalgebra::Point3::new(r * t.cos(), r * t.sin(), j_mm));
                format!("{} {x:.2} {y:.2}", if k == 0 { "M" } else { "L" })
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    for (j_mm, opacity) in [(0.0, 0.55), (height, 0.55), (RING_J as f32 * CELL, 0.22)] {
        s.push_str(&format!(
            "  <path d=\"{} Z\" fill=\"none\" stroke=\"{INK}\" stroke-width=\"1.2\" \
             stroke-opacity=\"{opacity}\"/>\n",
            ring_path(j_mm)
        ));
    }
    for sign in [-1.0_f32, 1.0] {
        let (x0, y0) = at(nalgebra::Point3::new(sign * r, 0.0, 0.0));
        let (x1, y1) = at(nalgebra::Point3::new(sign * r, 0.0, height));
        s.push_str(&format!(
            "  <line x1=\"{x0:.2}\" y1=\"{y0:.2}\" x2=\"{x1:.2}\" y2=\"{y1:.2}\" \
             stroke=\"{INK}\" stroke-width=\"1.2\" stroke-opacity=\"0.55\"/>\n"
        ));
    }

    // The cylinder axis, +Z.
    let (ax0, ay0) = at(nalgebra::Point3::new(0.0, 0.0, -0.5 * CELL));
    let (ax1, ay1) = at(nalgebra::Point3::new(0.0, 0.0, height + 0.9 * CELL));
    s.push_str(&format!(
        "  <line x1=\"{ax0:.2}\" y1=\"{ay0:.2}\" x2=\"{ax1:.2}\" y2=\"{ay1:.2}\" \
         stroke=\"{INK}\" stroke-width=\"1.4\" stroke-dasharray=\"5 3\"/>\n\
           <text x=\"{:.2}\" y=\"{:.2}\" font-size=\"13\" font-weight=\"600\" \
         fill=\"{INK}\">+Z</text>\n",
        ax1 + 7.0,
        ay1 + 4.0
    ));

    // The corner ring. Front half solid, back half faded — the depth cue is
    // what makes the seam's position on the *near* side legible.
    for k in 0..PERIOD {
        let p = geometry::object_position(PERIOD, RING_J, k, CELL);
        let (x, y) = at(p);
        let front = p.y >= 0.0;
        let seam = k == 0;
        s.push_str(&format!(
            "  <circle cx=\"{x:.2}\" cy=\"{y:.2}\" r=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/>\n",
            if seam { 4.6 } else { 3.0 },
            if seam { WARN } else { ACCENT },
            if front { 1.0 } else { 0.35 }
        ));
    }

    // Labels. The seam is the one a reader has to find, so it gets a leader.
    let (sx, sy) = at(geometry::object_position(PERIOD, RING_J, 0, CELL));
    let label_x = ox + half_w + 16.0;
    s.push_str(&format!(
        "  <line x1=\"{:.2}\" y1=\"{sy:.2}\" x2=\"{:.2}\" y2=\"{sy:.2}\" stroke=\"{WARN}\" \
         stroke-width=\"0.9\"/>\n\
           <text x=\"{:.2}\" y=\"{:.2}\" font-size=\"12.5\" fill=\"{WARN}\">seam · θ = 0 · \
         k = 0</text>\n",
        sx + 7.0,
        label_x - 4.0,
        label_x,
        sy + 4.2
    ));

    // Which way `k` runs is the other thing a reader cannot infer, and an arrow
    // glyph would be a claim about the projection rather than a reading of it.
    // Draw the arc itself, from the seam along the ring, and put the head on
    // the end — then the direction is whatever the geometry actually does.
    let arc_at = |u: f32| -> (f32, f32) {
        let t = TAU * u / PERIOD as f32;
        at(nalgebra::Point3::new(
            r * t.cos(),
            r * t.sin(),
            RING_J as f32 * CELL,
        ))
    };
    let arc: Vec<(f32, f32)> = (0..=40)
        .map(|i| arc_at(0.35 + 2.3 * i as f32 / 40.0))
        .collect();
    let d: String = arc
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{} {x:.2} {y:.2}", if i == 0 { "M" } else { "L" }))
        .collect::<Vec<_>>()
        .join(" ");
    s.push_str(&format!(
        "  <path d=\"{d}\" fill=\"none\" stroke=\"{ACCENT}\" stroke-width=\"2.2\"/>\n"
    ));
    let (hx, hy) = arc[arc.len() - 1];
    let (px, py) = arc[arc.len() - 4];
    let (dx, dy) = (hx - px, hy - py);
    let n = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / n, dy / n);
    s.push_str(&format!(
        "  <polygon points=\"{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}\" fill=\"{ACCENT}\"/>\n",
        hx + 7.0 * ux,
        hy + 7.0 * uy,
        hx - 3.4 * uy,
        hy + 3.4 * ux,
        hx + 3.4 * uy,
        hy - 3.4 * ux
    ));
    let (kx, ky) = arc_at(2.0);
    s.push_str(&format!(
        "  <text x=\"{:.2}\" y=\"{:.2}\" font-size=\"12.5\" fill=\"{INK}\" \
         text-anchor=\"middle\">k increases</text>\n",
        kx,
        ky + 19.0
    ));

    s.push_str(&format!(
        "  <text x=\"14\" y=\"22\" font-size=\"12.5\" fill=\"{INK}\">\
         <tspan font-weight=\"600\">j</tspan> = axial corner index, along +Z　　\
         <tspan font-weight=\"600\">k</tspan> = cyclic corner index, around</text>\n\
           <text x=\"14\" y=\"{:.2}\" font-size=\"11.5\" fill=\"{INK}\" fill-opacity=\"0.65\">\
         period {PERIOD} · {CELL:.0} mm pieces · ⌀ {:.1} mm</text>\n",
        height_px - 12.0,
        2.0 * r
    ));
    s.push_str("</svg>\n");
    s
}
