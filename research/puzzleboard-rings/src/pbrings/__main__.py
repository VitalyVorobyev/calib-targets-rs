"""Entry point. Pins BLAS threading *before* NumPy is imported anywhere.

Not decoration: once campaigns run one process per core, each worker spinning up
its own 8-thread BLAS costs several times the throughput and presents itself as
"Python is slow". The environment has to be set before the first NumPy import in
the process, which is why this module exists separately from :mod:`pbrings.cli`
and why the console script points here.
"""

from __future__ import annotations

import os
import sys

for _var in (
    "OMP_NUM_THREADS",
    "OPENBLAS_NUM_THREADS",
    "MKL_NUM_THREADS",
    "VECLIB_MAXIMUM_THREADS",
    "NUMEXPR_NUM_THREADS",
):
    os.environ.setdefault(_var, "1")


def main(argv: list[str] | None = None) -> int:
    from .cli import run

    return run(argv if argv is not None else sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
