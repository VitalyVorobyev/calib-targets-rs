# AGENTS.md

Agent instructions for this repository live in a single file:
**[`.claude/CLAUDE.md`](.claude/CLAUDE.md)**. Read it before making any change —
it carries the everyday gate (fmt / clippy / test / doc with zero warnings), the
crate map and dependency rules, the coordinate and corner-orientation
conventions, the marker-decoding rules, the precision contract (a miss is
acceptable, a wrong `(i, j)` label never is), and links to every developer
guide (map: [`docs/README.md`](docs/README.md)).

Nothing here overrides it; this file exists so tools that look for `AGENTS.md`
find the same rules.
