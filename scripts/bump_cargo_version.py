#!/usr/bin/env python3
"""Update Cargo.toml [package] version."""

from __future__ import annotations

import re
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: python3 scripts/bump_cargo_version.py X.Y.Z", file=sys.stderr)
        return 1

    version = sys.argv[1]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        print("Version must be semantic X.Y.Z", file=sys.stderr)
        return 1

    cargo_path = Path("Cargo.toml")
    lines = cargo_path.read_text(encoding="utf-8").splitlines()

    in_package = False
    updated = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_package = stripped == "[package]"
            continue
        if in_package and stripped.startswith("version = "):
            lines[i] = f'version = "{version}"'
            updated = True
            break

    if not updated:
        print("Could not locate [package] version in Cargo.toml", file=sys.stderr)
        return 1

    cargo_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Updated Cargo.toml to version {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
