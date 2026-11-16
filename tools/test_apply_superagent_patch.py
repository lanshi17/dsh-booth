"""The pinned superagent patch applies cleanly and can be rerun during source builds."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "superagent"
PATCHER = ROOT / "tools" / "apply_superagent_patch.py"
FILES = ("src/app/runtime.ts", "src/app/sessions.ts", "tests/runtime.spec.ts")


class ApplySuperagentPatchTests(unittest.TestCase):
    def test_applies_once_to_the_pinned_source_and_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory(prefix="booth-superagent-patch-") as temporary:
            bundle = Path(temporary)
            for name in FILES:
                target = bundle / name
                target.parent.mkdir(parents=True, exist_ok=True)
                content = subprocess.check_output(
                    ["git", "-C", str(SOURCE), "show", f"HEAD:{name}"], text=True
                )
                target.write_text(content)

            first = subprocess.run([str(PATCHER), str(bundle)], text=True, capture_output=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertIn("replaceMasterId", (bundle / "src/app/sessions.ts").read_text())
            self.assertIn("currentMasterId", (bundle / "src/app/runtime.ts").read_text())
            self.assertIn("misreports a missing persisted session", (bundle / "tests/runtime.spec.ts").read_text())

            second = subprocess.run([str(PATCHER), str(bundle)], text=True, capture_output=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertIn("already has", second.stdout)


if __name__ == "__main__":
    unittest.main()
