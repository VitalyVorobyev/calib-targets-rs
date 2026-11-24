# calib-targets 0.18.0

More chessboard corners on dense, strongly distorted boards with no new wrong
labels, and two precision fixes: a labelled grid is always right-handed (a
strongly sheared view can no longer come out mirrored), and a ChArUco fragment
joins the board only when its corners agree with the board's geometry. Both
reach the native library unchanged in shape. The Rust crates add an opt-in
blur-aware photometric corner refiner and a physical defocus model (not yet in
the C API); see the
[workspace changelog](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/CHANGELOG.md#0180).

## Native C API archives

Prebuilt `calib-targets-ffi` archives for Linux, macOS and Windows: the shared
library, the generated `calib_targets_ffi.h`, the header-only C++ wrapper
`calib_targets_ffi.hpp`, and a CMake package you can point `find_package` at.

Consumption walkthrough:
[`docs/ffi/cmake-consumer-quickstart.md`](../ffi/cmake-consumer-quickstart.md).

The C ABI carries **its own version**, independent of the Rust workspace,
because it evolves on a different cadence. The archive filename is the ABI
version, not the workspace version.

## ABI 6.0.4

Workspace 0.18.0 ships the 6.0.4 native ABI, with no type, layout or symbol
change since 6.0.0: a caller built against 6.0.0–6.0.3 links and runs
unchanged. Detection results change as described above, and a tightly packed
image (`stride == width`) is no longer copied before detection. A caller
built against 5.x or earlier must relink and rebuild every caller-allocated
array, because several structs differ in size. Older ABI history is in
[`CHANGELOG.md`](../../CHANGELOG.md) and the
[0.16 migration guide](../migrations/0.16.0.md).

## Verifying an archive

Every archive is built and smoke-tested in CI by the native consumer, CMake
consumer, and release-archive tests before upload. `ct_version_string()`
returns the ABI version the library was built as.
