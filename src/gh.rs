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

/// Runs `program` and returns its standard output, retrying once: a just-written script can
/// fail with `ETXTBSY` while another thread still holds it open for writing.
fn output_of(program: &Path, args: &[&str]) -> Option<String> {
    for attempt in 0..2 {
        match Command::new(program).args(args).output() {
            Ok(output) => return Some(String::from_utf8_lossy(&output.stdout).into_owned()),
            Err(error)
                if attempt == 0 && error.kind() == std::io::ErrorKind::ExecutableFileBusy =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    None
}

/// Whether `program` has the flag the Source's reads pass.
pub fn capable(program: &Path) -> bool {
    output_of(program, &["api", "--help"])
        .map(|text| text.contains("--slurp"))
        .unwrap_or(false)
}

/// The `gh` on PATH, when it is new enough.
pub fn system_capable() -> bool {
    capable(Path::new("gh"))
}

/// The version string of `program`, such as `2.63.2`.
pub fn version(program: &Path) -> Option<String> {
    output_of(program, &["--version"])?
        .split_whitespace()
        .nth(2)
        .map(str::to_owned)
}

/// The pinned release's path. It is handed to the Source, which runs it from its own working
/// directory (the bundle's), so a state directory named relative to `/setup`'s directory is
/// resolved here instead of leaking a path that would not exist for the service.
fn installed(cfg: &service::Config) -> PathBuf {
    let local = cfg.state_dir.join("bin").join("gh");
    std::path::absolute(&local).unwrap_or(local)
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
        "the system gh is too old for dsh-superagent's reads; `booth gh --install` installs {}",
        local.display()
    )
}

/// The asset GitHub publishes for a target: Linux is a `.tar.gz`, macOS is a `.zip`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Archive {
    TarGz,
    Zip,
}

impl Archive {
    fn for_target(target: &str) -> Self {
        if target.starts_with("macOS_") {
            Self::Zip
        } else {
            Self::TarGz
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::TarGz => ".tar.gz",
            Self::Zip => ".zip",
        }
    }

    /// Unpack only `member` from `archive`, landing it at `<state>/bin/gh`.
    fn unpack(self, archive: &Path, member: &str, state_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(state_dir)
            .map_err(|error| format!("cannot create {}: {error}", state_dir.display()))?;
        match self {
            Self::TarGz => {
                // The archive holds gh_<version>_<target>/bin/gh; stripping its top directory
                // lands it at `<state>/bin/gh`, beside other files the service owns.
                let status = Command::new("tar")
                    .args(["-xzf"])
                    .arg(archive)
                    .args(["-C"])
                    .arg(state_dir)
                    .args(["--strip-components", "1", member])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map_err(|error| format!("cannot run tar: {error}"))?;
                if !status.success() {
                    return Err(format!("tar could not unpack {member}"));
                }
                Ok(())
            }
            Self::Zip => {
                // `unzip -p` writes the member to stdout, so only the binary is unpacked instead
                // of the release's manual pages and license. The write is renamed into place
                // afterwards, so a failed unpack cannot leave a half-written CLI behind.
                let bin = state_dir.join("bin");
                fs::create_dir_all(&bin)
                    .map_err(|error| format!("cannot create {}: {error}", bin.display()))?;
                let partial = bin.join(format!("gh-{}.partial", std::process::id()));
                let file = fs::File::create(&partial)
                    .map_err(|error| format!("cannot write {}: {error}", partial.display()))?;
                let status = Command::new("unzip")
                    .arg("-p")
                    .arg(archive)
                    .arg(member)
                    .stdout(Stdio::from(file))
                    .stderr(Stdio::null())
                    .status()
                    .map_err(|error| format!("cannot run unzip: {error}"))?;
                if !status.success() {
                    let _ = fs::remove_file(&partial);
                    return Err(format!("unzip could not unpack {member}"));
                }
                fs::rename(&partial, bin.join("gh")).map_err(|error| {
                    format!("cannot move {} into place: {error}", partial.display())
                })?;
                Ok(())
            }
        }
    }
}

