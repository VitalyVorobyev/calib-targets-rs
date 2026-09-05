# calib-targets C API — native archives

Prebuilt `calib-targets-ffi` archives for Linux, macOS and Windows: the shared
library, the generated `calib_targets_ffi.h`, the header-only C++ wrapper
`calib_targets_ffi.hpp`, and a CMake package you can point `find_package` at.

Consumption walkthrough:
[`docs/ffi/cmake-consumer-quickstart.md`](../ffi/cmake-consumer-quickstart.md).

The C ABI carries **its own version**, independent of the Rust workspace,
because it evolves on a different cadence. The archive filename is the ABI
version, not the workspace version.

## ABI 5.0.0 — breaking

Paired with workspace 0.15.0. **Relink; recompiling is not enough** — three
marker-board structs change size, so a caller built against 4.0.0 passes
structs of the wrong layout and is silently misaligned rather than failing to
link. Nothing outside the marker board is touched: a consumer using only the
chessboard, ChArUco or PuzzleBoard entry points rebuilds against the shipped
header and is done.

**The printed disk diameter moved onto the board.**

- `ct_marker_board_layout_t` gains `float circle_diameter_rel` — the printed
  disk diameter as a fraction of the square side, typically `0.5`. Every radius
  the circle scorer probes is relative to it.
- `ct_circle_score_params_t` loses `diameter_frac`, which stated the same
  quantity. Two copies of one physical fact could disagree, and did: a board
  printed at `0.45` was probed at `0.5`, putting every scorer radius 11% off.
  Move the value; do not duplicate it.

**The circle matcher lost its distance gate.**

- `ct_circle_match_params_t` loses `max_distance_cells`. Frame resolution is now
  hypothesis-and-verify over the four rotations rather than nearest-cell
  matching, so a match is an exact integer coincidence and a distance tolerance
  has nothing to tolerate.
- `min_offset_inliers` keeps its type and its meaning but its *recommended*
  value changes from `1` to `3` — the whole layout. The three circles exist only
  to break the board's 4-fold rotational symmetry, and one circle is consistent
  with all four rotations. The Rust default moved with it; a C caller fills the
  struct itself and should set `3` unless it is knowingly trading the
  never-a-wrong-frame guarantee for recall.

**One new failure mode reaches C as a miss.**
`ct_marker_board_detector_detect` now returns `CT_STATUS_NOT_FOUND` when two
board frames explain the detected circles equally well, where it previously
returned a confidently rotated result. `ct_marker_board_detector_diagnose_json`
carries the detail: `alignment_ambiguous`, `alignment_runner_up_inliers`, and a
`squareness` reading on every scored circle candidate.

Full Rust-side context is in the
[0.15 migration guide](../migrations/0.15.0.md) and
[`CHANGELOG.md`](../../CHANGELOG.md).

## ABI 4.0.0 — breaking

Paired with workspace 0.13.0, and unchanged in both 0.14.0 and 0.14.1 — the
archives for those releases carry this same ABI, so a consumer already built
against 4.0.0 needs no action. Recompile; do not relink.

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
