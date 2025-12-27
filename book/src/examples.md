# Examples

Examples live under `crates/*/examples/` and are built per crate. Many examples accept a JSON config file (defaults point to `testdata/` or `tmpdata/`), while the facade examples under `calib-targets` take an image path directly.

To run an example from the workspace root:

```bash
cargo run -p calib-targets-chessboard --example chessboard -- testdata/chessboard_config.json
```

See the sub-chapters for what each example produces and how to interpret the outputs.