/// Downloads the pinned release, checks its published checksum and unpacks only its `gh`.
fn install(cfg: &service::Config) -> Result<(), String> {
    let target = target()?;
    let format = Archive::for_target(target);
    let asset = format!("gh_{VERSION}_{target}{}", format.suffix());
    let base = release_base();
    let bytes = fetch(&format!("{base}/{asset}"))?;
    let sums = fetch(&format!("{base}/gh_{VERSION}_checksums.txt"))?;
    let text =
        String::from_utf8(sums).map_err(|error| format!("checksums are not text: {error}"))?;
    let expected = checksum(&text, &asset)?;
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(format!(
            "the {asset} download does not match its published checksum"
        ));
    }

    fs::create_dir_all(&cfg.state_dir)
        .map_err(|error| format!("cannot create {}: {error}", cfg.state_dir.display()))?;
    let scratch = cfg.state_dir.join("gh-download");
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("cannot create {}: {error}", scratch.display()))?;
    let file = scratch.join(&asset);
    fs::write(&file, &bytes)
        .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
    let member = format!("gh_{VERSION}_{target}/bin/gh");
    let unpacked = format.unpack(&file, &member, &cfg.state_dir);
    let _ = fs::remove_dir_all(&scratch);
    unpacked?;
    let local = installed(cfg);
    fs::set_permissions(&local, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("cannot make {} executable: {error}", local.display()))?;
    Ok(())
}

/// The release host. The install test points this at a loopback fixture instead of GitHub; the
/// download is still checked against the checksums served beside it.
fn release_base() -> String {
    std::env::var("BOOTH_GH_RELEASE_BASE")
        .unwrap_or_else(|_| format!("https://github.com/cli/cli/releases/download/v{VERSION}"))
}

