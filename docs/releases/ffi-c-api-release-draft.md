# calib-targets 0.16.3

A missing or undetected corner no longer costs a whole patch of the board: the
square grid walk now refuses the cell that spans a hole instead of discarding
the connected component around it, so dense, partly occluded boards keep
corners they lost before, with no wrong labels. The build also no longer
compiles C or assembly: `projective-grid` 0.14.5 answers its neighbour queries
from its own kd-tree instead of `kiddo`. No API change. See the
[workspace changelog](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/CHANGELOG.md#0163).

## Native C API archives

Prebuilt `calib-targets-ffi` archives for Linux, macOS and Windows: the shared
library, the generated `calib_targets_ffi.h`, the header-only C++ wrapper
`calib_targets_ffi.hpp`, and a CMake package you can point `find_package` at.

Consumption walkthrough:
[`docs/ffi/cmake-consumer-quickstart.md`](../ffi/cmake-consumer-quickstart.md).

The C ABI carries **its own version**, independent of the Rust workspace,
because it evolves on a different cadence. The archive filename is the ABI
version, not the workspace version.

## ABI 6.0.2

Workspace 0.16.3 ships the 6.0.2 native ABI. It has no type or layout change
relative to 6.0.0, so a caller built against 6.0.0 or 6.0.1 links and runs
unchanged. A caller built against 5.x or earlier must relink and rebuild every
caller-allocated array, because several structs differ in size. Older ABI
history is in [`CHANGELOG.md`](../../CHANGELOG.md) and the
[0.16 migration guide](../migrations/0.16.0.md).

## Verifying an archive

Every archive is built and smoke-tested in CI by the native consumer, CMake
consumer, and release-archive tests before upload. `ct_version_string()`
returns the ABI version the library was built as.
