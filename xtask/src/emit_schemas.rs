//! Emit the JSON Schemas of the configs the npm (wasm) package accepts.
//!
//! Output goes to `schemas/<name>.json` at the repo root, one file per
//! wasm-facing config. With `--check`, the command instead verifies that the
//! committed files match what would be generated from the current source; CI
//! runs this to catch drift between the config types and the shipped schemas.

use anyhow::{bail, Context, Result};
use schemars::{schema_for, JsonSchema};
use serde_json::Value;
use std::path::Path;

/// Directory (relative to the workspace root) the schemas are written to.
pub const SCHEMA_DIR: &str = "schemas";

/// One shipped schema: file name and the wasm entry points that read the value.
pub struct SchemaFile {
    pub name: &'static str,
    pub schema: Value,
}

/// Schema of `T` as a JSON value, after the cosmetic clean-ups in [`tidy`].
fn schema_of<T: JsonSchema>() -> Value {
    let mut schema =
        serde_json::to_value(schema_for!(T)).expect("JsonSchema serialization is infallible");
    tidy(&mut schema);
    schema
}

/// `CharucoParams`, with the one field the detector overwrites marked
/// `readOnly`.
///
/// `CharucoDetector::new` assigns `scan.border_bits = board.border_bits`
/// unconditionally (the two describe the same printed ring, and the board is
/// the source of truth), so whatever a caller puts in `scan.border_bits` is
/// discarded. The test `charuco_scan_border_bits_is_derived_from_the_board`
/// pins that behaviour. The annotation lives here rather than on
/// `ScanDecodeConfig::border_bits` because that type is also used standalone
/// by the ArUco scanner, where the field is an ordinary knob.
fn charuco_params_schema() -> Value {
    let mut schema = schema_of::<calib_targets_charuco::CharucoParams>();
    let field = schema
        .pointer_mut("/$defs/ScanDecodeConfig/properties/border_bits")
        .and_then(Value::as_object_mut)
        .expect("ScanDecodeConfig.border_bits is in the charuco schema");
    field.insert("readOnly".into(), Value::Bool(true));
    let note = "Derived, not configured: the ChArUco detector overwrites it with \
                `board.border_bits`, so set the border width on the board.";
    let description = match field.get("description").and_then(Value::as_str) {
        Some(text) => format!("{text}\n\n{note}"),
        None => note.to_owned(),
    };
    field.insert("description".into(), Value::String(description));
    schema
}

/// Every schema shipped in the npm package.
///
/// The `*_sweep_*` presets and the `detect_*_best` entry points take a plain
/// array of the matching params type, so they need no schema of their own.
pub fn schemas() -> Vec<SchemaFile> {
    vec![
        // `detect_corners` / `detect_chessboard*` `chess_cfg`, and the `chess`
        // field of the charuco / marker-board / puzzleboard params.
        SchemaFile {
            name: "chess_config.json",
            schema: schema_of::<calib_targets_core::DetectorConfig>(),
        },
        SchemaFile {
            name: "chessboard_params.json",
            schema: schema_of::<calib_targets_chessboard::ChessboardParams>(),
        },
        SchemaFile {
            name: "charuco_params.json",
            schema: charuco_params_schema(),
        },
        SchemaFile {
            name: "marker_board_params.json",
            schema: schema_of::<calib_targets_marker::MarkerBoardParams>(),
        },
        // The `spec` argument of `marker_board_sweep_for_board`.
        SchemaFile {
            name: "marker_board_spec.json",
            schema: schema_of::<calib_targets_marker::MarkerBoardSpec>(),
        },
        SchemaFile {
            name: "puzzleboard_params.json",
            schema: schema_of::<calib_targets_puzzleboard::PuzzleBoardParams>(),
        },
        // The `doc` argument of `render_target_bundle_json`.
        SchemaFile {
            name: "printable_target_document.json",
            schema: schema_of::<calib_targets_print::PrintableTargetDocument>(),
        },
    ]
}

/// Pretty-printed schema text with a trailing newline.
pub fn render(schema: &Value) -> Result<String> {
    let mut text =
        serde_json::to_string_pretty(schema).context("rendering schema as pretty JSON")?;
    text.push('\n');
    Ok(text)
}

