#!/usr/bin/env python3
"""Summarize a cargo-mutants JSON report; exit nonzero on any surviving mutant.

A survivor means some behaviour in the changed files has no test that notices it changing.
`tools/mutants.sh` calls this, and `tools/gauntlet.sh` turns a nonzero exit into a failed layer.
Equivalent mutants are not auto-classified: the reviewer lists them in EVIDENCE with a reason,
so nothing is silently excused here.
"""
import json
import sys


def load(path: str) -> dict:
    try:
        with open(path) as handle:
            return json.load(handle)
    except (OSError, ValueError) as error:
        print(f"cannot read {path}: {error}")
        sys.exit(1)


def main() -> int:
    report = load(sys.argv[1] if len(sys.argv) > 1 else "target/mutants.json")
    counts = report.get("summary") or {}
    total = counts.get("total_mutants", counts.get("total", 0))
    caught = counts.get("caught", counts.get("killed", 0))
    missed = counts.get("missed", counts.get("survived", 0))
    timeout = counts.get("timeout", 0)
    unviable = counts.get("unviable", 0)

    print(f"mutants: {caught} caught, {missed} missed, {timeout} timeout, {unviable} unviable, {total} total")

    survivors = [m for m in report.get("mutants", []) if m.get("summary") == "Missed"]
    for mutant in survivors:
        print(f"  MISSED {mutant.get('file')}:{mutant.get('line')} {mutant.get('function')} — {mutant.get('replacement')}")

    # Unfinished runs are not evidence of a clean layer.
    for mutant in report.get("mutants", []):
        if mutant.get("summary") in ("Timeout", "Failure"):
            print(f"  UNFINISHED {mutant.get('file')}:{mutant.get('line')} {mutant.get('summary')}")

    unfinished = sum(1 for m in report.get("mutants", []) if m.get("summary") in ("Timeout", "Failure"))
    if missed == 0 and unfinished == 0:
        print("every mutant was caught")
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
