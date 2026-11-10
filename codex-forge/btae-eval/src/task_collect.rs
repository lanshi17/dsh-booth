//! Plan-bound isolated task collection; engineering evidence only.
use crate::PlannedRun;
use crate::dsh_live::LiveLimits;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub struct TaskRequest {
    pub manifest: PathBuf,
    pub task_id: String,
    pub arm: String,
    pub repeat: u32,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub bundle: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub share_network: bool,
    pub output: PathBuf,
    pub cancel_file: Option<PathBuf>,
    pub limits: LiveLimits,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub schema_version: u32,
    pub data_kind: String,
    pub manifest_sha256: String,
    pub run: PlannedRun,
    pub network: String,
    pub outcome: String,
    pub initial_files_sha256: BTreeMap<String, String>,
    pub files_sha256: BTreeMap<String, String>,
    pub hidden_test_result: Option<bool>,
    pub empirical_task_success: Option<bool>,
    pub billing_verified: bool,
}

use crate::Manifest;
use crate::digest;
use crate::dsh_live::run_live;
use crate::plan;
use crate::runner::export_seed;
use crate::runner::inventory;
use crate::runner::snapshot;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io::Write;
use std::io::{self};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Component;

fn invalid(message: &str) -> Box<dyn Error + Send + Sync> {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    serde_json::to_writer_pretty(&mut f, value)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
fn bounded_inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut pending = vec![root.to_path_buf()];
    let mut bytes = 0u64;
    let mut entries = 0;
    while let Some(path) = pending.pop() {
        let meta = fs::symlink_metadata(&path)?;
        entries += 1;
        if entries > 4096 {
            return Err(invalid("too many collected entries"));
        }
        if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if meta.is_file() {
            bytes = bytes
                .checked_add(meta.len())
                .ok_or_else(|| invalid("artifact size overflow"))?;
            if bytes > 64 * 1024 * 1024 {
                return Err(invalid("collected artifacts exceed 64 MiB"));
            }
        } else {
            return Err(invalid("symlink or special file in collected artifacts"));
        }
    }
    inventory(root)
}
fn task_context(
    manifest_path: &Path,
    task: &str,
    arm: &str,
    repeat: u32,
) -> Result<(PathBuf, Manifest, PlannedRun, String)> {
    let path = manifest_path.canonicalize()?;
    let bytes = fs::read(&path)?;
    let m: Manifest = serde_json::from_slice(&bytes)?;
    let schedule = plan(&m, &bytes)?;
    let run = schedule
        .planned_runs
        .into_iter()
        .find(|r| r.task_id == task && r.arm == arm && r.repeat == repeat)
        .ok_or_else(|| invalid("task/arm/repeat is not planned"))?;
    if run.round != 1 {
        return Err(invalid(
            "initial-round task export only; no trajectory reset",
        ));
    }
    let root = path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid("expected datasets/manifest"))?
        .to_path_buf();
    Ok((root, m, run, schedule.manifest_sha256))
}
fn public_input(root: &Path, m: &Manifest, run: &PlannedRun) -> Result<serde_json::Value> {
    let task = m
        .tasks
        .iter()
        .find(|t| t.id == run.task_id)
        .ok_or_else(|| invalid("missing task"))?;
    Ok(
        serde_json::json!({"data_kind":"dsh_task_engineering_only","run":run,"request":task.request,
        "arm_instructions":fs::read_to_string(root.join(format!("baselines/{}.md",run.arm)))?}),
    )
}
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
fn checked_output(req: &TaskRequest, root: &Path, exe: &Path) -> Result<PathBuf> {
    let parent = req
        .output
        .parent()
        .ok_or_else(|| invalid("output parent missing"))?
        .canonicalize()?;
    let name = req
        .output
        .file_name()
        .ok_or_else(|| invalid("output name missing"))?;
    if !matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
    ) {
        return Err(invalid("invalid output name"));
    }
    let output = parent.join(name);
    if root.starts_with(&output) || overlaps(&output, exe) {
        return Err(invalid("output overlap with source/deployment"));
    }
    for folder in ["datasets", "repositories", "baselines", "evaluator-private"] {
        if overlaps(&output, &root.join(folder)) {
            return Err(invalid("output overlap with public or private source"));
        }
    }
    for source in [&req.bundle, &req.profile].into_iter().flatten() {
        if overlaps(&output, &source.canonicalize()?) {
            return Err(invalid("output overlap with deployment"));
        }
    }
    Ok(output)
}
fn profile_files(profile: Option<&Path>) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    if let Some(profile) = profile {
        if !fs::symlink_metadata(profile)?.is_dir() {
            return Err(invalid("profile must be a regular directory"));
        }
        for name in ["cordis.yml", "cordis.patch.yml"] {
            let path = profile.join(name);
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.len() > 1024 * 1024 {
                return Err(invalid("profile file must be regular and at most 1 MiB"));
            }
            files.insert(name.into(), fs::read(path)?);
        }
    }
    Ok(files)
}
fn child_environment(extra: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::from([
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("HOME".into(), "/tmp".into()),
        ("LANG".into(), "C.UTF-8".into()),
        ("DSH_HOME".into(), "/state".into()),
        ("DSH_AGENTS_HOME".into(), "/tmp/agents".into()),
        ("DSH_TELEMETRY_DISABLED".into(), "1".into()),
    ]);
    for (key, value) in extra {
        if matches!(key.as_str(), "HOME" | "PATH" | "LANG") || key.starts_with("DSH_") {
            return Err(invalid("reserved environment key"));
        }
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || value.contains('\0')
        {
            return Err(invalid("invalid environment entry"));
        }
        env.insert(key.clone(), value.clone());
    }
    Ok(env)
}
fn launcher_args(
    output: &Path,
    exe: &Path,
    bundle: Option<&Path>,
    args: &[String],
    network: bool,
) -> Result<Vec<String>> {
    let mut a: Vec<String> = ["--unshare-all", "--die-with-parent", "--new-session"]
        .into_iter()
        .map(String::from)
        .collect();
    if network {
        a.push("--share-net".into());
    }
    for p in ["/usr", "/lib", "/lib64"] {
        a.extend(["--ro-bind".into(), p.into(), p.into()]);
    }
    if network {
        for p in ["/etc/ssl", "/etc/resolv.conf", "/etc/hosts"] {
            a.extend(["--ro-bind".into(), p.into(), p.into()]);
        }
    }
    a.extend(
        [
            "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--dir", "/input",
        ]
        .into_iter()
        .map(String::from),
    );
    for (option, source, target) in [
        ("--ro-bind", exe.to_path_buf(), "/app/node"),
        (
            "--ro-bind",
            output.join("public-input.json"),
            "/input/task.json",
        ),
        ("--bind", output.join("workspace"), "/workspace"),
        ("--bind", output.join("state"), "/state"),
    ] {
        a.extend([
            option.into(),
            source
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 launcher path"))?
                .into(),
            target.into(),
        ]);
    }
    if let Some(bundle) = bundle {
        a.extend([
            "--ro-bind".into(),
            bundle
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 bundle path"))?
                .into(),
            "/app/node_modules".into(),
        ]);
    }
    a.extend(
        ["--chdir", "/workspace", "--", "/app/node"]
            .into_iter()
            .map(String::from),
    );
    if args.is_empty() {
        a.extend(
            ["--profile", "headless", "--json", "-"]
                .into_iter()
                .map(String::from),
        );
    } else {
        a.extend(args.iter().cloned());
    }
    Ok(a)
}

