# calib-targets-cli

Repo-local command-line utilities for `calib-targets`.

Today this crate is primarily the printable-target generator CLI. It is not
published on crates.io; run it from this workspace with `cargo run -p
calib-targets-cli -- ...`.

## Generate from an existing spec

```bash
cargo run -p calib-targets-cli -- generate \
  --spec testdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

## Initialize and then generate

```bash
cargo run -p calib-targets-cli -- init charuco \
  --out tmpdata/printable/charuco_a4.json \
  --rows 5 \
  --cols 7 \
  --square-size-mm 20 \
  --marker-size-rel 0.75 \
  --dictionary DICT_4X4_50

cargo run -p calib-targets-cli -- generate \
  --spec tmpdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

The available `init` targets are `chessboard`, `charuco`, and `marker-board`.
For the canonical JSON model, Rust/Python entry points, and print-scale
guidance, see [book/src/printable.md](../../book/src/printable.md).
