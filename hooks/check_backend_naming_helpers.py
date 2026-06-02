#!/usr/bin/env python3
"""Reject backend-local generic casing and serde helper definitions."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MESSAGE = (
    "Backend-local generic casing/serde helpers are not allowed. Use "
    "`src/codegen/naming.rs`, or a context-specific backend wrapper that delegates to it."
)

BANNED_FUNCTIONS = {
    "apply_rename_all",
    "apply_serde_rename",
    "wire_variant_value",
    "variant_discriminator",
    "serde_variant_name",
    "unit_enum_raw_value",
    "variant_serde_name",
    "to_snake_case",
    "to_camel_case",
    "to_pascal_case",
    "pascal_case",
    "pascal_to_snake",
}

FUNCTION_PATTERN = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+"
    rf"({'|'.join(re.escape(name) for name in sorted(BANNED_FUNCTIONS))})\b"
)


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\x00" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def violations_for_file(path: Path) -> list[str]:
    normalized = path.as_posix()
    if not normalized.startswith("src/backends/") or path.suffix != ".rs":
        return []

    content = read_text(path)
    if content is None:
        return []

    violations: list[str] = []
    for line_number, line in enumerate(content.splitlines(), start=1):
        match = FUNCTION_PATTERN.search(line)
        if match:
            violations.append(f"{path}:{line_number}: backend-local helper `{match.group(1)}`")
    return violations


def main(argv: list[str] | None = None) -> int:
    paths = [Path(raw) for raw in (argv if argv is not None else sys.argv[1:])]
    if not paths:
        paths = list(Path("src/backends").rglob("*.rs"))

    violations: list[str] = []
    for path in paths:
        if path.is_file():
            violations.extend(violations_for_file(path))

    if violations:
        for violation in violations:
            print(violation, file=sys.stderr)
        print(f"\n{MESSAGE}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
