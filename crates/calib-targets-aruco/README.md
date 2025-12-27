# calib-targets-aruco

Embedded ArUco/AprilTag dictionaries and marker decoding utilities.

## Example

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

## Features

- `tracing`: enables tracing output for decoding steps.

## Links

- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
