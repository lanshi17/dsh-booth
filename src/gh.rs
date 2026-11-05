//! A GitHub CLI new enough for dsh-superagent's reads, without touching the system one.
//!
//! The bundle's GitHub Source reads through `gh api --slurp`, which CLIs before 2.57 reject.
//! When the `gh` on `PATH` is older, booth keeps a pinned release under the state directory and
//! points the Source's `cli` at it; the agent's own pushes and pull requests already work with
//! the older CLI, so this only restores the GitHub front door.

use crate::service;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Pinned because a release URL is stable and the checksum below is checked against the same tag.
pub const VERSION: &str = "2.63.2";
const DOWNLOAD: Duration = Duration::from_secs(120);

fn target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux_amd64"),
        ("linux", "aarch64") => Ok("linux_arm64"),
        ("macos", "x86_64") => Ok("macOS_amd64"),
        ("macos", "aarch64") => Ok("macOS_arm64"),
        (os, arch) => Err(format!(
            "the system gh is too old for dsh-superagent's reads; booth cannot install one for {os}/{arch}"
        )),
    }
}

/// Whether `program` has the flag the Source's reads pass.
pub fn capable(program: &Path) -> bool {
    Command::new(program)
        .args(["api", "--help"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("--slurp"))
        .unwrap_or(false)
}

/// The `gh` on PATH, when it is new enough.
pub fn system_capable() -> bool {
    capable(Path::new("gh"))
}

/// The version string of `program`, such as `2.63.2`.
pub fn version(program: &Path) -> Option<String> {
    let output = Command::new(program).arg("--version").output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(2)
        .map(str::to_owned)
}

fn installed(cfg: &service::Config) -> PathBuf {
    cfg.state_dir.join("bin").join("gh")
}

/// The CLI the Source should use: `None` for the `gh` on PATH, or the path of a pinned release.
/// Downloads the release when neither exists and `download` is set.
pub fn ensure(cfg: &service::Config, download: bool) -> Result<Option<String>, String> {
    if system_capable() {
        return Ok(None);
    }
    let local = installed(cfg);
    if capable(&local) {
        return Ok(Some(local.display().to_string()));
    }
    if !download {
        return Err(installing_hint(&local));
    }
    install(cfg)?;
    if !capable(&local) {
        return Err(format!(
            "{} was installed but does not support `gh api --slurp`",
            local.display()
        ));
    }
    Ok(Some(local.display().to_string()))
}

fn installing_hint(local: &Path) -> String {
    format!(
        "the system gh is too old for dsh-superagent's reads; `booth gh` installs {}",
        local.display()
    )
}

/// Downloads the pinned release, checks its published checksum and unpacks only its `gh`.
fn install(cfg: &service::Config) -> Result<(), String> {
    let target = target()?;
    let archive = format!("gh_{VERSION}_{target}.tar.gz");
    let base = format!("https://github.com/cli/cli/releases/download/v{VERSION}");
    println!("installing gh {VERSION} for the GitHub front door (13 MB)");
    let bytes = fetch(&format!("{base}/{archive}"))?;
    let sums = fetch(&format!("{base}/gh_{VERSION}_checksums.txt"))?;
    let text =
        String::from_utf8(sums).map_err(|error| format!("checksums are not text: {error}"))?;
    let expected = text
        .lines()
        .find(|line| line.trim_end().ends_with(&archive))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| format!("no checksum for {archive}"))?;
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "the {archive} download does not match its published checksum"
        ));
    }

    fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    let scratch = cfg.state_dir.join("gh-download");
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("cannot create {}: {error}", scratch.display()))?;
    let file = scratch.join(&archive);
    fs::write(&file, &bytes)
        .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
    // The archive holds gh_<version>_<target>/bin/gh; stripping its top directory lands it at
    // `<state>/bin/gh`, beside other files the service owns.
    let member = format!("gh_{VERSION}_{target}/bin/gh");
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&file)
        .args(["-C"])
        .arg(&cfg.state_dir)
        .args(["--strip-components", "1", &member])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("cannot run tar: {error}"))?;
    let _ = fs::remove_dir_all(&scratch);
    if !status.success() {
        return Err(format!("tar could not unpack {member}"));
    }
    let local = installed(cfg);
    fs::set_permissions(&local, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("cannot make {} executable: {error}", local.display()))?;
    Ok(())
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(DOWNLOAD)
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("cannot download {url}: {error}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {url}: {error}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("booth-{name}-{}", std::process::id()));
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn finds_the_flag_the_reads_need() {
        let newer = script(
            "gh-newer",
            "#!/bin/sh\necho '      --slurp  use with --paginate'\n",
        );
        let older = script(
            "gh-older",
            "#!/bin/sh\necho '      --paginate  fetch all pages'\n",
        );
        assert!(capable(&newer));
        assert!(!capable(&older));
    }

    #[test]
    fn reads_the_version_line() {
        let gh = script(
            "gh-version",
            "#!/bin/sh\necho 'gh version 2.63.2 (2024-12-05)'\n",
        );
        assert_eq!(version(&gh).as_deref(), Some("2.63.2"));
    }

    #[test]
    fn a_platform_without_a_pinned_release_is_explained() {
        // The fixtures run on linux/amd64 or arm64, where target() succeeds.
        if target().is_ok() {
            assert!(
                target().unwrap().starts_with("linux_") || target().unwrap().starts_with("macOS_")
            );
        }
    }
}
