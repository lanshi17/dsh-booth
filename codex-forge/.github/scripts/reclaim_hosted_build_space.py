#!/usr/bin/env python3
"""Reclaim unused preinstalled SDKs on disposable GitHub-hosted Bazel runners."""

import os
import shutil
import subprocess
from pathlib import Path


LINUX_UNUSED_SDKS = (
    Path("/usr/local/lib/android"),
    Path("/usr/share/dotnet"),
    Path("/opt/ghc"),
    Path("/usr/local/.ghcup"),
    # These hosted-image installations are not consumed by this repository's
    # Bazel, Rust, Python, or SDK workflows. Keep the paths exact so a future
    # runner image change cannot turn reclamation into a broad deletion.
    Path("/opt/hostedtoolcache/CodeQL"),
    Path("/usr/local/share/boost"),
)


def unused_xcodes(applications: Path, selected_developer: Path) -> list[Path]:
    selected = selected_developer.resolve(strict=True)
    selected_app = next(
        (parent for parent in selected.parents if parent.suffix == ".app"), None
    )
    if selected_app is None or selected_app.parent != applications.resolve():
        raise ValueError("Selected Xcode must be an application inside /Applications")
    return sorted(
        path
        for path in applications.glob("Xcode*.app")
        if path.resolve() != selected_app
    )


def reclaim() -> None:
    if (
        os.environ.get("GITHUB_ACTIONS") != "true"
        or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
    ):
        print("SDK reclamation only applies to disposable GitHub-hosted runners.")
        return

    runner_os = os.environ.get("RUNNER_OS")
    if runner_os == "Linux":
        candidates = LINUX_UNUSED_SDKS
    elif runner_os == "macOS":
        developer = subprocess.check_output(["xcode-select", "-p"], text=True).strip()
        candidates = unused_xcodes(Path("/Applications"), Path(developer))
    else:
        return

    before = shutil.disk_usage(Path.cwd()).free
    for path in candidates:
        if path.exists() or path.is_symlink():
            print(f"Removing unused hosted SDK: {path}", flush=True)
            subprocess.run(["sudo", "-n", "rm", "-rf", "--", str(path)], check=True)
    after = shutil.disk_usage(Path.cwd()).free
    print(
        f"Build disk: {before // (1024**3)} GiB free before, "
        f"{after // (1024**3)} GiB free after SDK reclamation."
    )


if __name__ == "__main__":
    reclaim()
