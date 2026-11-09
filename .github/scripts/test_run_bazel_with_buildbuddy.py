#!/usr/bin/env python3

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

import run_bazel_with_buildbuddy


class RunBazelWithBuildBuddyTest(unittest.TestCase):
    def github_env(
        self,
        temp_dir: str,
        *,
        repository: str = "openai/codex",
        fork: bool = False,
        event_name: str = "pull_request",
    ) -> dict[str, str]:
        event_path = Path(temp_dir) / "event.json"
        event_path.write_text(
            json.dumps({"pull_request": {"head": {"repo": {"fork": fork}}}}),
            encoding="utf-8",
        )
        return {
            "BUILDBUDDY_API_KEY": "token",
            "GITHUB_ACTIONS": "true",
            "GITHUB_EVENT_NAME": event_name,
            "GITHUB_EVENT_PATH": str(event_path),
            "GITHUB_REPOSITORY": repository,
        }

    def test_keyless_invocation_drops_remote_ci_configuration(self) -> None:
        self.assertIsNone(
            run_bazel_with_buildbuddy.remote_config(
                ["build", "--config=ci-linux", "//codex-rs/cli:codex"],
                {},
            )
        )
        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_args_with_remote_config(
                ["build", "--config=ci-linux", "--", "//codex-rs/cli:codex"],
                {},
            ),
            ["build", "--", "//codex-rs/cli:codex"],
        )

    def test_program_arguments_after_separator_do_not_select_or_lose_rbe(self) -> None:
        args = ["run", "//codex-rs/cli:codex", "--", "--config=remote"]

        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_args_with_remote_config(args, {}),
            args,
        )
        self.assertEqual(
            run_bazel_with_buildbuddy.remote_config(
                args, {"BUILDBUDDY_API_KEY": "fork-token"}
            ),
            "buildbuddy-generic",
        )

    def test_upstream_push_selects_openai_rbe_before_target_separator(self) -> None:
        with TemporaryDirectory() as temp_dir:
            env = self.github_env(temp_dir, event_name="push")

            self.assertEqual(
                run_bazel_with_buildbuddy.bazel_args_with_remote_config(
                    ["build", "--config=ci-linux", "--", "//codex-rs/cli:codex"],
                    env,
                ),
                [
                    "build",
                    "--config=buildbuddy-openai-rbe",
                    "--remote_header=x-buildbuddy-api-key=token",
                    "--config=ci-linux",
                    "--",
                    "//codex-rs/cli:codex",
                ],
            )

    def test_windows_cross_ci_configuration_follows_remote_configuration(self) -> None:
        env = {"BUILDBUDDY_API_KEY": "fork-token"}

        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_args_with_remote_config(
                ["build", "--config=ci-windows-cross", "//codex-rs/cli:codex"],
                env,
            ),
            [
                "build",
                "--config=buildbuddy-generic-rbe",
                "--remote_header=x-buildbuddy-api-key=fork-token",
                "--config=ci-windows-cross",
                "//codex-rs/cli:codex",
            ],
        )

    def test_query_remote_configuration_is_inserted_before_expression(self) -> None:
        expression = 'kind("rust_library rule", //codex-rs/...)'
        env = {"BUILDBUDDY_API_KEY": "fork-token"}

        for command in ("query", "cquery", "aquery"):
            with self.subTest(command=command):
                self.assertEqual(
                    run_bazel_with_buildbuddy.bazel_args_with_remote_config(
                        [
                            command,
                            "--config=ci-windows-cross",
                            "--output=label",
                            expression,
                        ],
                        env,
                    ),
                    [
                        command,
                        "--config=buildbuddy-generic-rbe",
                        "--remote_header=x-buildbuddy-api-key=fork-token",
                        "--config=ci-windows-cross",
                        "--output=label",
                        expression,
                    ],
                )

    def test_same_repository_pull_request_selects_openai_host(self) -> None:
        with TemporaryDirectory() as temp_dir:
            self.assertEqual(
                run_bazel_with_buildbuddy.remote_config(
                    ["build", "--config=ci-v8"], self.github_env(temp_dir)
                ),
                "buildbuddy-openai-rbe",
            )

    def test_fork_pull_request_cannot_select_openai_host(self) -> None:
        with TemporaryDirectory() as temp_dir:
            env = self.github_env(temp_dir, fork=True)

            self.assertEqual(
                run_bazel_with_buildbuddy.remote_config(
                    ["build", "--config=ci-v8"], env
                ),
                "buildbuddy-generic-rbe",
            )

    def test_run_in_fork_repository_cannot_select_openai_host(self) -> None:
        with TemporaryDirectory() as temp_dir:
            env = self.github_env(temp_dir, repository="contributor/codex")

            self.assertEqual(
                run_bazel_with_buildbuddy.remote_config(
                    ["build", "--config=ci-v8"], env
                ),
                "buildbuddy-generic-rbe",
            )

    def test_pull_request_without_readable_event_payload_fails_closed(self) -> None:
        for event_path in (None, "missing-event.json"):
            env = {
                "BUILDBUDDY_API_KEY": "token",
                "GITHUB_ACTIONS": "true",
                "GITHUB_EVENT_NAME": "pull_request",
                "GITHUB_REPOSITORY": "openai/codex",
            }
            if event_path is not None:
                env["GITHUB_EVENT_PATH"] = event_path

            with self.subTest(event_path=event_path):
                self.assertEqual(
                    run_bazel_with_buildbuddy.remote_config(["build"], env),
                    "buildbuddy-generic",
                )

    def test_bazel_command_uses_configured_binary_locally(self) -> None:
        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_command(
                "info",
                "execution_root",
                env={"CODEX_BAZEL_BIN": "fake-bazel"},
            ),
            ["fake-bazel", "info", "execution_root"],
        )

    def test_bazel_command_normalizes_github_actions_startup_options(self) -> None:
        env = {
            "BAZEL_OUTPUT_USER_ROOT": "/tmp/bazel-output",
            "GITHUB_ACTIONS": "true",
        }

        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_command("build", "//codex-rs/...", env=env),
            [
                "bazel",
                "--output_user_root=/tmp/bazel-output",
                "--noexperimental_remote_repo_contents_cache",
                "build",
                "//codex-rs/...",
            ],
        )
        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_command(
                "--experimental_remote_repo_contents_cache",
                "build",
                "//codex-rs/...",
                env=env,
            ),
            [
                "bazel",
                "--output_user_root=/tmp/bazel-output",
                "--experimental_remote_repo_contents_cache",
                "build",
                "//codex-rs/...",
            ],
        )

    def test_bazel_command_uses_configured_local_caches(self) -> None:
        env = {
            "BAZEL_REPO_CONTENTS_CACHE": "/tmp/bazel-repo-contents",
            "BAZEL_REPOSITORY_CACHE": "/tmp/bazel-repository",
        }

        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_command(
                "build",
                "--config=local",
                "//codex-rs/...",
                env=env,
            ),
            [
                "bazel",
                "build",
                "--config=local",
                "//codex-rs/...",
                "--repo_contents_cache=/tmp/bazel-repo-contents",
                "--repository_cache=/tmp/bazel-repository",
            ],
        )

    def test_bazel_command_adds_local_caches_before_separator(self) -> None:
        self.assertEqual(
            run_bazel_with_buildbuddy.bazel_command(
                "build",
                "//codex-rs/...",
                "--",
                "--program-arg",
                env={"BAZEL_REPOSITORY_CACHE": "/tmp/bazel-repository"},
            ),
            [
                "bazel",
                "build",
                "//codex-rs/...",
                "--repository_cache=/tmp/bazel-repository",
                "--",
                "--program-arg",
            ],
        )

    def test_main_preserves_spaced_argument_and_child_exit_status(self) -> None:
        spaced_arg = (
            r"--test_env=PATH=C:\Program Files\PowerShell\7;C:\Program Files\Git\bin"
        )
        child_code = (
            f"import sys; sys.exit(37 if sys.argv[1] == {spaced_arg!r} else 91)"
        )
        env = os.environ.copy()
        env["CODEX_BAZEL_BIN"] = sys.executable
        env.pop("BAZEL_OUTPUT_USER_ROOT", None)
        env.pop("BUILDBUDDY_API_KEY", None)
        env.pop("GITHUB_ACTIONS", None)

        result = subprocess.run(
            [
                sys.executable,
                str(Path(run_bazel_with_buildbuddy.__file__)),
                "-c",
                child_code,
                spaced_arg,
            ],
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 37, result.stderr)

    @unittest.skipUnless(os.name == "posix", "exercises the Bash CI helper on Unix")
    def test_keyless_windows_failure_logs_use_the_test_target_platform(self) -> None:
        with TemporaryDirectory() as temp_dir:
            capture = Path(temp_dir) / "args.jsonl"
            testlogs = Path(temp_dir) / "testlogs"
            test_log = testlogs / "fixture" / "test" / "test.log"
            test_log.parent.mkdir(parents=True)
            test_log.write_text("fixture failure details\n", encoding="utf-8")
            fake_bazel = Path(temp_dir) / "fake-bazel"
            fake_bazel.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['BAZEL_CAPTURE'], 'a') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if 'info' in sys.argv:\n"
                "    print(os.environ['FIXTURE_TESTLOGS'])\n"
                "else:\n"
                "    print('FAIL: //fixture:test')\n"
                "    sys.exit(37)\n",
                encoding="utf-8",
            )
            fake_bazel.chmod(0o755)
            result = subprocess.run(
                [
                    "bash",
                    str(Path(__file__).with_name("run-bazel-ci.sh")),
                    "--windows-cross-compile",
                    "--print-failed-test-logs",
                    "--",
                    "test",
                    "--",
                    "//fixture:test",
                ],
                env={
                    "PATH": os.defpath,
                    "RUNNER_OS": "Windows",
                    "CODEX_BAZEL_BIN": str(fake_bazel),
                    "BAZEL_CAPTURE": str(capture),
                    "FIXTURE_TESTLOGS": str(testlogs),
                    "CODEX_BAZEL_WINDOWS_PATH": r"C:\Windows",
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertIn("fixture failure details", result.stdout)
            calls = [json.loads(line) for line in capture.read_text().splitlines()]
            self.assertEqual(len(calls), 2)
            for args in calls:
                self.assertIn("--platforms=//:windows_x86_64_gnullvm", args)
                self.assertIn("--host_platform=//:local_windows", args)
            self.assertIn("--local_test_jobs=1", calls[0])
            self.assertIn("--test_env=RUST_TEST_THREADS=1", calls[0])
            self.assertFalse(any("SKIP_FILTERS" in arg for arg in calls[0]))
            self.assertFalse(any("debuginfo=0" in arg for arg in calls[0]))

            # Exercise the same real wrapper on Unix hosted/local builds. The
            # compact test profile must never strip release package symbols.
            for runner_os in ("Linux", "macOS"):
                for hosted, release, verify_release in (
                    (True, False, False),
                    (True, True, False),
                    (True, False, True),
                    (False, False, False),
                ):
                    with self.subTest(
                        runner_os=runner_os, hosted=hosted, release=release
                    ):
                        capture.unlink()
                        args = ["build"]
                        if release:
                            args.append("--config=release")
                        if verify_release:
                            args.append("--build_metadata=TAG_job=verify-release-build")
                        result = subprocess.run(
                            [
                                "bash",
                                str(Path(__file__).with_name("run-bazel-ci.sh")),
                                "--",
                                *args,
                                "--",
                                "//fixture:test",
                            ],
                            env={
                                "PATH": os.defpath,
                                "RUNNER_OS": runner_os,
                                "RUNNER_ENVIRONMENT": "github-hosted"
                                if hosted
                                else "self-hosted",
                                "CODEX_BAZEL_BIN": str(fake_bazel),
                                "BAZEL_CAPTURE": str(capture),
                            },
                            capture_output=True,
                            text=True,
                            check=False,
                        )
                        self.assertEqual(result.returncode, 37, result.stderr)
                        [call] = [
                            json.loads(line)
                            for line in capture.read_text().splitlines()
                        ]
                        self.assertEqual(
                            f"--jobs={'1' if verify_release else '4'}" in call,
                            hosted,
                        )
                        self.assertEqual("--local_test_jobs=1" in call, hosted)
                        for flag in ("extra_rustc_flag", "extra_exec_rustc_flag"):
                            self.assertEqual(
                                f"--@rules_rust//rust/settings:{flag}=-Cdebuginfo=0"
                                in call,
                                hosted and not release,
                            )

    @unittest.skipUnless(os.name == "posix", "exercises the Bash CI helper on Unix")
    def test_windows_voice_inputs_are_fixed_and_do_not_reach_linux_rbe(self) -> None:
        with TemporaryDirectory() as temp_dir:
            capture = Path(temp_dir) / "args.json"
            fake_bazel = Path(temp_dir) / "fake-bazel"
            fake_bazel.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "Path(os.environ['BAZEL_CAPTURE']).write_text(json.dumps(sys.argv[1:]))\n",
                encoding="utf-8",
            )
            fake_bazel.chmod(0o755)
            env = {
                "PATH": os.defpath,
                "RUNNER_OS": "Windows",
                "CODEX_BAZEL_BIN": str(fake_bazel),
                "BAZEL_CAPTURE": str(capture),
                "CODEX_BAZEL_WINDOWS_PATH": r"C:\Program Files\Build Tools",
                "VOICE_WINDOWS_BAZEL_REPOSITORY": r"D:\voice tools",
                "VOICE_WINDOWS_SYSTEM_ROOT": r"C:\Windows",
                "VOICE_WINDOWS_HOST_ARCH": "AMD64",
            }
            for cross_compile, remote, status_override in (
                (False, False, None),
                (True, False, None),
                (True, True, None),
                (False, False, "--workspace_status_command=./custom-status.cmd"),
            ):
                with self.subTest(
                    cross_compile=cross_compile,
                    remote=remote,
                    status_override=status_override,
                ):
                    case_env = dict(env)
                    if remote:
                        case_env["BUILDBUDDY_API_KEY"] = "fixture"
                    result = subprocess.run(
                        [
                            "bash",
                            str(Path(__file__).with_name("run-bazel-ci.sh")),
                            *(["--windows-cross-compile"] if cross_compile else []),
                            "--",
                            "build",
                            *([status_override] if status_override else []),
                            "--",
                            "//third_party/voice:pkg_config",
                        ],
                        env=case_env,
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    args = json.loads(capture.read_text())
                    self.assertEqual(
                        [
                            arg
                            for arg in args
                            if arg.startswith("--workspace_status_command=")
                        ],
                        [
                            status_override
                            or "--workspace_status_command=./scripts/workspace-status.cmd"
                        ],
                    )
                    voice_args = [
                        r"--inject_repository=voice_windows_tools=D:\voice tools",
                        "--//third_party/voice:windows_installed_tools=@voice_windows_tools//:tools",
                        r"--action_env=SystemRoot=C:\Windows",
                        r"--host_action_env=SystemRoot=C:\Windows",
                        "--action_env=PROCESSOR_ARCHITECTURE=AMD64",
                        "--host_action_env=PROCESSOR_ARCHITECTURE=AMD64",
                    ]
                    for arg in voice_args:
                        if remote:
                            self.assertNotIn(arg, args)
                        else:
                            self.assertIn(arg, args)
                            self.assertLess(args.index(arg), args.index("--"))
                    if cross_compile and not remote:
                        for arg in (
                            "--host_platform=//:local_windows",
                            "--platforms=//:windows_x86_64_gnullvm",
                            "--extra_execution_platforms=//:windows_x86_64_gnullvm",
                            "--extra_toolchains=//:windows_gnullvm_tests_on_gnullvm_host_toolchain",
                        ):
                            self.assertIn(arg, args)
                        self.assertNotIn("--host_platform=//:local_windows_msvc", args)
                    if not remote:
                        self.assertIn("--disk_cache=", args)


if __name__ == "__main__":
    unittest.main()
