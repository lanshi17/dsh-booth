#!/usr/bin/env python3
"""Validate the landing source, not a copied candidate; reports are outside scope."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
WORK = ROOT / 'btae-eval'
state = json.loads((BASE / 'final-source-state.json').read_text())
expected = state['files_sha256']
for name, digest in expected.items():
    if hashlib.sha256((WORK / name).read_bytes()).hexdigest() != digest:
        raise RuntimeError('landing input mismatch: ' + name)
actual_code = {str(p.relative_to(WORK)) for d in ['src', 'tests'] for p in (WORK / d).rglob('*') if p.is_file()}
expected_code = {p for p in expected if p.startswith(('src/', 'tests/'))}
if actual_code != expected_code:
    raise RuntimeError('landing code set mismatch')
out = (BASE / sys.argv[1]).resolve()
if not out.is_relative_to(BASE / 'output'):
    raise RuntimeError('output must be inside audit/output')
out.mkdir(parents=True, exist_ok=False)
env = dict(os.environ)
env['CARGO_TARGET_DIR'] = str(BASE / 'landing-target')
results = []
for name, cmd, expected_exit in [
    ('tests', ['cargo', 'test', '--all-targets', '--locked', '--offline', '-j', '2', '--', '--test-threads=4'], 0),
    ('lint', ['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'], 0),
    ('format', ['cargo', 'fmt', '--all', '--check'], 0),
    ('readiness', ['cargo', 'run', '--locked', '--offline', '--', 'check-ready', 'datasets/pilot.json'], 1),
]:
    run = subprocess.run(cmd, cwd=WORK, env=env, capture_output=True, text=True)
    (out / (name + '.stdout.txt')).write_text(run.stdout)
    (out / (name + '.stderr.txt')).write_text(run.stderr)
    results.append({'layer': name, 'command': cmd, 'actual_exit': run.returncode, 'expected_exit': expected_exit})
    (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    if run.returncode != expected_exit:
        raise RuntimeError(name + ': unexpected exit')
    print(name, run.returncode, flush=True)
for name, digest in expected.items():
    if hashlib.sha256((WORK / name).read_bytes()).hexdigest() != digest:
        raise RuntimeError('landing changed during validation: ' + name)
