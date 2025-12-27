# Examples

Examples live under `crates/*/examples/` and are built per crate. Each example accepts a JSON config file; defaults point to `testdata/` or `tmpdata/`.

To run an example from the workspace root:

```bash
cargo run -p calib-targets-chessboard --example chessboard -- testdata/chessboard_config.json
```

See the sub-chapters for what each example produces and how to interpret the outputs.
