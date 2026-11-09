#!/usr/bin/env python3
"""Explain every changed line no test runs, by naming the function it belongs to.

The coverage ledger is line-number sensitive: an edit above a miss moves it, and hand-editing
`sigh` numbers is how a ledger quietly stops meaning anything. This tool derives the entry from
the code instead — the enclosing function, plus a reason kept in this file — so a miss is either
explained by a known function or reported as needing a reason.

    python3 tools/ledger.py            # rewrite tools/coverage_excludes.txt
    python3 tools/ledger.py --check    # print what would change, exit nonzero if unknown

A function missing from REASONS is an error on purpose: the reader can then tell an explained
exclusion from an unexamined one.
"""
import re
import subprocess
import sys

LEDGER = "tools/coverage_excludes.txt"

# Why a line in this function cannot run in a unit test. Every reason is a claim about the code,
# not about coverage. Keys are `<file>::<fn>`: the same name in another module is a different
# function, and a bare name would let a miss there inherit a reason that was never about it.
REASONS = {
    # Terminal frames: only a pty renders them, and the pty run is recorded in EVIDENCE.
    "src/app.rs::draw": (
        "the whole frame is drawn here; only a pty observes it, and the pty run is in EVIDENCE"
    ),
    "src/app.rs::draw_modal": "modal dispatch into a frame: pty only",
    "src/app.rs::draw_model": "drawn into a frame: pty only",
    "src/app.rs::draw_composer": "drawn into a frame: pty only",
    "src/app.rs::draw_keys": "drawn into a frame: pty only",
    "src/app.rs::draw_history": "drawn into a frame: pty only",
    "src/app.rs::draw_oauth": "drawn into a frame: pty only",
    "src/app.rs::draw_login": "drawn into a frame: pty only",
    "src/app.rs::draw_login_list": "drawn into a frame: pty only",
    "src/app.rs::draw_working": "drawn into a frame: pty only",
    "src/app.rs::draw_splash": "drawn into a frame: pty only",
    # A poll thread posts its result; the same call is covered directly.
    "src/app.rs::refresh_status": "runs on the poll thread; the call it makes is covered directly",
    # Branches that need a real process or service.
    "src/service.rs::start": "spawns the service, so a unit test cannot reach it",
    "src/main.rs::show_provider": (
        "its only effect is printing; the lines it prints are tested through provider_lines"
    ),
    # `run` is also the console's event loop in `src/app.rs`: the same name, a different
    # function with a different reason, so the key names the file.
    "src/main.rs::run": "the CLI dispatcher, reached only through `main`",
    # The exchange answers only over the network.
    "src/oauth.rs::exchange": "asks platform.deepseek.com; a unit test must not open that socket",
    # Assertions that only run when they fail.
    "src/app.rs::the_model_picker_opens_on_the_current_choice": "assertion failure message",
    "src/app.rs::the_picker_moves_both_ways_and_stops_at_the_ends": "assertion failure message",
    "src/app.rs::the_login_form_edits_one_field_at_a_time": "assertion failure message",
    "src/app.rs::an_incomplete_login_form_stays_open_with_a_reason": "assertion failure message",
    "src/app.rs::provider_and_model_read_from_the_service": "assertion failure message",
    "src/app.rs::an_api_key_login_writes_the_profile_and_a_credential_ref": "assertion failure message",
    "src/service.rs::an_unreadable_layer_is_reported_rather_than_ignored": "assertion failure message",
    # The sign-in path restarts the service, which a unit test must not do.
    "src/app.rs::apply": "the OAuth success branch restarts the service, which a unit test must not do",
    "src/provider.rs::migrate": (
        "the `?` that propagates a failed add_endpoint; the success path is covered"
    ),
}
# A closing brace reports a region the compiler attributes to its neighbour; the statement above
# it is covered, which is what the entry would claim anyway.
BRACE_REASON = "a closing brace llvm-cov attributes to a later region; the statement above is covered"


def misses() -> list[tuple[str, int]]:
    """The coverage tool's unexplained misses, as (file, line)."""
    run = subprocess.run(
        ["python3", "tools/changed_lines.py"], capture_output=True, text=True, check=False
    )
    # The summary line means the measurement ran, even when it exits 1 with misses or a stale
    # entry — exactly the case this tool rewrites. Without it the run never measured anything,
    # and rewriting the ledger from that would delete the reviewed entries.
    measured = any(
        line.startswith("changed executable lines:") for line in run.stdout.splitlines()
    )
    if run.returncode not in (0, 1) or not measured:
        detail = run.stdout.strip()
        if not detail and run.stderr.strip():
            detail = run.stderr.strip().splitlines()[-1]
        print(f"cannot measure the misses: {detail or f'changed_lines.py exited {run.returncode}'}")
        sys.exit(1)
    found = []
    for line in run.stdout.splitlines():
        # Both forms name the same location: a miss the ledger explains ("excluded") and one it
        # does not ("UNCOVERED"). Regenerating must see both, or the explained entries vanish.
        match = re.match(r"\s+(?:UNCOVERED|excluded)\s+(\S+):(\d+)", line)
        if match:
            found.append((match.group(1), int(match.group(2))))
    return found


def enclosing_function(path: str, line: int) -> str:
    """The innermost `fn` whose body contains `line`, by indentation."""
    with open(path) as handle:
        lines = handle.read().splitlines()
    for index in range(min(line, len(lines)) - 1, -1, -1):
        match = re.match(r"\s*(?:pub )?(?:async )?fn (\w+)", lines[index])
        if match:
            return match.group(1)
    return ""


def explain(path: str, line: int) -> str | None:
    with open(path) as handle:
        text = handle.read().splitlines()
    body = text[line - 1].strip() if 0 < line <= len(text) else ""
    if body in ("}", "};"):
        return BRACE_REASON
    if "panic!" in body or "_ => panic" in body:
        return "assertion failure message: evaluated only when that assertion fails"
    name = enclosing_function(path, line)
    # A bare name would explain a miss in another module's function that happens to share it;
    # the reason is a claim about this one file's function.
    return REASONS.get(f"{path}::{name}")


def main() -> int:
    check = "--check" in sys.argv
    unknown = 0
    entries = []
    for path, line in misses():
        reason = explain(path, line)
        if reason is None:
            unknown += 1
            print(f"needs a reason: {path}:{line} in {path}::{enclosing_function(path, line)}()")
            continue
        entries.append(f"{path}:{line} {reason}")

    if check:
        print(f"{len(entries)} explained, {unknown} needing a reason")
        return 1 if unknown else 0

    header = (
        "# Changed lines that cannot run in a test, with the reason. One `<file>:<line> <reason>`\n"
        "# per line. `tools/changed_lines.py` fails on any miss that is NOT listed here, and fails\n"
        "# again when a listed line stops being a miss, so this file cannot rot into a blanket\n"
        "# exemption.\n"
        "#\n"
        "# Generated by `tools/ledger.py` from the misses and the reason table in that script; the\n"
        "# reasons are claims about the code, reviewed when a function is added.\n"
        "\n"
    )
    with open(LEDGER, "w") as handle:
        handle.write(header + "\n".join(entries) + "\n")
    print(f"wrote {LEDGER}: {len(entries)} entries, {unknown} needing a reason")
    return 1 if unknown else 0


if __name__ == "__main__":
    sys.exit(main())