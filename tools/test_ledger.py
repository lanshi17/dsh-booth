#!/usr/bin/env python3
"""Regression test for the coverage ledger's fail-closed contract.

`ledger.py` regenerates the exclusion list from `changed_lines.py`. When that measurement
fails, the run has no misses — but that is not a clean ledger, and rewriting from it would
delete the reviewed entries.
"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

LEDGER = Path(__file__).with_name("ledger.py")
GATE = Path(__file__).with_name("changed_lines.py")


class LedgerTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "src").mkdir()
        (self.root / "tools").mkdir()
        (self.root / "target").mkdir()
        (self.root / "tools" / "changed_lines.py").write_text(GATE.read_text())
        self.ledger = self.root / "tools" / "coverage_excludes.txt"
        self.ledger.write_text(
            "# a reviewed entry\nsrc/app.rs:2 a reason that must survive the failed run\n"
        )
        (self.root / "src" / "main.rs").write_text("fn main() {\n    let _ = 1;\n}\n")
        self.git("init", "-q")
        self.git("config", "user.name", "test")
        self.git("config", "user.email", "test@example.com")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "tag.gpgsign", "false")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        self.git("tag", "v1")
        (self.root / "src" / "app.rs").write_text("fn app() {\n    let _ = 2;\n}\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "add app")
        # No target/cov.json: the measurement cannot run, so the ledger must not be rewritten.

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git", *args], cwd=self.root, capture_output=True, text=True, check=True
        )

    def report(self) -> None:
        path = str(self.root / "src" / "app.rs")
        segments = [[1, 1, 1, True, True, False],
                    [2, 5, 1, True, True, False],
                    [3, 1, 1, True, True, False]]
        report = {"data": [{"files": [{"filename": path, "segments": segments}]}]}
        (self.root / "target" / "cov.json").write_text(json.dumps(report))

    def test_a_ran_measurement_can_still_rewrite_stale_entries(self):
        # A stale ledger entry makes changed_lines.py exit 1, but with a measurement it is
        # exactly what this tool is for: the entry must be dropped, not treated as a crash.
        self.report()
        result = subprocess.run(
            [sys.executable, str(LEDGER)], cwd=self.root, capture_output=True, text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("src/app.rs:2", self.ledger.read_text())

    def test_a_failed_measurement_fails_without_clearing_the_ledger(self):
        before = self.ledger.read_bytes()
        result = subprocess.run(
            [sys.executable, str(LEDGER)], cwd=self.root, capture_output=True, text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("cannot measure the misses", result.stdout)
        self.assertEqual(self.ledger.read_bytes(), before, "the reviewed ledger was rewritten")


if __name__ == "__main__":
    unittest.main()
