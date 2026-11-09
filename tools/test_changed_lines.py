#!/usr/bin/env python3
"""Regression tests for the changed-line gate's fail-closed contract.

`changed_lines.py` claims every module under `src` is measured, and that a source file the
coverage report does not mention is a failure, never a pass. Exercised against a throwaway
repository so the test does not depend on this branch's own diff.
"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("changed_lines.py")


class ChangedLinesTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "src").mkdir()
        (self.root / "tools").mkdir()
        (self.root / "target").mkdir()
        (self.root / "tools" / "coverage_excludes.txt").write_text("# no exclusions\n")
        (self.root / "src" / "main.rs").write_text("fn main() {\n    let _ = 1;\n}\n")
        self.git("init", "-q")
        self.git("config", "user.name", "test")
        self.git("config", "user.email", "test@example.com")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        self.base = self.git("rev-parse", "HEAD").stdout.strip()
        # A file this branch adds. The diff names it, so the report must name it too.
        (self.root / "src" / "app.rs").write_text("fn app() {\n    let _ = 2;\n}\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "add app")

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git", *args], cwd=self.root, capture_output=True, text=True, check=True
        )

    def report(self, files: list[dict]) -> None:
        (self.root / "target" / "cov.json").write_text(json.dumps({"data": [{"files": files}]}))

    def run_gate(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), self.base, "target/cov.json"],
            cwd=self.root, capture_output=True, text=True, check=False,
        )

    def segments(self, path: str) -> dict:
        return {
            "filename": path,
            "segments": [[1, 1, 1, True, True, False],
                         [2, 5, 1, True, True, False],
                         [3, 1, 1, True, True, False]],
        }

    def test_a_changed_file_missing_from_the_report_fails(self):
        # main.rs is untouched by the diff, so the report naming only it must not excuse app.rs.
        self.report([self.segments(str(self.root / "src" / "main.rs"))])
        result = self.run_gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("does not mention: src/app.rs", result.stdout)

    def test_a_missing_report_fails_closed(self):
        result = self.run_gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("cannot read the coverage report", result.stdout)

    def test_a_covered_changed_file_passes(self):
        self.report([self.segments(str(self.root / "src" / "app.rs")),
                     self.segments(str(self.root / "src" / "main.rs"))])
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("3/3 covered", result.stdout)

    def test_a_source_file_the_branch_adds_is_measured(self):
        # Watching `src` instead of a list of today's modules is what makes this fail: a new
        # module's uncovered line must not pass just because no list names it.
        (self.root / "src" / "extra.rs").write_text("fn extra() {\n    let _ = 3;\n}\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "add extra")
        self.report([
            self.segments(str(self.root / "src" / "app.rs")),
            {"filename": str(self.root / "src" / "extra.rs"),
             "segments": [[1, 1, 1, True, True, False], [2, 5, 0, True, True, False]]},
        ])
        result = self.run_gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("UNCOVERED src/extra.rs:2", result.stdout)

    def test_the_test_only_fixture_is_not_measured(self):
        # testkit.rs is compiled only into the test binary; watching `src` must not start
        # demanding that a fixture the report may not name be covered.
        (self.root / "src" / "testkit.rs").write_text("pub fn fixture() {\n    let _ = 4;\n}\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "add testkit")
        self.report([self.segments(str(self.root / "src" / "app.rs")),
                     self.segments(str(self.root / "src" / "main.rs"))])
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
