# calib-targets-aruco

![Mesh-rectified grid](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/mesh_rectified_small.png)

Embedded ArUco/AprilTag dictionaries and marker decoding utilities.

## Quickstart

```rust
use calib_targets_aruco::{builtins, scan_decode_markers, Matcher, ScanDecodeConfig};
use calib_targets_core::GrayImageView;

fn main() {
    let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
    let matcher = Matcher::new(dict, 1);

    let pixels = vec![0u8; 16 * 16];
    let view = GrayImageView {
        width: 16,
        height: 16,
        data: &pixels,
    };

    let scan_cfg = ScanDecodeConfig::default();
    let markers = scan_decode_markers(&view, 4, 4, 4.0, &scan_cfg, &matcher);
    println!("markers: {}", markers.len());
}
```

## Notes

- Built-in dictionaries are compiled in; see `builtins::BUILTIN_DICTIONARY_NAMES`.
- Decoding expects rectified grids or per-cell quads.

## Features

- `tracing`: enables tracing output for decoding steps.

## Python bindings

Python bindings are provided via the workspace facade (`calib_targets` module).
See `python/README.md` in the repo root for setup.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
