# calib-targets-cli

Repo-local command-line utilities for `calib-targets`.

Today this crate is the official repo-local app for printable target
generation. It is not published on crates.io; run it from this workspace with
`cargo run -p calib-targets-cli -- ...`.

## Discover dictionaries

Use this when initializing a ChArUco target and you need a valid built-in
dictionary name:

```bash
cargo run -p calib-targets-cli -- list-dictionaries
```

## Initialize, validate, then generate

```bash
cargo run -p calib-targets-cli -- init charuco \
  --out testdata/printable/charuco_a4.json \
  --rows 5 \
  --cols 7 \
  --square-size-mm 20 \
  --marker-size-rel 0.75 \
  --dictionary DICT_4X4_50

cargo run -p calib-targets-cli -- validate \
  --spec testdata/printable/charuco_a4.json

cargo run -p calib-targets-cli -- generate \
  --spec testdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

`validate` prints `valid <target-kind>` on success and exits non-zero when the
spec does not pass printable validation.

## Generate from an existing spec

```bash
cargo run -p calib-targets-cli -- generate \
  --spec testdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

Available commands:

- `list-dictionaries`
- `init chessboard`
- `init charuco`
- `init marker-board`
- `validate`
- `generate`

For the canonical JSON model, Rust/Python entry points, and print-scale
guidance, see [book/src/printable.md](../../book/src/printable.md).
