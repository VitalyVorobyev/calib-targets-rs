# calib-targets 0.17.0

Two fixes that stop wrong labels on boards blurred past readability: ChArUco
no longer places the board on a marker read no stronger than chance, and the
chessboard's final wrong-label check now also runs on small detections. Both
apply to the native library unchanged in shape. The Rust crates add per-corner
blur measurement (not yet in the C API) and remove some rarely used Rust
surface; see the
[workspace changelog](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/CHANGELOG.md#0170).

## Native C API archives

Prebuilt `calib-targets-ffi` archives for Linux, macOS and Windows: the shared
library, the generated `calib_targets_ffi.h`, the header-only C++ wrapper
`calib_targets_ffi.hpp`, and a CMake package you can point `find_package` at.

Consumption walkthrough:
[`docs/ffi/cmake-consumer-quickstart.md`](../ffi/cmake-consumer-quickstart.md).

The C ABI carries **its own version**, independent of the Rust workspace,
because it evolves on a different cadence. The archive filename is the ABI
version, not the workspace version.

## ABI 6.0.3

Workspace 0.17.0 ships the 6.0.3 native ABI, a fix with no type, layout or
symbol change since 6.0.0: a caller built against 6.0.0–6.0.2 links and runs
unchanged. ChArUco
diagnostics JSON can now carry the `evidence_at_chance` rejection. A caller
built against 5.x or earlier must relink and rebuild every caller-allocated
array, because several structs differ in size. Older ABI history is in
[`CHANGELOG.md`](../../CHANGELOG.md) and the
[0.16 migration guide](../migrations/0.16.0.md).

## Verifying an archive

Every archive is built and smoke-tested in CI by the native consumer, CMake
consumer, and release-archive tests before upload. `ct_version_string()`
returns the ABI version the library was built as.