/// Cosmetic, validation-neutral clean-ups so the schema reads well in a
/// form UI:
///
/// - `f32` defaults/bounds widen to `f64` when derived (`0.001_f32` becomes
///   `0.0010000000474974513`); numbers that are exactly an `f32` are
///   rewritten to that `f32`'s shortest decimal form.
/// - rustdoc intra-doc links (``[`Foo`]`` and ``[`Foo`](path)``) in
///   `description` strings are reduced to inline code (`` `Foo` ``), fenced
///   Rust code blocks are dropped, and `crate::` / `Self::` path prefixes are
///   removed (they mean nothing to a form reader).
/// - a `oneOf` made only of string `const` branches (a unit-variant enum)
///   also gets `type: "string"` and an `enum` list, which simple form
///   generators recognise as a closed set. The `oneOf` stays so the
///   per-variant descriptions are not lost; the two are equivalent.
pub(crate) fn tidy(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                if n.is_f64() && x.is_finite() && (x as f32) as f64 == x {
                    if let Ok(short) = format!("{}", x as f32).parse::<f64>() {
                        if let Some(num) = serde_json::Number::from_f64(short) {
                            *n = num;
                        }
                    }
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(tidy),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "description" {
                    if let Value::String(text) = child {
                        *text = clean_description(text);
                    }
                } else {
                    tidy(child);
                }
            }
            add_enum_for_const_one_of(map);
        }
        _ => {}
    }
}

fn add_enum_for_const_one_of(map: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(branches)) = map.get("oneOf") else {
        return;
    };
    let consts: Option<Vec<Value>> = branches
        .iter()
        .map(|b| match (b.get("const"), b.get("type")) {
            (Some(c @ Value::String(_)), Some(Value::String(t))) if t == "string" => {
                Some(c.clone())
            }
            _ => None,
        })
        .collect();
    if let Some(consts) = consts.filter(|c| !c.is_empty()) {
        map.insert("type".into(), Value::String("string".into()));
        map.insert("enum".into(), Value::Array(consts));
    }
}

/// Doc-comment prose to form-friendly text: see [`tidy`].
fn clean_description(text: &str) -> String {
    let text = strip_code_fences(&strip_intra_doc_links(text));
    text.replace("crate::", "").replace("Self::", "")
}

/// Drop fenced (```` ``` ````) code blocks, e.g. rustdoc examples, together
/// with the blank line that separated them from the preceding prose.
fn strip_code_fences(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push(line);
        }
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Turn ``[`Foo`]`` / ``[`Foo`](target)`` into `` `Foo` ``.
pub(crate) fn strip_intra_doc_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[`") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // `after` begins with the opening backtick of the code span.
        let Some(end) = after[1..].find('`').map(|i| i + 1) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let code = &after[..=end];
        let tail = &after[end + 1..];
        if let Some(tail) = tail.strip_prefix(']') {
            out.push_str(code);
            rest = match tail.strip_prefix('(') {
                Some(link) => link.split_once(')').map_or(tail, |(_, r)| r),
                None => tail,
            };
        } else {
            out.push('[');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub fn run(workspace_root: &Path, check: bool) -> Result<()> {
    let dir = workspace_root.join(SCHEMA_DIR);
    let entries = schemas();

    if check {
        let mut drift: Vec<String> = Vec::new();
        for file in &entries {
            let path = dir.join(file.name);
            let expected = render(&file.schema)?;
            match std::fs::read_to_string(&path) {
                Err(_) => drift.push(format!("{SCHEMA_DIR}/{} is missing", file.name)),
                Ok(on_disk) if on_disk == expected => {}
                // Same bytes once CRLF is normalised: a checkout problem
                // (`.gitattributes` forces LF), not a stale schema.
                Ok(on_disk) if on_disk.replace("\r\n", "\n") == expected => drift.push(format!(
                    "{SCHEMA_DIR}/{} has CRLF line endings; re-checkout with LF \
                     (`git add --renormalize .`), do not re-emit",
                    file.name
                )),
                Ok(_) => drift.push(format!("{SCHEMA_DIR}/{} is out of date", file.name)),
            }
        }
        // A stale file for a config that no longer ships is drift too.
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".json") && !entries.iter().any(|f| f.name == name) {
                    drift.push(format!("{SCHEMA_DIR}/{name} is not generated any more"));
                }
            }
        }
        if drift.is_empty() {
            println!(
                "schemas up to date ({} files in {SCHEMA_DIR}/)",
                entries.len()
            );
            return Ok(());
        }
        for line in &drift {
            eprintln!("schema drift: {line}");
        }
        bail!(
            "{} schema file(s) differ from the source; run `cargo xtask emit-schemas` and commit the result",
            drift.len()
        );
    }

    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for file in &entries {
        let path = dir.join(file.name);
        std::fs::write(&path, render(&file.schema)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    println!("emitted {} schemas to {}", entries.len(), dir.display());
    Ok(())
}
