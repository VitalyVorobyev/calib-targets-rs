//! Point samplers for the three printable output formats.
//!
//! Each sampler answers one question — "what colour is the artwork at this
//! millimetre coordinate?" — so a conformance assertion can be written once and
//! run against SVG, PNG and DXF alike. SVG and DXF are vector formats, so their
//! samplers reconstruct the painted rectangles and test point containment; the
//! PNG sampler just reads the pixel.

/// The two colours a printable target is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Black,
    White,
}

/// One painted axis-aligned rectangle, in page millimetres with y running down.
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    ink: Ink,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// A rendered target that can be probed at a point.
pub struct Sampler {
    repr: Repr,
}

enum Repr {
    /// Painter's-order rectangle list (SVG and DXF).
    Vector {
        rects: Vec<Rect>,
        /// Colour where no rectangle covers the point.
        background: Ink,
    },
    /// Decoded 8-bit grayscale raster.
    Raster {
        pixels: Vec<u8>,
        width: usize,
        height: usize,
        px_per_mm: f64,
    },
}

impl Sampler {
    /// Colour of the artwork at `(x_mm, y_mm)`, y running down from the page's
    /// top-left corner.
    pub fn ink_at(&self, x_mm: f64, y_mm: f64) -> Ink {
        match &self.repr {
            // Later rectangles paint over earlier ones, so the last hit wins.
            Repr::Vector { rects, background } => rects
                .iter()
                .rev()
                .find(|r| r.contains(x_mm, y_mm))
                .map_or(*background, |r| r.ink),
            Repr::Raster {
                pixels,
                width,
                height,
                px_per_mm,
            } => {
                let px = (x_mm * px_per_mm).floor().max(0.0) as usize;
                let py = (y_mm * px_per_mm).floor().max(0.0) as usize;
                let px = px.min(width - 1);
                let py = py.min(height - 1);
                if pixels[py * width + px] < 128 {
                    Ink::Black
                } else {
                    Ink::White
                }
            }
        }
    }

    /// Parse the `<rect>` elements the SVG emitter writes.
    ///
    /// The emitter produces only axis-aligned `<rect>` elements with literal
    /// `#000000` / `#ffffff` fills, in painter's order, so a scan for the four
    /// attributes is exact — no general SVG support is needed or wanted here.
    pub fn from_svg(svg: &str) -> Self {
        let mut rects = Vec::new();
        for tag in svg.split("<rect ").skip(1) {
            let tag = &tag[..tag.find("/>").expect("unterminated <rect>")];
            let attr = |name: &str| -> f64 {
                let key = format!("{name}=\"");
                let start = tag
                    .find(&key)
                    .unwrap_or_else(|| panic!("no {name} in {tag}"))
                    + key.len();
                let rest = &tag[start..];
                rest[..rest.find('"').expect("unterminated attribute")]
                    .parse()
                    .expect("numeric attribute")
            };
            let ink = if tag.contains("fill=\"#000000\"") {
                Ink::Black
            } else {
                Ink::White
            };
            rects.push(Rect {
                x: attr("x"),
                y: attr("y"),
                w: attr("width"),
                h: attr("height"),
                ink,
            });
        }
        Self {
            repr: Repr::Vector {
                rects,
                background: Ink::White,
            },
        }
    }

    /// Decode the PNG bundle output and sample it as a raster.
    pub fn from_png(png_bytes: &[u8], dpi: u32) -> Self {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().expect("png header");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
        let info = reader.next_frame(&mut buf).expect("png frame");
        assert_eq!(
            info.color_type,
            png::ColorType::Grayscale,
            "printable PNGs are 8-bit grayscale"
        );
        buf.truncate(info.buffer_size());
        Self {
            repr: Repr::Raster {
                pixels: buf,
                width: info.width as usize,
                height: info.height as usize,
                px_per_mm: f64::from(dpi) / 25.4,
            },
        }
    }

    /// Reconstruct the DXF's black regions, undoing the Y-flip into DXF's
    /// cartesian (Y-up) space so the sampler shares the SVG coordinate frame.
    ///
    /// The emitter writes each black rectangle as a closed `LWPOLYLINE` whose
    /// vertices carry group codes `10` (x) and `20` (y). Anything the DXF omits
    /// is unpatterned substrate, i.e. white.
    pub fn from_dxf(dxf: &str, page_height_mm: f64) -> Self {
        let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
        let mut rects = Vec::new();
        let (mut xs, mut ys): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        let mut in_polyline = false;

        let mut flush = |xs: &mut Vec<f64>, ys: &mut Vec<f64>| {
            if xs.is_empty() {
                return;
            }
            let (x0, x1) = (min_of(xs), max_of(xs));
            let (y0, y1) = (min_of(ys), max_of(ys));
            rects.push(Rect {
                x: x0,
                // DXF y grows upward from the page bottom; flip it back.
                y: page_height_mm - y1,
                w: x1 - x0,
                h: y1 - y0,
                ink: Ink::Black,
            });
            xs.clear();
            ys.clear();
        };

        let mut i = 0;
        while i + 1 < lines.len() {
            let (code, value) = (lines[i], lines[i + 1]);
            match code {
                "0" => {
                    flush(&mut xs, &mut ys);
                    in_polyline = value == "LWPOLYLINE";
                }
                "10" if in_polyline => xs.push(value.parse().expect("dxf x")),
                "20" if in_polyline => ys.push(value.parse().expect("dxf y")),
                _ => {}
            }
            i += 2;
        }
        flush(&mut xs, &mut ys);

        Self {
            repr: Repr::Vector {
                rects,
                background: Ink::White,
            },
        }
    }
}

fn min_of(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max_of(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
