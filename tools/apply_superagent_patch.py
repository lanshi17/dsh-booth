#!/usr/bin/env python3
"""Apply the booth-owned session recovery integration to the pinned superagent source tree."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PATCH = ROOT / "patches" / "dsh-superagent-session-recovery.patch"


def check(bundle: Path, reverse: bool = False) -> subprocess.CompletedProcess[str]:
    args = ["git", "-C", str(bundle), "apply", "--check"]
    if reverse:
        args.append("--reverse")
    args.append(str(PATCH))
    return subprocess.run(args, text=True, capture_output=True, check=False)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: apply_superagent_patch.py SUPERAGENT_DIR", file=sys.stderr)
        return 2
    bundle = Path(sys.argv[1]).resolve()
    if not PATCH.is_file() or not (bundle / "src/app/sessions.ts").is_file():
        print(f"cannot apply the booth integration patch to {bundle}: patch or superagent source is missing", file=sys.stderr)
        return 2

    forward = check(bundle)
    if forward.returncode == 0:
        applied = subprocess.run(
            ["git", "-C", str(bundle), "apply", str(PATCH)],
            text=True,
            capture_output=True,
            check=False,
        )
        if applied.returncode == 0:
            print("applied booth's missing-session recovery to dsh-superagent")
            return 0
        print(applied.stderr, file=sys.stderr, end="")
        return applied.returncode

    reverse = check(bundle, reverse=True)
    if reverse.returncode == 0:
        print("dsh-superagent already has booth's missing-session recovery")
        return 0

    print(f"cannot apply the booth integration patch to {bundle}", file=sys.stderr)
    print(forward.stderr.strip(), file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
