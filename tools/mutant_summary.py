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

EQUIVALENTS = "tools/mutant_equivalents.txt"


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


def equivalents() -> dict[str, str]:
    """Surviving mutants known to be equivalent, with the reason, from the listing file."""
    listed: dict[str, str] = {}
    try:
        with open(EQUIVALENTS) as handle:
            for raw in handle:
                line = raw.strip()
                if not line or line.startswith("#"):
                    continue
                # `<mutant name> — <reason>`: the em dash separates name from reason.
                name, _, reason = line.partition(" — ")
                listed[name.strip()] = reason.strip() or "(no reason given)"
    except OSError as error:
        print(f"cannot read {EQUIVALENTS}: {error}")
        sys.exit(1)
    return listed


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

    # cargo-mutants writes `end_time` when the lab finishes; an interrupted or aborted run
    # leaves it null, so the outcomes it managed to write are only part of the run.
    finished = bool(report.get("end_time"))
    if not finished:
        print(f"the run did not finish (no end_time in {path}); not a pass")

    listed = equivalents()
    unfinished = 0
    survivors: list[str] = []
    for outcome in report["outcomes"]:
        summary = outcome.get("summary")
        # cargo-mutants 27 uses `MissedMutant` / `CaughtMutant` / `Unviable`, and carries the
        # mutant under either a dict with a "Mutant" key or a plain name string.
        scenario = outcome.get("scenario")
        if isinstance(scenario, dict):
            name = (scenario.get("Mutant") or {}).get("name", str(scenario))
        else:
            name = str(scenario)
        if summary in ("Missed", "MissedMutant"):
            survivors.append(name)
            if name in listed:
                print(f"  equivalent  {name}")
                print(f"              {listed[name]}")
            else:
                print(f"  MISSED      {name}")
        elif summary in ("Timeout", "TimeoutMutant", "Failure", "Unfinished"):
            unfinished += 1
            print(f"  {summary.upper():<11} {name}")

    unexplained = [name for name in survivors if name not in listed]
    # Only a finished run shows that a listed equivalent stopped surviving; a run cut short
    # may merely not have reached it.
    stale = [name for name in listed if name not in survivors] if finished else []
    for name in stale:
        print(f"  STALE       {name} — no longer survives; remove it from {EQUIVALENTS}")

    # An interrupted run is not evidence of a clean layer, however clean its outcomes so far.
    if finished and not unexplained and not stale and timed_out == 0 and unfinished == 0:
        print(f"every mutant was caught or is a listed equivalent ({len(listed)} listed)")
        return 0
    print(f"{len(unexplained)} unexplained survivor(s), {timed_out} timed out, "
          f"{unfinished} unfinished, {len(stale)} stale")
    return 1


if __name__ == "__main__":
    sys.exit(main())
