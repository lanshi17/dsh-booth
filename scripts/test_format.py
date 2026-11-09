"""Regression checks for active code versus source-hashed evidence selection."""

import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "repo_format", Path(__file__).with_name("format.py")
)
FORMAT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = FORMAT
SPEC.loader.exec_module(FORMAT)
PROJECT = Path(__file__).resolve().parent


class FormatterSelection(unittest.TestCase):
    def test_rust_keeps_tracked_and_untracked_code_but_not_archive_or_ignored(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            paths = [
                "codex-rs/src/tracked.rs",
                "btae-eval/src/untracked.rs",
                "superagent-forge/src/untracked.rs",
                "audit/revision/src/old.rs",
                "audit-other/current.rs",
                "ignored/worker.rs",
            ]
            for relative in paths:
                file = root / relative
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text("fn f() {}\n")
            (root / ".gitignore").write_text("ignored/\n")
            subprocess.run(["git", "-C", str(root), "add", paths[0]], check=True)
            with patch.object(FORMAT, "REPO_ROOT", root):
                group = FORMAT.rust_formatter_group(check=True)
            selected = {
                (command.cwd / argument).resolve()
                for command in group.commands
                for argument in command.args
                if argument.endswith(".rs")
            }
            self.assertEqual(
                selected, {root / relative for relative in paths[:3] + paths[4:5]}
            )
            self.assertTrue(
                all("--check" in command.args for command in group.commands)
            )

    def test_python_checker_rejects_active_bad_format_and_preserves_frozen_bytes(self):
        uv = shutil.which("uv")
        self.assertIsNotNone(uv, "CI requires the pinned uv formatter toolchain")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            frozen = [
                root / "audit/old.py",
                root / "btae-eval/repositories/queue/domain.py",
            ]
            active = root / "scripts/current.py"
            for file in frozen + [active]:
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text("items=[1,2]\n")
            original = {file: file.read_bytes() for file in frozen}
            for check in [True, False, True]:
                command = FORMAT.python_scripts_formatter_group(check=check).commands[0]
                args = list(command.args)
                args[0] = uv
                args[args.index("--project") + 1] = str(PROJECT)
                result = subprocess.run(args, cwd=root, capture_output=True, text=True)
                if check and active.read_text() == "items=[1,2]\n":
                    self.assertEqual(
                        result.returncode, 1, result.stdout + result.stderr
                    )
                    self.assertIn("current.py", result.stdout)
                    self.assertNotIn("old.py", result.stdout)
                    self.assertNotIn("domain.py", result.stdout)
                else:
                    self.assertEqual(
                        result.returncode, 0, result.stdout + result.stderr
                    )
                for file, expected in original.items():
                    self.assertEqual(file.read_bytes(), expected)
            self.assertEqual(active.read_text(), "items = [1, 2]\n")

    def test_formatter_start_or_execution_failure_does_not_pass(self):
        groups = [
            FORMAT.FormatterGroup(
                "missing", (FORMAT.Command(("/nonexistent/formatter",)),)
            ),
            FORMAT.FormatterGroup(
                "failed",
                (FORMAT.Command((sys.executable, "-c", "raise SystemExit(7)")),),
            ),
        ]
        for group in groups:
            with self.subTest(group=group.name):
                self.assertNotEqual(FORMAT.run_formatter_group(group).returncode, 0)


if __name__ == "__main__":
    unittest.main()
