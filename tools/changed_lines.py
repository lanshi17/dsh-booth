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
EXCLUDES = "tools/coverage_excludes.txt"

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


def executable_and_covered(report_path: str) -> tuple[set, set]:
    """Line -> ran in a test, keyed by the repo-relative path.

    The report names files absolutely while a diff names them relatively, so the suffix is what
    both can agree on. A segment is `[line, col, count, has_count, is_region_entry, is_gap]`;
    only a region entry is executable code, and a gap region on the same line is not.
    """
    with open(report_path) as handle:
        report = json.load(handle)
    executable: set[tuple[str, int]] = set()
    covered: set[tuple[str, int]] = set()
    for export in report.get("data", []):
        for entry in export.get("files", []):
            name = entry.get("filename", "")
            for segment in entry["segments"]:
                line, count = segment[0], segment[2]
                # A segment is a region of code; `is_region_entry` marks its start, and a gap
                # region is the space between code, not code itself.
                if len(segment) > 5 and segment[5]:
                    continue
                if len(segment) > 4 and not segment[4]:
                    continue
                executable.add((name, line))
                if count:
                    covered.add((name, line))
    return executable, covered


def relative(name: str) -> str:
    """`/repo/src/intake.rs` -> `src/intake.rs`, so a report path matches a diff path."""
    marker = "/src/"
    return "src/" + name.split(marker, 1)[1] if marker in name else name


def excludes() -> dict[str, str]:
    """Listed misses and their reasons, read from the exclusion file."""
    listed: dict[str, str] = {}
    try:
        with open(EXCLUDES) as handle:
            for raw in handle:
                line = raw.strip()
                if not line or line.startswith("#"):
                    continue
                key, _, reason = line.partition(" ")
                listed[key] = reason.strip() or "(no reason given)"
    except OSError as error:
        print(f"cannot read {EXCLUDES}: {error}")
        sys.exit(1)
    return listed


def main() -> int:
    base = sys.argv[1] if len(sys.argv) > 1 else "master"
    # Compare against where this branch left the trunk, so a long branch does not count the
    # trunk's own commits as its changes.
    merge_base = subprocess.run(
        ["git", "merge-base", "HEAD", base], capture_output=True, text=True, check=False
    ).stdout.strip()
    if merge_base:
        base = merge_base
    report = sys.argv[2] if len(sys.argv) > 2 else "target/cov.json"
    changed = changed_lines(base)
    if not changed:
        print(f"no changed source lines against {base}")
        return 0
    executable, covered = executable_and_covered(report)
    executable = {(relative(path), line) for path, line in executable}
    covered = {(relative(path), line) for path, line in covered}

    total = hit = plain = 0
    misses: list[str] = []
    for name, lines in sorted(changed.items()):
        for line in sorted(lines):
            key = (name, line)
            # A changed line with no executable region — a comment, a blank line, a struct
            # field, a closing brace — cannot be covered and is not a miss.
            if key not in executable:
                plain += 1
                continue
            total += 1
            if key in covered:
                hit += 1
            else:
                misses.append(f"{name}:{line}")
    listed = excludes()
    unexplained = [miss for miss in misses if miss not in listed]
    stale = [key for key in listed if key not in misses]

    print(f"changed executable lines: {hit}/{total} covered "
          f"({plain} changed lines carry no executable code, {len(listed)} listed exclusions)")
    for miss in misses:
        if miss in listed:
            print(f"  excluded  {miss} — {listed[miss]}")
        else:
            print(f"  UNCOVERED {miss}")
    for key in stale:
        print(f"  STALE EXCLUSION {key} — this line is covered or gone; remove it from {EXCLUDES}")
    if unexplained or stale:
        print(f"{len(unexplained)} unexplained line(s), {len(stale)} stale exclusion(s)")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
