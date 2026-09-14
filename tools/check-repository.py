#!/usr/bin/env python3
"""Fast repository contracts that complement compiler and integration tests."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
failures: list[str] = []

for path in ROOT.glob("crates/**/*.rs"):
    text = path.read_text()
    for match in re.finditer(r"(?<!assert2::)(?<!debug_)\bassert(?:_eq|_ne)?!\(", text):
        failures.append(f"{path.relative_to(ROOT)}:{text.count(chr(10), 0, match.start()) + 1}: use the shared assert2 wrapper")

for path in ROOT.glob(".github/workflows/*.yml"):
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if re.search(r"\buses:\s+[^\s]+@(?![0-9a-f]{40}(?:\s|$))", line):
            failures.append(f"{path.relative_to(ROOT)}:{number}: action is not pinned to a commit")

link = re.compile(r"(?<!!)\[[^]]+\]\(([^)]+)\)")
for path in ROOT.rglob("*.md"):
    if ".git" in path.parts or "target" in path.parts:
        continue
    prose = re.sub(r"```.*?```", "", path.read_text(), flags=re.DOTALL)
    for target in link.findall(prose):
        target = target.split("#", 1)[0].strip("<>")
        if not target or "://" in target or target.startswith(("mailto:", "#")):
            continue
        if not (path.parent / target).resolve().exists():
            failures.append(f"{path.relative_to(ROOT)}: broken link: {target}")

if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)
print("repository contracts passed")
