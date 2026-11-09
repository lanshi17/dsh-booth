#!/usr/bin/env python3
"""Recreate the sealed review candidate without relying on ignored files."""
from pathlib import Path
import hashlib
import json
import subprocess

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
STATE = json.loads((BASE / 'source-state.json').read_text())
WORK = BASE / 'workspace/btae-eval'
if WORK.exists():
    raise RuntimeError('review workspace already exists; move it aside before reconstructing')
for name, expected in STATE['files_sha256'].items():
    override = BASE / 'round2-overrides' / name
    content = override.read_bytes() if override.exists() else subprocess.check_output(
        ['git', 'show', '73c4b3dae:btae-eval/' + name], cwd=ROOT)
    if hashlib.sha256(content).hexdigest() != expected:
        raise RuntimeError('reconstruction hash mismatch: ' + name)
    dst = WORK / name
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(content)
print(STATE['tree_sha256'])
