#!/usr/bin/env python3
"""Generate typed Python artifacts for calib-targets bindings.

Outputs:
- python/calib_targets/_generated_dictionary.py
- python/calib_targets/_core.pyi
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
RUST_LIB = ROOT / "src" / "lib.rs"
PY_PACKAGE = ROOT / "python" / "calib_targets"
ARUCO_DATA = ROOT.parent / "calib-targets-aruco" / "data"

DICTIONARY_OUTPUT = PY_PACKAGE / "_generated_dictionary.py"
CORE_STUB_OUTPUT = PY_PACKAGE / "_core.pyi"


def _signature_inner(sig: str) -> str:
    text = sig.strip()
    if not (text.startswith("(") and text.endswith(")")):
        raise ValueError(f"unexpected signature format: {sig!r}")
    return text[1:-1].strip()


def _class_init_signature(sig: str) -> str:
    inner = _signature_inner(sig)
    if not inner:
        return "self"
    return f"self, {inner}"


def _function_signature(sig: str) -> str:
    inner = _signature_inner(sig)
    if not inner:
        return "()"
    return f"({inner})"


def _extract_dictionary_names() -> list[str]:
    names: list[str] = []
    for path in sorted(ARUCO_DATA.glob("*_CODES.json")):
        names.append(path.name.removesuffix("_CODES.json"))
    if not names:
        raise RuntimeError(f"no dictionary files found in {ARUCO_DATA}")
    return names


def _extract_rust_core_surface() -> tuple[list[tuple[str, str]], list[tuple[str, str]]]:
    src = RUST_LIB.read_text(encoding="utf-8")

    class_names = re.findall(
        r'#\[pyclass\(name = "([A-Za-z0-9_]+)", module = "calib_targets\._core"\)\]',
        src,
    )
    ctor_sigs = re.findall(
        r"#\[new\]\s*#\[pyo3\(signature = (\([^\)]*\))\)\]",
        src,
        re.MULTILINE,
    )
    if len(class_names) != len(ctor_sigs):
        raise RuntimeError(
            "failed to map pyclass constructors: "
            f"{len(class_names)} classes vs {len(ctor_sigs)} constructor signatures"
        )

    classes = list(zip(class_names, ctor_sigs, strict=True))

    function_matches = re.findall(
        r"#\[pyfunction\]\s*#\[pyo3\(signature = (\([^\)]*\))\)\]\s*fn ([a-zA-Z0-9_]+)\(",
        src,
        re.MULTILINE,
    )
    if not function_matches:
        raise RuntimeError("no #[pyfunction] signatures found")

    functions = [(name, sig) for (sig, name) in function_matches]
    return classes, functions


def _render_dictionary_module(names: Sequence[str]) -> str:
    literal_items = ", ".join(f'"{name}"' for name in names)
    tuple_items = "\n".join(f'    "{name}",' for name in names)

    return (
        "from __future__ import annotations\n\n"
        "from typing import Final, Literal\n\n"
        "DICTIONARY_NAMES: Final[tuple[str, ...]] = (\n"
        f"{tuple_items}\n"
        ")\n\n"
        f"DictionaryName = Literal[{literal_items}]\n"
    )


def _render_core_stub(
    classes: Sequence[tuple[str, str]], functions: Sequence[tuple[str, str]]
) -> str:
    lines: list[str] = [
        "from __future__ import annotations",
        "",
        "from typing import Any",
        "",
    ]

    for class_name, ctor_sig in classes:
        init_sig = _class_init_signature(ctor_sig)
        lines.append(f"class {class_name}:")
        lines.append(f"    def __init__({init_sig}) -> None: ...")
        lines.append("")

    return_types = {
        "detect_charuco": "dict[str, Any]",
        "detect_chessboard": "dict[str, Any] | None",
        "detect_marker_board": "dict[str, Any] | None",
    }

    for func_name, func_sig in functions:
        sig = _function_signature(func_sig)
        ret = return_types.get(func_name, "Any")
        lines.append(f"def {func_name}{sig} -> {ret}: ...")

    lines.append("")
    return "\n".join(lines)


def _write_or_check(path: Path, content: str, check: bool) -> bool:
    existing = path.read_text(encoding="utf-8") if path.exists() else None
    if existing == content:
        return True
    if check:
        print(f"stale generated file: {path}")
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    print(f"wrote {path}")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if generated files are out-of-date",
    )
    args = parser.parse_args()

    dictionary_names = _extract_dictionary_names()
    classes, functions = _extract_rust_core_surface()

    ok = True
    ok &= _write_or_check(
        DICTIONARY_OUTPUT,
        _render_dictionary_module(dictionary_names),
        check=args.check,
    )
    ok &= _write_or_check(
        CORE_STUB_OUTPUT,
        _render_core_stub(classes, functions),
        check=args.check,
    )

    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
