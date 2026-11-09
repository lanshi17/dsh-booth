"""Bind verification to public Rust sources and frozen fixture inputs only."""
from pathlib import Path
import hashlib
import json

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
SOURCE = ROOT / "btae-eval"


def snapshot():
    paths = [SOURCE / "Cargo.toml", SOURCE / "Cargo.lock"]
    for folder in ("src", "tests", "datasets", "baselines", "repositories"):
        paths.extend(p for p in (SOURCE / folder).rglob("*") if p.is_file())
    files = {}
    for p in sorted(paths):
        if p.is_symlink():
            raise RuntimeError("symlink in public input: " + str(p))
        files[p.relative_to(SOURCE).as_posix()] = hashlib.sha256(p.read_bytes()).hexdigest()
    encoded = json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
    return {"tree_sha256": hashlib.sha256(encoded).hexdigest(), "files_sha256": files}


def check(actual, expected):
    if actual != expected:
        raise RuntimeError("public source/input state does not match source-state.json")


def verified_state():
    expected = json.loads((BASE / "source-state.json").read_text())
    check(snapshot(), expected)
    return expected
