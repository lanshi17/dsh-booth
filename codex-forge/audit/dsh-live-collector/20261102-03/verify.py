#!/usr/bin/env python3
import hashlib
import json
import os
import re
import subprocess
import sys
from source_state import BASE, SOURCE, check, verified_state


def run(name, cmd, expected, out):
    r = subprocess.run(cmd, cwd=SOURCE, capture_output=True, text=True,
                       env=dict(os.environ, CARGO_TARGET_DIR=str(BASE / "target")), timeout=240)
    (out / (name + ".stdout.txt")).write_text(r.stdout)
    (out / (name + ".stderr.txt")).write_text(r.stderr)
    if r.returncode != expected:
        raise RuntimeError(f"{name}: {r.returncode} != {expected}")
    return {"name": name, "exit": r.returncode, "expected": expected}


def main():
    state = verified_state()
    # Exercise the source guard with a known-bad digest; report no pass if it accepts it.
    bad = dict(state, tree_sha256="0" * 64)
    try:
        check(bad, state)
    except RuntimeError:
        pass
    else:
        raise RuntimeError("source guard accepted negative control")
    out = BASE / (sys.argv[1] if len(sys.argv) > 1 else "output")
    out.mkdir(parents=True, exist_ok=False)
    layers = []
    layers.append(run("tests", ["cargo", "test", "--all-targets", "--locked", "--offline", "-j", "2", "--", "--test-threads=4"], 0, out))
    counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", (out / "tests.stdout.txt").read_text())
    if not counts or any(int(failed) for _, failed in counts):
        raise RuntimeError("missing or failing test summaries")
    layers.append(run("clippy", ["cargo", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"], 0, out))
    layers.append(run("fmt", ["cargo", "fmt", "--all", "--check"], 0, out))
    layers.append(run("readiness", ["cargo", "run", "--locked", "--offline", "--", "check-ready", "datasets/pilot.json"], 1, out))
    r = subprocess.run([sys.executable, str(BASE / "mutations.py"), str(out / "mutations")],
                       cwd=BASE, capture_output=True, text=True, timeout=420)
    (out / "mutations.stdout.txt").write_text(r.stdout)
    (out / "mutations.stderr.txt").write_text(r.stderr)
    if r.returncode:
        raise RuntimeError("mutation layer failed")
    mutants = json.loads((out / "mutations/results.json").read_text())
    if (len(mutants) != 3 or len({m["id"] for m in mutants}) != 3
            or any(m["outcome"] != "KILLED" or m["exit_code"] != 101 for m in mutants)):
        raise RuntimeError("mutation results incomplete")
    if verified_state() != state:
        raise RuntimeError("source changed during validation")
    versions = {}
    for tool in (["rustc", "--version"], ["cargo", "--version"], [sys.executable, "--version"]):
        p = subprocess.run(tool, capture_output=True, text=True, timeout=10, check=True)
        versions[tool[0].split("/")[-1]] = p.stdout.strip()
    scripts = {name: hashlib.sha256((BASE / name).read_bytes()).hexdigest()
               for name in ("verify.py", "mutations.py", "source_state.py")}
    result = {"layers": layers, "tests_passed": sum(int(n) for n, _ in counts),
              "source_tree_sha256": state["tree_sha256"], "source_guard_negative_control": "rejected",
              "checker_sha256": scripts, "tool_versions": versions,
              "manual_mutants_killed": len(mutants), "manual_mutants_total": 3,
              "formal_pilot": "NOT_RUN", "full_tier3_assurance": False}
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print("DSH_LIVE_VALIDATION_PASSED")


if __name__ == "__main__":
    main()
