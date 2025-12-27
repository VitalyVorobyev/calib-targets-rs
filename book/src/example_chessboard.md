# Chessboard Detection Example

File: `crates/calib-targets-chessboard/examples/chessboard.rs`

This example runs the full chessboard pipeline:

![Chessboard detection overlay](../img/chessboard_detection_mid_overlay.png)
*Example output overlay for chessboard detection.*

1. Detects ChESS corners using the `chess-corners` crate.
2. Adapts them to `calib_targets_core::Corner`.
3. Runs `ChessboardDetector`.
4. Optionally outputs debug data (orientation histogram, grid graph).

The default config is `testdata/chessboard_config.json` (input: `testdata/mid.png`,
output: `tmpdata/chessboard_detection_mid.json`).

Run it with:

```bash
cargo run -p calib-targets-chessboard --example chessboard -- testdata/chessboard_config.json
```

The output JSON contains detected corners, grid coordinates, and optional debug diagnostics
(if `debug_outputs` are enabled in the config).
