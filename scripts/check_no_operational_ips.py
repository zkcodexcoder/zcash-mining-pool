#!/usr/bin/env python3
"""Reject operational IP literals in tracked repository text files."""

from __future__ import annotations

import ipaddress
from pathlib import Path
import re
import subprocess
import sys


IPV4 = re.compile(r"(?<![0-9.])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![0-9.])")
IPV6 = re.compile(
    r"(?<![0-9A-Za-z_:])(?:[0-9A-Fa-f]{0,4}:){2,7}"
    r"[0-9A-Fa-f]{0,4}(?![0-9A-Za-z_:])"
)


def tracked_files() -> list[Path]:
    output = subprocess.check_output(["git", "ls-files", "-z"])
    return [Path(name.decode()) for name in output.split(b"\0") if name]


def classify(raw: str) -> str | None:
    try:
        address = ipaddress.ip_address(raw)
    except ValueError:
        return None
    if address.is_loopback or address.is_unspecified:
        return None
    if address.is_global:
        return "public"
    if address.is_private:
        return "private"
    if address.is_link_local:
        return "link-local"
    return "non-local"


def main() -> int:
    violations: list[tuple[Path, int, str]] = []
    for path in tracked_files():
        try:
            lines = path.read_text().splitlines()
        except (OSError, UnicodeDecodeError):
            continue

        for line_number, line in enumerate(lines, 1):
            matches = list(IPV4.finditer(line)) + list(IPV6.finditer(line))
            for match in matches:
                kind = classify(match.group())
                if kind is not None:
                    violations.append((path, line_number, kind))

    if not violations:
        print("Repository IP privacy check passed")
        return 0

    print("Operational IP literals found:", file=sys.stderr)
    for path, line_number, kind in violations:
        print(f"  {path}:{line_number}: {kind} address", file=sys.stderr)
    print("Use a hostname or private, untracked deployment configuration.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
