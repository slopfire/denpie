#!/usr/bin/env python3
"""Return URLs from a DDGS text search without fetching any page or image bytes.

This is deliberately a very small sidecar.  The Rust caller owns URL validation
and downloading; this process only asks the optional ``ddgs`` package for text
results and serializes their links.
"""

from __future__ import annotations

import json
import sys
from collections.abc import Mapping
from urllib.parse import urlparse


MAX_RESULTS = 8


def _http_url(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    value = value.strip()
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return None
    return value


def _row_url(row: Mapping[object, object]) -> str | None:
    # ``href`` is the DDGS text-result field.  ``url``/``image`` keep this
    # compatible with DDGS backends that expose a direct image URL instead.
    for key in ("href", "url", "image"):
        url = _http_url(row.get(key))
        if url is not None:
            return url
    return None


def main() -> int:
    if len(sys.argv) != 2 or not sys.argv[1].strip():
        print("usage: ddgs-image-search.py QUERY", file=sys.stderr)
        return 2

    try:
        from ddgs import DDGS
    except ImportError:
        print("the optional ddgs package is not installed", file=sys.stderr)
        return 2

    try:
        with DDGS() as ddgs:
            rows = ddgs.text(sys.argv[1], max_results=MAX_RESULTS)
            urls: list[str] = []
            seen: set[str] = set()
            for row in rows:
                if not isinstance(row, Mapping):
                    continue
                url = _row_url(row)
                if url is None or url in seen:
                    continue
                seen.add(url)
                urls.append(url)
                if len(urls) == MAX_RESULTS:
                    break
    except Exception as exc:  # pragma: no cover - depends on optional backend
        print(f"ddgs text search failed: {exc}", file=sys.stderr)
        return 1

    print(json.dumps(urls, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