/// Engineering-only first-round collection. Deployment/profile inputs are trusted;
/// all child-writable artifacts are private, may contain model/session data, and
/// must not be published. No fallback if Bubblewrap cannot establish isolation.
pub fn collect_task(req: TaskRequest) -> Result<TaskRecord> {
    let (root, m, run, manifest_hash) =
        task_context(&req.manifest, &req.task_id, &req.arm, req.repeat)?;
    let exe = req.executable.canonicalize()?;
    if !exe.is_file() {
        return Err(invalid("executable is not a regular file"));
    }
    let output = checked_output(&req, &root, &exe)?;
    let profile = profile_files(req.profile.as_deref())?;
    let env = child_environment(&req.environment)?;
    let bundle = req.bundle.as_ref().map(|p| p.canonicalize()).transpose()?;
    if let Some(bundle) = &bundle {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok());
        if !bundle.is_dir()
            || root.starts_with(bundle)
            || home.as_ref().is_some_and(|h| h.starts_with(bundle))
        {
            return Err(invalid(
                "bundle must be a dedicated code directory, not project/home or ancestor",
            ));
        }
    }
    let input = public_input(&root, &m, &run)?;
    let prompt = serde_json::to_vec_pretty(&input)?;
    if prompt.len() > crate::dsh_live::MAX_PROMPT_BYTES {
        return Err(invalid("public prompt exceeds collector limit"));
    }
    if req.limits.wall_ms > m.budget.wall_seconds.saturating_mul(1000) {
        return Err(invalid("process deadline exceeds study budget"));
    }
    let network = if req.share_network {
        "shared_engineering_only"
    } else {
        "disabled"
    };
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    write_json(
        &output.join("started.json"),
        &serde_json::json!({"data_kind":"dsh_task_engineering_only",
        "manifest_sha256":manifest_hash,"run":run,"network":network,
        "deployment_sha256":digest(&fs::read(&exe)?),"prompt_sha256":digest(&prompt),
        "note":"Missing task.json means incomplete setup/execution/sealing; do not infer task success."}),
    )?;
    let workspace = output.join("workspace");
    fs::create_dir(&workspace)?;
    export_seed(&root, &m, &run.repository, &workspace)?;
    let initial_files = bounded_inventory(&workspace)?;
    write_json(&output.join("public-input.json"), &input)?;
    let state = output.join("state");
    fs::DirBuilder::new().mode(0o700).create(&state)?;
    if !profile.is_empty() {
        let dir = state.join("profiles/headless");
        fs::create_dir_all(&dir)?;
        for (name, bytes) in profile {
            fs::write(dir.join(name), bytes)?;
        }
    }
    let args = launcher_args(
        &output,
        &exe,
        bundle.as_deref(),
        &req.args,
        req.share_network,
    )?;
    let process = run_live(crate::dsh_live::LiveRequest {
        executable: "/usr/bin/bwrap".into(),
        args,
        prompt,
        cwd: None,
        environment: env,
        cancel_file: req.cancel_file,
        limits: req.limits,
        output: Some(output.join("process")),
    })?;
    bounded_inventory(&workspace)?;
    snapshot(&workspace, &output.join("workspace-snapshot"))?;
    let record = TaskRecord {
        schema_version: 1,
        data_kind: "dsh_task_engineering_only".into(),
        manifest_sha256: manifest_hash,
        run,
        network: network.into(),
        outcome: serde_json::to_value(process.outcome)?
            .as_str()
            .ok_or_else(|| invalid("invalid process outcome"))?
            .into(),
        initial_files_sha256: initial_files,
        files_sha256: bounded_inventory(&output)?,
        hidden_test_result: None,
        empirical_task_success: None,
        billing_verified: false,
    };
    write_json(&output.join("task.json"), &record)?;
    Ok(record)
}

