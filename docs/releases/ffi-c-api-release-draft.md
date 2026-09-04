# calib-targets C API — native archives

Prebuilt `calib-targets-ffi` archives for Linux, macOS and Windows: the shared
library, the generated `calib_targets_ffi.h`, the header-only C++ wrapper
`calib_targets_ffi.hpp`, and a CMake package you can point `find_package` at.

Consumption walkthrough:
[`docs/ffi/cmake-consumer-quickstart.md`](../ffi/cmake-consumer-quickstart.md).

The C ABI carries **its own version**, independent of the Rust workspace,
because it evolves on a different cadence. The archive filename is the ABI
version, not the workspace version.

## ABI 4.0.0 — breaking

Paired with workspace 0.13.0, and unchanged in 0.14.0 — the 0.14.0 archives
carry this same ABI, so a consumer already built against 4.0.0 needs no
action. Recompile; do not relink.

**Three entry points renamed.** The `_with_diagnostics` / `detect_diagnostics`
spelling is gone workspace-wide; `diagnose` is the one name for "run the
pipeline and also return evidence".

| before | after |
|---|---|
| `ct_charuco_detector_detect_diagnostics_json` | `ct_charuco_detector_diagnose_json` |
| `ct_marker_board_detector_detect_diagnostics_json` | `ct_marker_board_detector_diagnose_json` |
| `ct_puzzleboard_detector_detect_diagnostics_json` | `ct_puzzleboard_detector_diagnose_json` |

**Two structs grew fields** — a layout break, so a stale caller passing the old
struct is silently misaligned rather than failing to link. Rebuild against the
shipped header.

- `ct_puzzleboard_decode_config_t` gains `symmetry_mode`, taking
  `CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS` (the new default: the four 90°
  rotations, all a camera can physically see of a printed board) or
  `CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS_AND_REFLECTIONS` (the previous
  eight-transform search — set this only when the optical path flips
  handedness).
- `ct_puzzleboard_result_t` gains `logical_bits`, `logical_bit_error_rate` and
  `dot_dissent_rate`, from the new period-3 consensus stage. `dot_dissent_rate`
  is the useful one for a host application: it is computed before any pose
  hypothesis, so it is a read-quality meter that stays meaningful even when the
  decode is wrong, and it rises sharply on a mislabelled grid.

**One behaviour change.** `ct_marker_board_detector_diagnose_json` now returns
`CT_STATUS_OK` with a well-formed payload on a *failed* detection, where it
previously returned `CT_STATUS_NOT_FOUND`. A caller that treated `NOT_FOUND` as
"no diagnostics available" will now receive the scored circle candidates and
attempted matches — which is exactly when they are worth having. This matches
the ChArUco and PuzzleBoard diagnostics entry points.

Full Rust-side context, including the detector/free-function restructuring this
ABI change follows from, is in the
[0.13 migration guide](../migrations/0.13.0.md) and
[`CHANGELOG.md`](../../CHANGELOG.md).

## Verifying an archive

Every archive is built and smoke-tested in CI by the native consumer, CMake
consumer, and release-archive tests before upload. `ct_version_string()`
returns the ABI version the library was built as.