/// The checksum line for `asset` in the published checksums file.
fn checksum(text: &str, asset: &str) -> Result<String, String> {
    text.lines()
        .find(|line| line.trim_end().ends_with(asset))
        .and_then(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .ok_or_else(|| format!("no checksum for {asset}"))
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn script(name: &str, body: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "booth-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
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
    fn a_selected_cli_is_absolute_even_from_a_relative_state_directory() {
        // The Source runs the selected CLI from its own working directory (the bundle's), not
        // from where `/setup` ran, so a state directory named relative to the caller must not
        // reach it as a relative path.
        let mut cfg = crate::service::Config {
            profile: "test".into(),
            port: 0,
            dsh_home: PathBuf::from("home"),
            state_dir: PathBuf::from("booth-state"),
            bundle: PathBuf::from("bundle"),
        };
        let local = installed(&cfg);
        assert!(local.is_absolute(), "{local:?}");
        assert!(local.ends_with("booth-state/bin/gh"), "{local:?}");

        // An absolute state directory already names what the Source can run.
        cfg.state_dir = PathBuf::from("/var/lib/booth");
        assert_eq!(installed(&cfg), PathBuf::from("/var/lib/booth/bin/gh"));
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

    const FIXTURE_GH: &str = "#!/bin/sh\necho fixture\n";

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("booth-gh-{name}-{}", crate::oauth::uuid_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn config(root: &Path) -> crate::service::Config {
        crate::service::Config {
            profile: "test".into(),
            port: 0,
            dsh_home: root.to_path_buf(),
            state_dir: root.join("state"),
            bundle: root.to_path_buf(),
        }
    }

    #[test]
    fn the_offline_error_names_the_command_that_installs() {
        let _env = crate::testkit::env_lock();
        let root = scratch("offline");
        // An older gh first on PATH: `/setup` can read nothing with it, and this run may not
        // download the pinned release, so the error must name the command that would.
        let shadow = root.join("path");
        fs::create_dir_all(&shadow).unwrap();
        let older = shadow.join("gh");
        fs::write(&older, "#!/bin/sh\necho '      --paginate'\n").unwrap();
        fs::set_permissions(&older, fs::Permissions::from_mode(0o755)).unwrap();
        let previous = std::env::var_os("PATH");
        let mut search = vec![shadow];
        if let Some(path) = previous.as_deref() {
            search.extend(std::env::split_paths(path));
        }
        std::env::set_var("PATH", std::env::join_paths(search).unwrap());
        let error = ensure(&config(&root), false).unwrap_err();
        match previous {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }

        // `booth gh` only reports the choice; `booth gh --install` downloads the pinned release.
        assert!(error.contains("`booth gh --install`"), "{error}");
        assert!(error.contains("bin/gh"), "{error}");
        fs::remove_dir_all(&root).ok();
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Pack the release tree the way GitHub publishes it: the top directory holds `bin/gh`.
    fn fixture(root: &Path, target: &str, archive: Archive) -> (String, Vec<u8>) {
        let top = format!("gh_{VERSION}_{target}");
        let bin = root.join(&top).join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("gh"), FIXTURE_GH).unwrap();
        fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        let asset = format!("{top}{}", archive.suffix());
        let path = root.join(&asset);
        let status = match archive {
            Archive::TarGz => Command::new("tar")
                .args(["-czf"])
                .arg(&path)
                .args(["-C"])
                .arg(root)
                .arg(&top)
                .status(),
            Archive::Zip => Command::new("zip")
                .args(["-qr"])
                .arg(&path)
                .arg(&top)
                .current_dir(root)
                .status(),
        }
        .expect("the fixture packer is installed");
        assert!(status.success(), "cannot pack the {archive:?} fixture");
        (asset, fs::read(&path).unwrap())
    }

    /// Serve the asset and its checksums over loopback, with the stop flag a test sets to end it.
    fn serve_release(
        asset: String,
        bytes: Vec<u8>,
        checksums: String,
    ) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let stop = Arc::new(AtomicBool::new(false));
        let running = stop.clone();
        let server = std::thread::spawn(move || {
            while !running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut buffer = [0u8; 2048];
                        let size = stream.read(&mut buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..size]).into_owned();
                        let path = request.split_whitespace().nth(1).unwrap_or_default();
                        // The installer asks for the asset and then the checksums; anything else
                        // is answered with the checksums, which makes a wrong URL an honest
                        // "no checksum for" failure instead of a silent success.
                        let body = if path.ends_with(&asset) {
                            bytes.clone()
                        } else {
                            checksums.clone().into_bytes()
                        };
                        let head = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        (base, stop, server)
    }

    fn set_release_base(base: &str) -> Option<std::ffi::OsString> {
        let previous = std::env::var_os("BOOTH_GH_RELEASE_BASE");
        std::env::set_var("BOOTH_GH_RELEASE_BASE", base);
        previous
    }

    fn restore_release_base(previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => std::env::set_var("BOOTH_GH_RELEASE_BASE", value),
            None => std::env::remove_var("BOOTH_GH_RELEASE_BASE"),
        }
    }

    #[test]
    fn the_published_asset_follows_the_platform() {
        assert_eq!(Archive::for_target("linux_amd64").suffix(), ".tar.gz");
        assert_eq!(Archive::for_target("linux_arm64").suffix(), ".tar.gz");
        assert_eq!(Archive::for_target("macOS_amd64").suffix(), ".zip");
        assert_eq!(Archive::for_target("macOS_arm64").suffix(), ".zip");
    }

    #[test]
    fn unpacks_the_asset_each_platform_publishes() {
        for (target, archive) in [
            ("linux_amd64", Archive::TarGz),
            ("macOS_arm64", Archive::Zip),
        ] {
            let root = scratch("unpack");
            let (asset, _) = fixture(&root, target, archive);
            let state = root.join("state");
            let member = format!("gh_{VERSION}_{target}/bin/gh");
            archive.unpack(&root.join(&asset), &member, &state).unwrap();
            assert_eq!(
                fs::read_to_string(state.join("bin").join("gh")).unwrap(),
                FIXTURE_GH
            );
            assert!(
                !state
                    .join("bin")
                    .join(format!("gh-{}.partial", std::process::id()))
                    .exists(),
                "the partial write is renamed into place"
            );
            fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn a_release_that_cannot_be_unpacked_leaves_nothing_behind() {
        let root = scratch("broken");
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        let broken = root.join("broken");
        fs::write(&broken, "not an archive").unwrap();
        let error = Archive::TarGz
            .unpack(&broken, "gh_x/bin/gh", &state)
            .unwrap_err();
        assert!(error.contains("tar could not unpack"), "{error}");
        let error = Archive::Zip
            .unpack(&broken, "gh_x/bin/gh", &state)
            .unwrap_err();
        assert!(error.contains("unzip could not unpack"), "{error}");
        assert!(!state.join("bin").join("gh").exists());
        assert!(
            fs::read_dir(state.join("bin")).unwrap().next().is_none(),
            "a failed unpack leaves no partial binary"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_binary_that_cannot_be_moved_into_place_is_reported() {
        let root = scratch("rename");
        let (asset, _) = fixture(&root, "macOS_arm64", Archive::Zip);
        let state = root.join("state");
        // A directory named like the CLI cannot be replaced by the unpacked file.
        fs::create_dir_all(state.join("bin").join("gh")).unwrap();
        let member = format!("gh_{VERSION}_macOS_arm64/bin/gh");
        let error = Archive::Zip
            .unpack(&root.join(&asset), &member, &state)
            .unwrap_err();
        assert!(error.contains("cannot move"), "{error}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn installs_the_release_for_this_platform() {
        let _env = crate::testkit::env_lock();
        let target = target().unwrap();
        let root = scratch("install");
        let (asset, bytes) = fixture(&root, target, Archive::for_target(target));
        let published = format!(
            "{}  gh_{VERSION}_other.tar.gz\n{}  {asset}\n",
            "0".repeat(64),
            sha256(&bytes)
        );
        let (base, stop, server) = serve_release(asset, bytes, published);
        let previous = set_release_base(&base);
        let result = install(&config(&root));
        restore_release_base(previous);
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();

        result.unwrap();
        let local = config(&root).state_dir.join("bin").join("gh");
        assert_eq!(fs::read_to_string(&local).unwrap(), FIXTURE_GH);
        assert_eq!(
            fs::metadata(&local).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(
            !config(&root).state_dir.join("gh-download").exists(),
            "the download scratch is removed"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_a_download_without_a_matching_checksum() {
        let _env = crate::testkit::env_lock();
        let target = target().unwrap();
        let root = scratch("mismatch");
        let (asset, bytes) = fixture(&root, target, Archive::for_target(target));
        let published = format!("{}  {asset}\n", "0".repeat(64));
        let (base, stop, server) = serve_release(asset, bytes, published);
        let previous = set_release_base(&base);
        let result = install(&config(&root));
        restore_release_base(previous);
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();

        let error = result.unwrap_err();
        assert!(
            error.contains("does not match its published checksum"),
            "{error}"
        );
        assert!(!config(&root).state_dir.join("bin").join("gh").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn names_an_asset_without_a_published_checksum() {
        let _env = crate::testkit::env_lock();
        let target = target().unwrap();
        let root = scratch("no-checksum");
        let (asset, bytes) = fixture(&root, target, Archive::for_target(target));
        let published = format!("{}  gh_{VERSION}_other.tar.gz\n", sha256(&bytes));
        let (base, stop, server) = serve_release(asset, bytes, published);
        let previous = set_release_base(&base);
        let result = install(&config(&root));
        restore_release_base(previous);
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();

        let error = result.unwrap_err();
        assert!(error.contains("no checksum for"), "{error}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_release_host_defaults_to_the_pinned_tag_and_restores_an_override() {
        let _env = crate::testkit::env_lock();
        let previous = std::env::var_os("BOOTH_GH_RELEASE_BASE");
        std::env::remove_var("BOOTH_GH_RELEASE_BASE");
        let base = release_base();
        restore_release_base(Some("http://previous".into()));
        assert_eq!(
            std::env::var("BOOTH_GH_RELEASE_BASE").as_deref(),
            Ok("http://previous")
        );
        restore_release_base(previous);
        assert_eq!(
            base,
            format!("https://github.com/cli/cli/releases/download/v{VERSION}")
        );
    }
}
