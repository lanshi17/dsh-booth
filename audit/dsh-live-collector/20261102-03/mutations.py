#!/usr/bin/env python3
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
from source_state import BASE, SOURCE, verified_state

CASES = [
    ("clear-env", ".env_clear()\n        .envs", "\n        .envs",
     "environment_is_cleared_and_only_allowlist_is_visible"),
    ("stdout-inclusive", "stdout.bytes > req.limits.max_stdout_bytes as u64",
     "stdout.bytes >= req.limits.max_stdout_bytes as u64", "exact_stdout_limit_is_inclusive"),
    ("stop-on-error", "if (status.is_some() || outcome.is_some()) && cleanup.is_none() {",
     "if (status.is_some() || outcome.is_some_and(|o| !matches!(o, LiveOutcome::ProtocolError))) && cleanup.is_none() {",
     "malformed_transcript_is_protocol_error"),
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
        p = d / "src/dsh_live.rs"
        text = p.read_text()
        if text.count(before) != 1:
            raise RuntimeError("mutation target is not unique: " + label)
        text = text.replace(before, after, 1)
        if label == "stop-on-error":
            direct = '                        note = Some("invalid_dsh_stream".into());\n                        kill_tree(&mut child.0);'
            if text.count(direct) != 1:
                raise RuntimeError("direct cleanup mutation target is not unique")
            text = text.replace(direct, '                        note = Some("invalid_dsh_stream".into());', 1)
        p.write_text(text)
        env = dict(os.environ, CARGO_TARGET_DIR=str(BASE / ("target-" + label)))
        # A timeout or compilation failure is a failed layer, never a behavioral kill.
        run = subprocess.run(
            ["cargo", "test", "--test", "dsh_live", "--locked", "--offline", "--", "--test-threads=1"],
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
