# Global Rectification Example

File: `crates/calib-targets-chessboard/examples/rectify_global.rs`

This example detects a chessboard and computes a single global homography to produce a rectified board view. The output includes:

- A rectified grayscale image.
- A JSON report with homography matrices and grid bounds.

The code defaults to `tmpdata/rectify_config.json`, but a ready-made config exists in `testdata/rectify_config.json`.

Run it with:

```bash
cargo run -p calib-targets-chessboard --example rectify_global -- testdata/rectify_config.json
```

If rectification succeeds, the rectified image is written to `tmpdata/rectified.png` unless overridden in the config.
