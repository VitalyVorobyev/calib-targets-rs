#!/usr/bin/env python3
"""Extract one projective-grid changelog section for GitHub release notes."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CHANGELOG = ROOT / "crates" / "projective-grid" / "CHANGELOG.md"


def release_notes(changelog: str, version: str) -> str:
    """Return the body of the exact ``## [version]`` section."""
    heading = re.compile(rf"^## \[{re.escape(version)}\](?:\s+-\s+.+)?\s*$", re.MULTILINE)
    match = heading.search(changelog)
    if match is None:
        raise ValueError(f"missing changelog section for {version}")

    body_start = match.end()
    section_end = re.search(
        r"^(?:## |\[Unreleased\]:)", changelog[body_start:], re.MULTILINE
    )
    body_end = body_start + section_end.start() if section_end else len(changelog)
    body = changelog[body_start:body_end].strip()
    if not body:
        raise ValueError(f"empty changelog section for {version}")
    return body + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", help="crate version without the tag prefix")
    parser.add_argument("--changelog", type=Path, default=DEFAULT_CHANGELOG)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    notes = release_notes(args.changelog.read_text(encoding="utf-8"), args.version)
    if args.output is None:
        print(notes, end="")
    else:
        args.output.write_text(notes, encoding="utf-8")


if __name__ == "__main__":
    main()
