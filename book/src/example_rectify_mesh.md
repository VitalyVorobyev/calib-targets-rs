# Mesh Rectification Example

File: `crates/calib-targets/examples/rectify_mesh.rs`

This example detects a chessboard, performs per-cell mesh rectification, and scans the rectified grid for ArUco markers. It writes:

- A mesh-rectified grayscale image.
- A JSON report with rectification info and marker detections.

The code defaults to `tmpdata/rectify_config.json`, but you can use `testdata/rectify_config.json` as a starting point.

Run it with:

```bash
cargo run -p calib-targets --example rectify_mesh -- testdata/rectify_config.json
```

This is a good reference for the full grid -> rectification -> marker scan pipeline.
