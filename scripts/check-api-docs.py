#!/usr/bin/env python3
"""Check local Markdown links in the public API documentation."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
FILES = [
    ROOT / "README.md",
    *sorted((ROOT / "docs").glob("api*.md")),
    ROOT / "docs" / "protobuf-api.md",
    ROOT / "examples" / "api" / "README.md",
]


def main() -> int:
    failures: list[str] = []
    checked = 0
    for source in FILES:
        if not source.exists():
            failures.append(f"missing required documentation file: {source.relative_to(ROOT)}")
            continue
        for match in re.finditer(r"(?<!!)\[[^]]+\]\(([^)]+)\)", source.read_text()):
            target = match.group(1).strip().split("#", 1)[0]
            if not target or "://" in target or target.startswith("mailto:"):
                continue
            target = unquote(target).split(" ", 1)[0]
            checked += 1
            if not (source.parent / target).resolve().exists():
                line = source.read_text()[: match.start()].count("\n") + 1
                failures.append(
                    f"{source.relative_to(ROOT)}:{line}: missing link target {target}"
                )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"checked {checked} local API documentation links")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
