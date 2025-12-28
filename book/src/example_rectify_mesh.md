# Mesh Rectification Example

File: `crates/calib-targets-aruco/examples/rectify_mesh.rs`

This example detects a chessboard, performs per-cell mesh rectification, and scans the rectified grid for ArUco markers. It writes:

![Mesh rectification output](img/mesh_rectified_small.png)
*Per-cell mesh rectification output from the small test image.*

- A mesh-rectified grayscale image.
- A JSON report with rectification info and marker detections.

The code defaults to `testdata/rectify_mesh_config_small0.json`, and that config is a good
starting point (input: `testdata/small0.png`, mesh output: `tmpdata/mesh_rectified_small0.png`,
report: `tmpdata/rectify_mesh_report_small0.json`).

Run it with:

```bash
cargo run -p calib-targets-aruco --example rectify_mesh -- testdata/rectify_mesh_config_small0.json
```

This is a good reference for the full grid -> rectification -> marker scan pipeline.
