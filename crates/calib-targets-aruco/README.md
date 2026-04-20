# calib-targets-aruco

Embedded ArUco / AprilTag dictionaries and marker decoding primitives.
This is the low-level layer: it does **not** do quad detection. It expects
a rectified view or explicit cell-quad corners (typically supplied by
[`calib-targets-chessboard`](https://docs.rs/calib-targets-chessboard))
and decodes each cell into a marker ID.

Most users go through the facade [`calib-targets`][facade] or the
[`calib-targets-charuco`][charuco] crate rather than calling this directly.

[facade]: https://docs.rs/calib-targets
[charuco]: https://docs.rs/calib-targets-charuco

## Install

```toml
[dependencies]
calib-targets-aruco = "0.7"
calib-targets-core = "0.7"
```

## Quickstart

```rust
use calib_targets_aruco::{builtins, scan_decode_markers, Matcher, ScanDecodeConfig};
use calib_targets_core::GrayImageView;

let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
let matcher = Matcher::new(dict, 1);            // Hamming tolerance

let pixels = vec![0u8; 16 * 16];
let view = GrayImageView { width: 16, height: 16, data: &pixels };

let cfg = ScanDecodeConfig::default();
let markers = scan_decode_markers(&view, 4, 4, 4.0, &cfg, &matcher);
println!("markers: {}", markers.len());
```

## Built-in dictionaries

Dictionary data files (`data/*_CODES.json`) are compiled into the binary:

| Family | Dictionaries |
|---|---|
| Classic ArUco | `DICT_4X4_{50,100,250,1000}`, `DICT_5X5_{50,100,250,1000}`, `DICT_6X6_{50,100,250,1000}`, `DICT_7X7_{50,100,250,1000}` |
| Original ArUco | `DICT_ARUCO_ORIGINAL`, `DICT_ARUCO_MIP_36h12` |
| AprilTag | `DICT_APRILTAG_16h5`, `DICT_APRILTAG_25h9`, `DICT_APRILTAG_36h10`, `DICT_APRILTAG_36h11` |

Resolve by name with `builtins::builtin_dictionary(name)`.

## Inputs and outputs

| Call | Input | Output |
|---|---|---|
| [`scan_decode_markers`] | `&GrayImageView` + grid shape + cell size + config + matcher | `Vec<MarkerDetection>` (one per decoded cell) |
| [`scan_decode_markers_in_cells`] | `&GrayImageView` + `&[MarkerCell]` + config + matcher | `Vec<MarkerDetection>` |
| [`decode_marker_in_cell`] | a single `MarkerCell` | `Option<MarkerDetection>` |
| [`Matcher::best_match`] | raw `u64` code bits | `Option<(id, rotation, hamming)>` |

[`MarkerDetection`] carries `id`, `grid_coords`, `rotation` (0..3, 90° steps),
`hamming`, `score`, and the rectified-rectangle corners used to produce it.

## Configuration

[`ScanDecodeConfig`] is the main tuning struct:

| Field | Default | Effect |
|---|---|---|
| `border_bits` | 1 | Marker border width in cells (OpenCV default 1). |
| `inset_frac` | 0.08 | Per-cell edge inset to avoid sampling cell borders. Raise if markers print with soft edges. |
| `marker_size_rel` | 0.75 | Marker side relative to the enclosing chessboard cell. Match the printed target. |
| `min_border_score` | 0.7 | Minimum "frame looks like a marker border" score to accept a cell. Lower to recover low-contrast markers. |
| `multi_threshold` | `false` | Try several local thresholds per cell. Enable for uneven illumination. |
| `dedup_by_id` | `true` | Keep one detection per marker ID (highest score). Disable when multiple boards share a dictionary. |

[`Matcher::new(dict, max_hamming)`] — the second arg is the maximum
Hamming distance a candidate code may differ from a dictionary entry. Use
0 for clean synthetic targets, 1–2 for printed and photographed boards.

## Tuning difficult cases

- **Small markers (<12 px across)** — raise the rectification resolution
  upstream (`CharucoParams::px_per_square`) rather than loosening
  `min_border_score`; small markers admit more false positives.
- **Uneven illumination** — set `multi_threshold = true`; otherwise glare
  patches lose all markers inside them.
- **Motion blur** — raise `max_hamming` on the `Matcher` by 1 step; the
  marker-decoding stage is the single point where blur shows up as bit
  flips.
- **Very similar dictionaries** — prefer `DICT_5X5_*` or `DICT_6X6_*` over
  `DICT_4X4_*` when you have space to print them; the inter-code Hamming
  distance is larger.

## Limitations

- **No quad detector.** Input must be a rectified grid view (from
  `calib-targets-chessboard`) or explicit cell corners. Plain image → marker
  decoding is not provided here.
- **No localisation.** Output is a set of marker IDs with their cell /
  rectified-space corners; pose estimation lives in downstream crates.
- **Single-dictionary matcher.** A detector run scans against one
  dictionary at a time.

## Feature flags

- `tracing` — enables tracing spans on the decoding path.

## Related

- [`calib-targets-charuco`][charuco] — full ChArUco detector built on
  `calib-targets-chessboard` and this crate.
- [Book: ArUco decoding details](https://vitalyvorobyev.github.io/calib-targets-rs/aruco_decoding.html)
- [Book: ChArUco alignment + refinement](https://vitalyvorobyev.github.io/calib-targets-rs/charuco_alignment.html)
