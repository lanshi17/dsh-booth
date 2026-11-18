#!/usr/bin/env python3
"""Apply booth-owned integrations to the pinned dsh-superagent source tree."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PATCHES = [
    ROOT / "patches" / "dsh-superagent-session-recovery.patch",
    ROOT / "patches" / "dsh-superagent-goal-delegation.patch",
]


def check(bundle: Path, patch: Path, reverse: bool = False) -> subprocess.CompletedProcess[str]:
    args = ["git", "-C", str(bundle), "apply", "--unidiff-zero", "--check"]
    if reverse:
        args.append("--reverse")
    args.append(str(patch))
    return subprocess.run(args, text=True, capture_output=True, check=False)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: apply_superagent_patch.py SUPERAGENT_DIR", file=sys.stderr)
        return 2
    bundle = Path(sys.argv[1]).resolve()
    if not all(patch.is_file() for patch in PATCHES) or not (bundle / "src/app/sessions.ts").is_file():
        print(f"cannot apply the booth integration patch to {bundle}: patch or superagent source is missing", file=sys.stderr)
        return 2

    for patch in PATCHES:
        forward = check(bundle, patch)
        if forward.returncode == 0:
            applied = subprocess.run(["git", "-C", str(bundle), "apply", "--unidiff-zero", str(patch)], text=True, capture_output=True, check=False)
            if applied.returncode != 0:
                print(applied.stderr, file=sys.stderr, end="")
                return applied.returncode
            print(f"applied booth integration patch {patch.name}")
            continue
        reverse = check(bundle, patch, reverse=True)
        if reverse.returncode == 0:
            print(f"dsh-superagent already has {patch.name}")
            continue
        print(f"cannot apply the booth integration patch {patch} to {bundle}", file=sys.stderr)
        print(forward.stderr.strip(), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
