import os
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

import reclaim_hosted_build_space


class ReclaimHostedBuildSpaceTest(unittest.TestCase):
    def test_preserves_selected_xcode_and_every_alias_to_it(self) -> None:
        with TemporaryDirectory() as directory:
            applications = Path(directory)
            selected = applications / "Xcode_26.app"
            developer = selected / "Contents" / "Developer"
            developer.mkdir(parents=True)
            (applications / "Xcode.app").symlink_to(selected, target_is_directory=True)
            unused = applications / "Xcode_25.app"
            unused.mkdir()
            unrelated = applications / "Other.app"
            unrelated.mkdir()
            self.assertEqual(
                reclaim_hosted_build_space.unused_xcodes(applications, developer),
                [unused],
            )

    def test_invalid_selection_cannot_authorize_xcode_removal(self) -> None:
        with TemporaryDirectory() as directory:
            applications = Path(directory)
            (applications / "Xcode.app").mkdir()
            with self.assertRaises(ValueError):
                reclaim_hosted_build_space.unused_xcodes(applications, applications)
            with self.assertRaises(FileNotFoundError):
                reclaim_hosted_build_space.unused_xcodes(
                    applications, applications / "missing"
                )

    def test_never_deletes_on_local_or_self_hosted_machines(self) -> None:
        for environment in (
            {},
            {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "self-hosted"},
            {"RUNNER_ENVIRONMENT": "github-hosted"},
        ):
            with (
                self.subTest(environment=environment),
                patch.dict(os.environ, environment, clear=True),
                patch.object(reclaim_hosted_build_space.subprocess, "run") as run,
                patch.object(
                    reclaim_hosted_build_space.subprocess, "check_output"
                ) as check_output,
            ):
                reclaim_hosted_build_space.reclaim()
                run.assert_not_called()
                check_output.assert_not_called()


if __name__ == "__main__":
    unittest.main()
