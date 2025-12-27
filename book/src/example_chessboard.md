# Chessboard Detection Example

File: `crates/calib-targets-chessboard/examples/chessboard.rs`

This example runs the full chessboard pipeline:

1. Detects ChESS corners using the `chess-corners` crate.
2. Adapts them to `calib_targets_core::Corner`.
3. Runs `ChessboardDetector`.
4. Optionally outputs debug data (orientation histogram, grid graph).

The default config is `testdata/chessboard_config.json`.

Run it with:

```bash
cargo run -p calib-targets-chessboard --example chessboard -- testdata/chessboard_config.json
```

The output JSON contains detected corners, grid coordinates, and optional debug diagnostics.
