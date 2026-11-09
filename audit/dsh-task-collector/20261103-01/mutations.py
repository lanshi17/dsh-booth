#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
from source_state import BASE, SOURCE, verified_state

CASES = [
    ("round-reset", "if run.round != 1 {", "if false && run.round != 1 {",
     "later_rounds_and_existing_outputs_are_rejected"),
    ("artifact-guard", "if actual != record.files_sha256 {", "if false && actual != record.files_sha256 {",
     "tampered_missing_extra_and_symlink_artifacts_are_rejected"),
    ("shared-network", 'if network {\n        a.push("--share-net".into());',
     'if true {\n        a.push("--share-net".into());',
     "isolated_worker_edits_only_export_and_cannot_read_host_or_connect"),
]


def main():
    state = verified_state()
    out = BASE / (sys.argv[1] if len(sys.argv) > 1 else "mutation-output")
    out.mkdir(parents=True, exist_ok=False)
    results = []
    for label, before, after, test in CASES:
        d = out / "mutants" / label
        # Explicit allowlist prevents copying credentials, private evaluator or run output.
        for name, digest in state["files_sha256"].items():
            payload = (SOURCE / name).read_bytes()
            if hashlib.sha256(payload).hexdigest() != digest:
                raise RuntimeError("source changed during mutation setup")
            target = d / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(payload)
        p = d / "src/task_collect.rs"
        text = p.read_text()
        if text.count(before) != 1:
            raise RuntimeError("mutation target is not unique: " + label)
        text = text.replace(before, after, 1)
        p.write_text(text)
        env = dict(os.environ, CARGO_TARGET_DIR=str(BASE / ("target-mutant-" + label)))
        # A timeout or compilation failure is a failed layer, never a behavioral kill.
        run = subprocess.run(
            ["cargo", "test", "--test", "task_collect", "--locked", "--offline", "--", "--test-threads=1"],
            cwd=d, env=env, capture_output=True, text=True, timeout=120)
        (out / (label + ".stdout.txt")).write_text(run.stdout)
        (out / (label + ".stderr.txt")).write_text(run.stderr)
        killed = (run.returncode == 101 and f"test {test} ... FAILED" in run.stdout
                  and "could not compile" not in run.stderr)
        if not killed:
            raise RuntimeError(label + " did not produce a behavioral kill")
        results.append({"id": label, "test": test, "outcome": "KILLED", "exit_code": run.returncode,
                        "mutant_sha256": hashlib.sha256(p.read_bytes()).hexdigest()})
    if verified_state() != state:
        raise RuntimeError("source changed during mutations")
    (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{len(results)}/{len(CASES)} behavioral mutants killed")


if __name__ == "__main__":
    main()
