# calib-targets-rs

Calibration target detection in Rust (chessboard, ChArUco, ArUco/AprilTag, marker boards).

![ChArUco detection overlay](book/img/charuco_detect_report_small2_overlay.png)

> **Status:** experimental and evolving. APIs may change.

## Quickstart

```bash
cargo add calib-targets image
```

```rust,no_run
use calib_targets::detect;
use calib_targets::chessboard::{ChessboardParams, GridGraphParams};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config();
    let params = ChessboardParams::default();
    let graph = GridGraphParams::default();

    let result = detect::detect_chessboard(&img, &chess_cfg, params, graph);
    println!("detected: {}", result.is_some());
    Ok(())
}
```

## Crates

- `calib-targets` – facade crate with end-to-end helpers.
- `calib-targets-core` – core geometry and types.
- `calib-targets-chessboard` – chessboard detector.
- `calib-targets-aruco` – ArUco/AprilTag dictionaries and decoding.
- `calib-targets-charuco` – ChArUco alignment and IDs.
- `calib-targets-marker` – checkerboard + 3-circle marker boards.

## Examples

```bash
cargo run -p calib-targets --example detect_chessboard -- path/to/image.png
cargo run -p calib-targets --example detect_charuco -- path/to/image.png
cargo run -p calib-targets-aruco --example rectify_mesh -- testdata/rectify_mesh_config_small0.json
```

## Performance and accuracy

Benchmarks are coming. The goal is to be the fastest detector in this class while maintaining high sensitivity and accuracy.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo doc --workspace --all-features
mdbook build book
```

For contribution rules see [AGENTS.md](./AGENTS.ms).

## License

This project is dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE` and `LICENSE-APACHE`.
