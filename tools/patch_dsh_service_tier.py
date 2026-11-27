#!/usr/bin/env python3
"""Patch the installed dsh pi-ai adapter to forward samplingParams.

dsh 0.2.1 has no service-tier concept.  The booth control plugin sets
`samplingParams: { service_tier: ... }` on the agent request; this patch makes
the installed pi-ai adapter pass that object to pi-ai's provider adapters.
The patch is idempotent.
"""

from __future__ import annotations

import os
import pathlib
import sys


def candidate_roots() -> list[pathlib.Path]:
    roots: list[pathlib.Path] = []
    for key in ("BUN_INSTALL", "NPM_CONFIG_PREFIX", "HOME"):
        value = os.environ.get(key)
        if value:
            roots.append(pathlib.Path(value))
    return roots


def find_adapter() -> pathlib.Path | None:
    names = ("dsh-llm-pi-ai",)
    for root in candidate_roots():
        for base in (
            root / "install" / "global" / "node_modules" / "@deepseek-ai",
            root / "node_modules" / "@deepseek-ai",
            root / ".bun" / "install" / "global" / "node_modules" / "@deepseek-ai",
        ):
            for name in names:
                path = base / name / "lib" / "index.js"
                if path.is_file():
                    return path
    return None


def main() -> int:
    path = find_adapter()
    if path is None:
        print("dsh-llm-pi-ai is not installed; service tier patch skipped", file=sys.stderr)
        return 0
    text = path.read_text(encoding="utf-8")
    marker = "...options.samplingParams === void 0 ? {} : { samplingParams: options.samplingParams },"
    if marker in text:
        print(f"service tier patch already present: {path}")
        return 0
    needle = "...options.sessionId === void 0 ? {} : { sessionId: String(options.sessionId) },"
    if needle not in text:
        print(f"cannot find pi-ai stream options in {path}", file=sys.stderr)
        return 1
    text = text.replace(needle, needle + "\n\t\t\t\t\t" + marker, 1)
    path.write_text(text, encoding="utf-8")
    print(f"patched service tier support: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
