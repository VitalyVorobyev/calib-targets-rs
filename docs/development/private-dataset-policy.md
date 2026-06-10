# Private dataset disclosure policy

**Never cite any private regression dataset — or its concrete counts, filenames,
hashes, or per-frame identifiers — in public-facing documentation.** Public
surfaces include: every crate `README.md`, every file under `book/src/`, the
top-level `README.md`, `CHANGELOG.md` entries for tagged releases, rustdoc /
public docstring comments, Python-package README and docstrings, commit messages
on `main`, and PR descriptions. Use **general performance statements only** in
those surfaces (e.g. "high detection rate on our internal regression set with
zero wrong labels", "precision-by-construction on a private dataset of
real-world snaps") — never raw counts, filenames, dataset hashes, or per-frame
identifiers.

Concrete numbers are fine in local-only surfaces: `privatedata/`,
`bench_results/`, the agent memory, the gitignored `docs/datasets/` tree, and PR
review discussion that is not checked in.

**Why.** The datasets belong to private engagements; leaking their size or
failure breakdown into published crates, the book, or GitHub undermines the
confidentiality agreement and freezes a specific number into a surface we can't
update without a release.

**How to apply.** Before editing any file outside `privatedata/`,
`bench_results/`, the gitignored `docs/datasets/` tree, or the agent memory,
grep the change for any dataset hash, `snap`, `t…s…` frame identifier, or
`target_*.png` specifics; if any appear, rewrite to a general performance
statement. Existing leaks in READMEs and `book/src/` are pre-existing — clean
them up opportunistically when editing those files, but do not ship new ones.

## Regression dataset: 3536119669 (chessboard)

Canonical seed-and-grow precision-and-recall benchmark. Precision contract:
wrong `(i, j)` labels are unrecoverable (they would corrupt calibration);
missing corners are acceptable. Any algorithmic change that drops this contract
is a regression, full stop.

Dataset layout, baseline numbers, known failure modes, and harness commands live
in `docs/datasets/3536119669.md` (gitignored, local-only — fresh clones will not
have it).

## Regression dataset: 130x130_puzzle (puzzleboard)

Real-world PuzzleBoard regression set (sibling to `3536119669`). Precision
contract: wrong master-(i, j) labels are unrecoverable; missing corners are
acceptable. Any change that raises max BER above the current baseline, or
introduces a failure variant other than `edge_sampling / NotEnoughEdges`, is a
regression.

**Decoder-algorithm decision (2026-04-20).** Do **not** pre-emptively rewrite
the puzzleboard decoder from its current naive form (per-edge hard-bit + 501²×D4
exhaustive origin sweep + hard BER gate) to a ChArUco-style coherent-hypothesis
matcher (soft bits, joint likelihood, best-vs-runner-up margin). The naive
decoder already clears the precision/recall target on this dataset with
effectively zero wrong labels; revisit only if a new dataset demonstrates a
concrete gap. See `memory/feedback_puzzleboard_decoder_is_good_enough.md`.

Dataset layout, baseline numbers, preprocessing requirements, and harness
commands live in `docs/datasets/130x130_puzzle.md` (gitignored, local-only —
fresh clones will not have it).
