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
    let path = local.display().to_string();
    // The installed CLI is checksum-verified to be the pinned release, which has the flag, so a
    // failure here is about running it: report what actually happened instead of blaming the flag.
    match Command::new(&local).args(["api", "--help"]).output() {
        Ok(output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("--slurp") =>
        {
            Ok(Some(path))
        }
        Ok(output) if output.status.success() => Err(format!(
            "{path} was installed but does not support `gh api --slurp`"
        )),
        Ok(output) => {
            let status = output.status;
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            Err(if detail.is_empty() {
                format!("{path} was installed but `gh api --help` failed ({status})")
            } else {
                format!("{path} was installed but `gh api --help` failed ({status}): {detail}")
            })
        }
        Err(error) => Err(format!(
            "{path} was installed but cannot run: {error}{}",
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                "; the state directory must be on a filesystem that allows executables"
            } else {
                ""
            }
        )),
    }
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

    /// The tool that reads this format, named on `PATH` and in its errors.
    fn program(self) -> &'static str {
        match self {
            Self::TarGz => "tar",
            Self::Zip => "unzip",
        }
    }

    /// Unpack only `member` from `archive`, landing it at `<state>/bin/gh`.
    ///
    /// Both formats write only that member to stdout, so the CLI lands in a partial file that is
    /// renamed into place afterwards. An extraction that fails partway — a full disk, an
    /// interrupted `/setup` — therefore cannot leave a truncated CLI where the Source would run
    /// it, and a re-install cannot truncate the binary it is replacing.
    fn unpack(self, archive: &Path, member: &str, state_dir: &Path) -> Result<(), String> {
        self.unpack_with(self.program(), archive, member, state_dir)
    }

    /// [`unpack`] with the extraction tool named explicitly, so a test can drive a tool that
    /// fails without changing the process-wide `PATH`.
    fn unpack_with(
        self,
        program: &str,
        archive: &Path,
        member: &str,
        state_dir: &Path,
    ) -> Result<(), String> {
        let bin = state_dir.join("bin");
        fs::create_dir_all(&bin)
            .map_err(|error| format!("cannot create {}: {error}", bin.display()))?;
        let partial = bin.join(format!("gh-{}.partial", std::process::id()));
        let file = fs::File::create(&partial)
            .map_err(|error| format!("cannot write {}: {error}", partial.display()))?;
        let mut command = Command::new(program);
        match self {
            // `tar -O` and `unzip -p` stream the named member instead of the whole archive.
            Self::TarGz => command.args(["-xOzf"]),
            Self::Zip => command.args(["-p"]),
        };
        let output = command
            .arg(archive)
            .arg(member)
            .stdout(Stdio::from(file))
            .output();
        if !matches!(output, Ok(ref output) if output.status.success()) {
            let _ = fs::remove_file(&partial);
            let why = match output {
                // The tool's stderr is where a full disk, a corrupt archive or a missing member
                // is named; the exit status alone leaves `/setup` with nothing to report.
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let detail = stderr.trim();
                    if detail.is_empty() {
                        output.status.to_string()
                    } else {
                        format!("{}: {detail}", output.status)
                    }
                }
                Err(error) => error.to_string(),
            };
            return Err(format!(
                "{} could not unpack {member}: {why}",
                self.program()
            ));
        }
        if let Err(error) = fs::rename(&partial, bin.join("gh")) {
            let _ = fs::remove_file(&partial);
            return Err(format!(
                "cannot move {} into place: {error}",
                partial.display()
            ));
        }
        Ok(())
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

    /// Installs a loopback fixture release whose `bin/gh` is `body`, with an older `gh` first on
    /// PATH so the system CLI cannot serve the reads. The environment is restored before the
    /// result is returned, so a failing assertion cannot leak into the tests that follow.
    fn ensure_a_fixture_install(root: &Path, body: &str) -> Result<Option<String>, String> {
        let shadow = root.join("path");
        fs::create_dir_all(&shadow).unwrap();
        let older = shadow.join("gh");
        fs::write(&older, "#!/bin/sh\necho '      --paginate'\n").unwrap();
        fs::set_permissions(&older, fs::Permissions::from_mode(0o755)).unwrap();
        let previous = std::env::var_os("PATH").unwrap_or_default();
        let mut search = vec![shadow];
        search.extend(std::env::split_paths(&previous));
        std::env::set_var("PATH", std::env::join_paths(search).unwrap());

        let target = target().unwrap();
        let (asset, bytes) = fixture_with(root, target, Archive::for_target(target), body);
        let published = format!("{}  {asset}\n", sha256(&bytes));
        let (base, stop, server) = serve_release(asset, bytes, published);
        let release = set_release_base(&base);
        let result = ensure(&config(root), true);
        restore_release_base(release);
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        std::env::set_var("PATH", previous);
        result
    }

    #[test]
    fn ensure_selects_the_installed_pinned_cli() {
        let _env = crate::testkit::env_lock();
        let root = scratch("select");
        let body = "#!/bin/sh\ncase \"$1 $2\" in\n  'api --help') echo '      --slurp  use with --paginate' ;;\n  *) echo fixture ;;\nesac\n";
        let selected = ensure_a_fixture_install(&root, body).unwrap().unwrap();
        assert!(Path::new(&selected).is_absolute(), "{selected}");
        assert!(selected.ends_with("state/bin/gh"), "{selected}");
        assert!(Path::new(&selected).exists(), "{selected}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_installed_cli_that_cannot_run_reports_the_exec_error() {
        let _env = crate::testkit::env_lock();
        // A stored file that is not a program: the kernel refuses it with ENOEXEC, and there is
        // no permission advice to give.
        let root = scratch("cannot-run");
        let error = ensure_a_fixture_install(&root, "this is not an executable\n").unwrap_err();
        assert!(error.contains("was installed but cannot run"), "{error}");
        assert!(!error.contains("does not support"), "{error}");
        assert!(
            !error.contains("filesystem that allows executables"),
            "{error}"
        );
        fs::remove_dir_all(&root).ok();

        // A script whose interpreter cannot be executed: the kernel refuses it with EACCES, the
        // shape a `noexec` state directory has, so the error names the filesystem to check.
        let root = scratch("cannot-execute");
        fs::create_dir_all(root.join("interp")).unwrap();
        let body = format!("#!/{}\necho hi\n", root.join("interp").display());
        let error = ensure_a_fixture_install(&root, &body).unwrap_err();
        assert!(error.contains("was installed but cannot run"), "{error}");
        assert!(
            error.contains("filesystem that allows executables"),
            "{error}"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_installed_cli_whose_help_fails_reports_the_helper_output() {
        let _env = crate::testkit::env_lock();
        // The CLI runs but its help fails: its stderr is the explanation to surface, and an empty
        // stderr still reports the status instead of inventing a detail.
        for (index, (body, detail)) in [
            (
                "#!/bin/sh\necho 'help is broken' >&2\nexit 3\n",
                Some("help is broken"),
            ),
            ("#!/bin/sh\nexit 3\n", None),
        ]
        .into_iter()
        .enumerate()
        {
            let root = scratch(&format!("help-fails-{index}"));
            let error = ensure_a_fixture_install(&root, body).unwrap_err();
            assert!(error.contains("`gh api --help` failed"), "{error}");
            match detail {
                Some(detail) => assert!(error.contains(detail), "{error}"),
                None => assert!(error.ends_with("(exit status: 3)"), "{error}"),
            }
            fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn an_installed_cli_without_the_flag_is_still_named() {
        let _env = crate::testkit::env_lock();
        let root = scratch("no-flag");
        // The one case the old message was true for: help succeeds but has no `--slurp`.
        let error = ensure_a_fixture_install(&root, "#!/bin/sh\necho 'no such flag'\nexit 0\n")
            .unwrap_err();
        assert!(
            error.contains("does not support `gh api --slurp`"),
            "{error}"
        );
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
        fixture_with(root, target, archive, FIXTURE_GH)
    }

    /// The same fixture with `body` as the published CLI, for a release that cannot do the job.
    fn fixture_with(root: &Path, target: &str, archive: Archive, body: &str) -> (String, Vec<u8>) {
        let top = format!("gh_{VERSION}_{target}");
        let bin = root.join(&top).join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("gh"), body).unwrap();
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
    fn a_failed_unpack_reports_the_tools_stderr() {
        // The published checksum covers the download, but not the extraction: a full disk or a
        // corrupt archive makes the tool stop after it started. Its stderr is the only place that
        // says why, so it has to survive into the error.
        let root = scratch("unpack-detail");
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        let broken = root.join("broken");
        fs::write(&broken, "not an archive").unwrap();
        // `head` rejects the extraction flags, so its stderr is the explanation the error must
        // carry. Ask it the same question here instead of guessing its wording.
        let expected = Command::new("head")
            .args(["-xOzf"])
            .arg(&broken)
            .arg("gh_x/bin/gh")
            .output()
            .expect("head is available");
        assert!(
            !expected.status.success(),
            "head refuses the extraction flags"
        );
        let stderr = String::from_utf8_lossy(&expected.stderr);
        let detail = stderr.trim();
        assert!(!detail.is_empty(), "head explains why it refused");

        let error = Archive::TarGz
            .unpack_with("head", &broken, "gh_x/bin/gh", &state)
            .unwrap_err();
        assert!(error.contains("tar could not unpack"), "{error}");
        assert!(error.contains(detail), "{error}");
        assert!(!state.join("bin").join("gh").exists());
        assert!(
            fs::read_dir(state.join("bin")).unwrap().next().is_none(),
            "a failed unpack leaves no partial binary"
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_unpack_failure_without_stderr_keeps_the_status() {
        let root = scratch("unpack-no-stderr");
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        let broken = root.join("broken");
        fs::write(&broken, "not an archive").unwrap();
        let error = Archive::TarGz
            .unpack_with("false", &broken, "gh_x/bin/gh", &state)
            .unwrap_err();

        assert!(error.contains("tar could not unpack"), "{error}");
        assert!(error.contains("exit status: 1"), "{error}");
        assert!(!state.join("bin").join("gh").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_extractor_that_cannot_start_is_named_with_the_os_error() {
        let root = scratch("unpack-missing-tool");
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        let broken = root.join("broken");
        fs::write(&broken, "not an archive").unwrap();
        let error = Archive::TarGz
            .unpack_with("booth-missing-extractor", &broken, "gh_x/bin/gh", &state)
            .unwrap_err();

        assert!(error.contains("tar could not unpack"), "{error}");
        assert!(
            error.contains("No such file") || error.contains("os error 2"),
            "{error}"
        );
        assert!(!state.join("bin").join("gh").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_truncated_tar_archive_leaves_no_half_written_cli() {
        // The published checksum rules out a bad download, but not an extraction that stops
        // partway: a full disk or an interrupted `/setup` makes `tar` fail after it has written
        // part of the member. That truncated file must not stay where the pinned CLI belongs.
        let root = scratch("truncated");
        let target = "linux_amd64";
        let top = format!("gh_{VERSION}_{target}");
        let bin = root.join(&top).join("bin");
        fs::create_dir_all(&bin).unwrap();
        // Incompressible bytes, so halving the archive cuts through the member itself.
        let mut seed = 0x1234_5678u32;
        let body: Vec<u8> = (0..(1 << 20))
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 24) as u8
            })
            .collect();
        fs::write(bin.join("gh"), &body).unwrap();
        let archive = root.join(format!("{top}.tar.gz"));
        let packed = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&root)
            .arg(&top)
            .status()
            .unwrap();
        assert!(packed.success(), "cannot pack the fixture");
        let whole = fs::read(&archive).unwrap();
        fs::write(&archive, &whole[..whole.len() / 2]).unwrap();

        let state = root.join("state");
        let member = format!("gh_{VERSION}_{target}/bin/gh");
        let error = Archive::TarGz
            .unpack(&archive, &member, &state)
            .unwrap_err();
        assert!(error.contains("tar could not unpack"), "{error}");
        assert!(!state.join("bin").join("gh").exists(), "{error}");
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
        assert!(
            !state
                .join("bin")
                .join(format!("gh-{}.partial", std::process::id()))
                .exists(),
            "a failed move leaves no partial binary"
        );
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