/// Recompute evidence bindings on a trusted host. This is not a signed receipt
/// and cannot stop a host attacker from rewriting all artifact hashes.
pub fn verify_task(directory: &Path, manifest: &Path) -> Result<TaskRecord> {
    let record: TaskRecord = serde_json::from_slice(&fs::read(directory.join("task.json"))?)?;
    let (root, m, run, hash) = task_context(
        manifest,
        &record.run.task_id,
        &record.run.arm,
        record.run.repeat,
    )?;
    if record.schema_version != 1
        || record.data_kind != "dsh_task_engineering_only"
        || record.manifest_sha256 != hash
        || record.run != run
        || record.hidden_test_result.is_some()
        || record.empirical_task_success.is_some()
        || record.billing_verified
        || !matches!(
            record.network.as_str(),
            "disabled" | "shared_engineering_only"
        )
    {
        return Err(invalid(
            "task identity, plan, network or research fields invalid",
        ));
    }
    let mut actual = bounded_inventory(directory)?;
    actual.remove("task.json");
    if actual != record.files_sha256 {
        return Err(invalid("task artifact inventory mismatch"));
    }
    let expected_input = public_input(&root, &m, &run)?;
    let input: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("public-input.json"))?)?;
    if input != expected_input {
        return Err(invalid(
            "task request/SOP does not match frozen public input",
        ));
    }
    let seed = m
        .repositories
        .iter()
        .find(|r| r.id == run.repository)
        .ok_or_else(|| invalid("missing seed"))?;
    let pairs: Vec<_> = record.initial_files_sha256.iter().collect();
    if digest(&serde_json::to_vec(&pairs)?) != seed.snapshot_sha256 {
        return Err(invalid("initial snapshot does not match planned seed"));
    }
    if bounded_inventory(&directory.join("workspace"))?
        != bounded_inventory(&directory.join("workspace-snapshot"))?
    {
        return Err(invalid("sealed workspace differs from final snapshot"));
    }
    let process: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("process/record.json"))?)?;
    let started: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("started.json"))?)?;
    if process["data_kind"] != "dsh_process_smoke_only"
        || process["outcome"] != record.outcome
        || process["billing_verified"] != false
        || !process["hidden_test_result"].is_null()
        || !process["empirical_task_success"].is_null()
        || started["run"] != serde_json::to_value(&run)?
        || started["manifest_sha256"] != hash
        || started["network"] != record.network
        || started["prompt_sha256"] != digest(&serde_json::to_vec_pretty(&input)?)
        || started["prompt_sha256"] != process["prompt_sha256"]
    {
        return Err(invalid("process and started receipts inconsistent"));
    }
    Ok(record)
}
