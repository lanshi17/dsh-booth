"""The pinned superagent patch applies cleanly and can be rerun during source builds."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "superagent"
PATCHER = ROOT / "tools" / "apply_superagent_patch.py"
FILES = ("src/deployment.ts", "src/app/runtime.ts", "src/app/sessions.ts", "src/index.ts", "tests/runtime.spec.ts", "tests/deployment.spec.ts", "tests/bundle-patch.spec.ts", "tests/cards.spec.ts", "cordis.patch.yml", "prompts/orchestrator.md")


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
            self.assertIn("ROUTER_DELEGATION_TOOLS", (bundle / "src/app/sessions.ts").read_text())
            self.assertIn("get_goal", (bundle / "src/app/sessions.ts").read_text())
            self.assertIn("@deepseek-ai/dsh-tool-goal", (bundle / "cordis.patch.yml").read_text())
            self.assertIn("delegate substantial implementation to parallel `subagent`s", (bundle / "prompts/orchestrator.md").read_text())
            bundle_test = (bundle / "tests/bundle-patch.spec.ts").read_text()
            self.assertIn("agentPreset: 'dsh-superagent-worker'", bundle_test)
            self.assertIn("@deepseek-ai/dsh-tool-subagent", bundle_test)
            self.assertIn("currentMasterId", (bundle / "src/app/runtime.ts").read_text())
            self.assertIn("const delegation = ROUTER_DELEGATION_TOOLS.filter", (bundle / "src/app/sessions.ts").read_text())
            deployment = (bundle / "src/deployment.ts").read_text()
            self.assertIn("const cancel = new AbortController()", deployment)
            self.assertIn("const modeCheck = run(cancel.signal", deployment)
            self.assertIn("misreports a missing persisted session", (bundle / "tests/runtime.spec.ts").read_text())
            self.assertIn("componentRegistration", (bundle / "src/index.ts").read_text())
            self.assertIn("/supervisor/v1/components/register", (bundle / "src/index.ts").read_text())

            second = subprocess.run([str(PATCHER), str(bundle)], text=True, capture_output=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertIn("already has", second.stdout)


if __name__ == "__main__":
    unittest.main()
