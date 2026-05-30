//! Per-detector create/destroy/detect/diagnostics C ABI.
//!
//! Split into two cohesive halves:
//!
//! - [`impls`] — the `pub(super)` implementation functions that do the real
//!   work, calling into the shared output-buffer helpers in `lib.rs`.
//! - [`exports`] — the `#[repr(C)]` arg/buffer structs and the exported
//!   `#[no_mangle]` C symbols, each a thin panic-trapping wrapper over an
//!   `impls` function.
//!
//! Everything `exports` makes public is re-exported here so the rest of the
//! crate (and cbindgen) sees `crate::detectors::ct_*` exactly as before the
//! split.

mod exports;
mod impls;

// Glob re-export so the arg/buffer structs (consumed by `impls` and the
// `lib.rs` test module) and the exported `#[no_mangle]` symbols are visible
// as `crate::detectors::*`, exactly as when they lived in one file. The
// `#[no_mangle]` functions are linked as C symbols regardless; the re-export
// only serves the in-crate `crate::detectors::ct_*` paths.
pub use exports::*;
