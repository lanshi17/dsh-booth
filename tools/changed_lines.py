#!/usr/bin/env python3
"""Changed-line coverage: every line this branch adds or rewrites must run in a test.

Global coverage is vanity; the constraint is that the diff is exercised. Lines are read from
`git diff <base> --unified=0` and the coverage report from `cargo llvm-cov --json`.

Fails closed: a missing report, an unreadable diff, or a source file the report does not
mention is a failure, never a pass.
"""
import json
import re
import subprocess
import sys

# Source files whose changed lines this branch means to cover.
WATCHED = ("src/intake.rs", "src/service.rs", "src/app.rs", "src/commands.rs", "src/provider.rs",
           "src/oauth.rs", "src/gh.rs", "src/format.rs", "src/main.rs", "src/api.rs", "src/model.rs")


def changed_lines(base: str) -> dict[str, set[int]]:
    """Line numbers this branch added, per file, from a zero-context diff."""
    diff = subprocess.run(
        ["git", "diff", "--unified=0", base, "--", *WATCHED],
        capture_output=True, text=True, check=False,
    )
    if diff.returncode != 0:
        print(f"cannot read the diff against {base}: {diff.stderr.strip()}")
        sys.exit(1)
    changed: dict[str, set[int]] = {}
    current = None
    for line in diff.stdout.splitlines():
        if line.startswith("+++ b/"):
            current = line[6:]
            changed.setdefault(current, set())
        elif line.startswith("@@") and current:
            match = re.search(r"\+(\d+)(?:,(\d+))?", line)
            if not match:
                continue
            start, count = int(match.group(1)), int(match.group(2) or 1)
            changed[current].update(range(start, start + count))
    return {name: lines for name, lines in changed.items() if lines and name.endswith(".rs")}


def covered_lines(report_path: str) -> dict[tuple[str, int], bool]:
    with open(report_path) as handle:
        report = json.load(handle)
    covered: dict[tuple[str, int], bool] = {}
    for export in report.get("data", []):
        for entry in export.get("files", []):
            name = entry.get("filename", "")
            for segment in entry["segments"]:
                line, count = segment[0], segment[2]
                key = (name, line)
                covered[key] = covered.get(key, False) or bool(count)
    return covered


def main() -> int:
    base = sys.argv[1] if len(sys.argv) > 1 else "origin/master"
    report = sys.argv[2] if len(sys.argv) > 2 else "target/cov.json"
    changed = changed_lines(base)
    if not changed:
        print(f"no changed source lines against {base}")
        return 0
    covered = covered_lines(report)

    total = hit = 0
    misses: list[str] = []
    for name, lines in sorted(changed.items()):
        missing = [line for line in sorted(lines) if not covered.get((name, line), False)]
        hit += len(lines) - len(missing)
        total += len(lines)
        for line in missing:
            misses.append(f"{name}:{line}")
    print(f"changed lines: {hit}/{total} covered")
    for miss in misses:
        print(f"  uncovered {miss}")
    if miss_count := len(misses):
        print(f"{miss_count} changed line(s) no test runs")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
