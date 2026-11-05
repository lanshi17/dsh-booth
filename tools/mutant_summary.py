#!/usr/bin/env python3
"""Summarize a cargo-mutants run; exit nonzero on any survivor or unfinished mutant.

cargo-mutants 27 writes `outcomes.json` in its output directory:
`{"total_mutants": n, "caught": n, "missed": n, "timeout": n, "unviable": n, "outcomes": [...]}`.
A survivor means some behaviour in the changed code has no test that notices it changing.
`tools/mutants.sh` calls this, and `tools/gauntlet.sh` turns a nonzero exit into a failed layer.

Equivalent mutants are not auto-classified here: the reviewer lists them in EVIDENCE with a
reason, so nothing is silently excused.
"""
import json
import sys


def load(path: str) -> dict:
    try:
        with open(path) as handle:
            report = json.load(handle)
    except (OSError, ValueError) as error:
        print(f"cannot read {path}: {error}")
        sys.exit(1)
    # Older runs wrote a plain list of mutants; without outcomes it cannot be summarized.
    if not isinstance(report, dict) or "outcomes" not in report:
        print(f"{path} is not a cargo-mutants outcomes report")
        sys.exit(1)
    return report


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "target/mutants/mutants.out/outcomes.json"
    report = load(path)
    caught = report.get("caught", 0)
    missed = report.get("missed", 0)
    timed_out = report.get("timeout", 0)
    unviable = report.get("unviable", 0)
    total = report.get("total_mutants", 0)
    print(f"mutants: {caught} caught, {missed} missed, {timed_out} timeout, "
          f"{unviable} unviable, {total} total")

    unfinished = 0
    for outcome in report["outcomes"]:
        summary = outcome.get("summary")
        if summary == "Missed":
            print(f"  MISSED      {outcome.get('scenario')}")
        elif summary in ("Timeout", "Failure"):
            unfinished += 1
            print(f"  {summary.upper():<11} {outcome.get('scenario')}")

    # An unfinished run is not evidence of a clean layer.
    if missed == 0 and timed_out == 0 and unfinished == 0:
        print("every mutant was caught")
        return 0
    print(f"{missed} survivor(s), {timed_out} timed out, {unfinished} unfinished")
    return 1


if __name__ == "__main__":
    sys.exit(main())
