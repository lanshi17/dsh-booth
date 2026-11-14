//! Terminal setup for the Feishu (Lark) bot: the app credentials, the bot-only `lark-cli`, and
//! the Feishu Source that carries messages into the portal.
//!
//! `dsh-superagent` never talks to Feishu itself. It runs a `lark-cli` that acts only as the
//! app's bot: the CLI consumes `im.message.receive_v1` over the long connection, reads and
//! replies in chats, and refuses the user identity. `booth feishu login` writes that
//! configuration where the service can reach it, points the Source at a wrapper, and turns the
//! Source on. The App Secret goes to `lark-cli` on standard input and is never printed.

use crate::api::{Api, ApiError};
use crate::service;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The official CLI. `dsh-superagent`'s deployment guide was written against 1.0.90; this is
/// the release booth installs and checks, whose subcommands the bundle uses.
const LARK_CLI: &str = "@larksuite/cli";
const LARK_VERSION: &str = "1.0.100";
/// The bundle's Feishu plugin; its own layer leaves it off until a bot is configured.
const PLUGIN: &str = "dsh-superagent-feishu";
const SOURCE: &str = "feishu";
/// A settings save restarts the plugin, so reads and writes wait out a restart.
const RESTART_TIMEOUT: Duration = Duration::from_secs(90);

/// Configure the app, the bot-only CLI and the Source, then allow the named senders.
pub fn login(
    cfg: &service::Config,
    api: &Api,
    app_id: &str,
    app_secret: &str,
    open_ids: &[String],
) -> Result<(), String> {
    let app_id = valid_app_id(app_id)?;
    let app_secret = validate_app_secret(app_secret)?;
    service::create_private_state_dir(cfg)?;

    let lark = ensure_cli(cfg)?;
    let config_dir = cfg.state_dir.join("lark-cli");
    configure(&lark, &config_dir, &app_id, &app_secret)?;
    let wrapper = write_wrapper(cfg, &lark, &config_dir)?;
    let bot = bot_of(&wrapper, &app_id)?;
    println!("飞书应用 {app_id} · 机器人 {} ({})", bot.name, bot.open_id);
    println!("lark-cli  {}", wrapper.display());

    // The up instance is composed from the profile layer read at start; enabling the plugin in
    // that layer takes a restart. Starting a stopped service already loads the new layer.
    let changed = enable_plugin(cfg)?;
    let mut started = false;
    if api.state().is_err() {
        println!("服务未运行，正在启动……");
        service::start(cfg)?;
        started = true;
    }
    if changed && !started || api.source_block(SOURCE).is_err() {
        println!("正在重启服务以加载飞书 Source……");
        service::restart(cfg)?;
        crate::intake::wait_ready(api)?;
    }

    apply_source(api, &wrapper)?;
    for open_id in open_ids {
        allow(api, open_id)?;
    }
    match wait_feishu(api, Instant::now() + RESTART_TIMEOUT) {
        Some(problem) => println!("! 飞书 Source 尚未就绪：{}", problem.detail),
        None => println!("✓ 飞书 Source 已接入"),
    }
    if let Some(hint) = permission_hint(&wrapper) {
        println!("! 应用权限不足：{hint}");
    }
    let unsubscribed = event_hint(&wrapper);
    if let Some(hint) = &unsubscribed {
        println!(
            "! 事件未订阅：在开发者后台「事件与回调」选择长连接并添加 im.message.receive_v1，\
             发布应用版本，或打开 {hint}"
        );
    }
    if unsubscribed.is_some() {
        println!("完成订阅并发布版本后，私聊机器人或在群里 @ 它即可开始。");
    } else {
        println!(
            "在飞书里私聊机器人即可。其他人先运行 `booth feishu allow <ou_…>`；\n\
             群里 @ 机器人需先运行 `booth feishu chat <oc_…>`（“全部消息”加 --all）。"
        );
    }
    Ok(())
}

/// Allow one Feishu sender to originate work without discarding other Policy settings.
pub fn allow(api: &Api, open_id: &str) -> Result<(), String> {
    let open_id = open_id.trim();
    if !id(open_id, "ou_") {
        return Err("usage: booth feishu allow <ou_… open_id>".into());
    }
    retry_api("enable the Feishu Source", || enable_source(api))?;
    retry_api("update the Feishu allow-list", || {
        api.set("policy", &|policy| allow_in_policy(policy, open_id))
            .map(|_| ())
    })?;
    println!("已允许飞书用户 {open_id} 联系 Portal。");
    Ok(())
}

/// Receive messages from a group chat, in addition to direct messages.
pub fn chat(api: &Api, chat_id: &str, all: bool) -> Result<(), String> {
    let chat_id = chat_id.trim();
    if !id(chat_id, "oc_") {
        return Err("usage: booth feishu chat <oc_… chat_id> [--all]".into());
    }
    retry_api("enable the Feishu Source", || enable_source(api))?;
    retry_api("add the Feishu chat", || {
        change_source(api, &|source| listen_in_source(source, chat_id, all))
    })?;
    println!(
        "已接收群 {chat_id} 的消息（{}）。",
        if all { "全部消息" } else { "仅 @" }
    );
    Ok(())
}

/// The managed CLI, `lark-cli` on `PATH`, or a fresh install under the state directory.
fn ensure_cli(cfg: &service::Config) -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("BOOTH_LARK_CLI") {
        let path = PathBuf::from(value);
        return executable(&path)
            .ok_or_else(|| format!("BOOTH_LARK_CLI={} is not an executable", path.display()));
    }
    let managed = cfg
        .state_dir
        .join("tools")
        .join("node_modules")
        .join(".bin")
        .join("lark-cli");
    if executable(&managed).is_some() {
        return Ok(managed);
    }
    if let Some(path) = on_path("lark-cli") {
        return Ok(path);
    }
    install(cfg)
}

/// Install the CLI beside the state, so booth owns the version the bundle was checked against.
fn install(cfg: &service::Config) -> Result<PathBuf, String> {
    let tools = cfg.state_dir.join("tools");
    fs::create_dir_all(&tools)
        .map_err(|error| format!("cannot create {}: {error}", tools.display()))?;
    let manifest = tools.join("package.json");
    if !manifest.exists() {
        fs::write(
            &manifest,
            "{\n  \"name\": \"booth-tools\",\n  \"private\": true\n}\n",
        )
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))?;
    }
    println!("installing {LARK_CLI}@{LARK_VERSION} (the bot's CLI)");
    let output = Command::new("npm")
        .args([
            "install",
            "--no-audit",
            "--no-fund",
            "--no-save",
            &format!("{LARK_CLI}@{LARK_VERSION}"),
        ])
        .current_dir(&tools)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            format!(
                "cannot run npm ({error}); install Node.js, or set BOOTH_LARK_CLI to a lark-cli"
            )
        })?;
    if !output.status.success() {
        return Err(format!("cannot install {LARK_CLI}: {}", detail(&output)));
    }
    let binary = tools.join("node_modules").join(".bin").join("lark-cli");
    executable(&binary).ok_or_else(|| {
        format!(
            "npm installed {LARK_CLI} but {} is missing",
            binary.display()
        )
    })
}

/// Save the app in its own configuration directory and restrict the CLI to the bot identity.
///
/// A separate directory is required: `lark-cli` allows one profile per app id, and an operator's
/// own configuration may already hold a user login for this app. `--force-init` lets the save
/// happen inside an agent workspace, where the CLI otherwise insists on binding to its app.
fn configure(lark: &Path, config_dir: &Path, app_id: &str, secret: &str) -> Result<(), String> {
    create_private_dir(config_dir)?;
    let init = run(
        lark,
        &[
            "config",
            "init",
            "--app-id",
            app_id,
            "--app-secret-stdin",
            "--brand",
            "feishu",
            "--force-init",
        ],
        Some(config_dir),
        Some(secret.as_bytes()),
    )?;
    if !init.status.success() {
        return Err(format!("lark-cli config init failed: {}", detail(&init)));
    }
    // The profile setting, not `--global`: the host runs `config strict-mode` without flags and
    // refuses a CLI that can act as a user.
    let mode = run(
        lark,
        &["config", "strict-mode", "bot"],
        Some(config_dir),
        None,
    )?;
    if !mode.status.success() {
        return Err(format!(
            "lark-cli could not restrict itself to the bot identity: {}",
            detail(&mode)
        ));
    }
    let shown = run(lark, &["config", "strict-mode"], Some(config_dir), None)?;
    let text = String::from_utf8_lossy(&shown.stdout);
    if !text.contains("strict-mode: bot") {
        return Err(format!(
            "lark-cli did not keep the bot-only identity: {}",
            text.trim()
        ));
    }
    Ok(())
}

/// The bot identity the app answers with, read through the wrapper the service will run.
fn bot_of(wrapper: &Path, app_id: &str) -> Result<Bot, String> {
    let status = run(wrapper, &["auth", "status"], None, None)?;
    if !status.status.success() {
        return Err(format!("lark-cli auth status failed: {}", detail(&status)));
    }
    let text = String::from_utf8_lossy(&status.stdout);
    let identity: BotIdentity = parse_json(&text)
        .ok_or_else(|| format!("lark-cli auth status printed no JSON: {}", text.trim()))
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|error| format!("cannot read lark-cli auth status: {error}"))
        })?;
    if identity.app_id != app_id {
        return Err(format!(
            "lark-cli is configured for {}, not {app_id}",
            identity.app_id
        ));
    }
    if identity.identities.bot.status != "ready" {
        return Err(format!(
            "the bot identity of {app_id} is {}; check the App ID and App Secret",
            identity.identities.bot.status
        ));
    }
    let info = run(
        wrapper,
        &[
            "api",
            "GET",
            "/open-apis/bot/v3/info",
            "--as",
            "bot",
            "--format",
            "ndjson",
        ],
        None,
        None,
    )?;
    if !info.status.success() {
        return Err(format!(
            "Feishu refused the app credentials: {}",
            detail(&info)
        ));
    }
    let text = String::from_utf8_lossy(&info.stdout);
    let line = text.lines().next().unwrap_or_default();
    let info: BotInfo = serde_json::from_str(line)
        .map_err(|error| format!("cannot read the bot's identity: {error}"))?;
    Ok(Bot {
        open_id: info.bot.open_id,
        name: info.bot.app_name,
    })
}

/// Write the wrapper the Source and its workers run as `lark-cli`.
fn write_wrapper(cfg: &service::Config, lark: &Path, config_dir: &Path) -> Result<PathBuf, String> {
    let bin = cfg.state_dir.join("bin");
    fs::create_dir_all(&bin)
        .map_err(|error| format!("cannot create {}: {error}", bin.display()))?;
    let path = bin.join("lark-bot");
    let lark = std::path::absolute(lark)
        .map_err(|error| format!("cannot resolve {}: {error}", lark.display()))?;
    let config_dir = std::path::absolute(config_dir)
        .map_err(|error| format!("cannot resolve {}: {error}", config_dir.display()))?;
    let text = format!(
        "#!/bin/sh\n\
         # Written by booth for the Feishu Source; `booth feishu login` rewrites it.\n\
         export LARKSUITE_CLI_CONFIG_DIR={}\n\
         export LARKSUITE_CLI_NO_UPDATE_NOTIFIER=1\n\
         export LARKSUITE_CLI_NO_SKILLS_NOTIFIER=1\n\
         exec {} \"$@\"\n",
        quoted(&config_dir),
        quoted(&lark)
    );
    write_atomic(&path, text.as_bytes(), 0o755)?;
    Ok(path)
}

/// Turn the bundle's Feishu plugin on in the profile layer; returns whether the layer changed.
///
/// The entry is merged with the bundle's by id, so it needs no plugin name and no configuration.
fn enable_plugin(cfg: &service::Config) -> Result<bool, String> {
    let mut patch = service::Patch::read(cfg)?;
    let enabled = patch
        .get(PLUGIN)
        .and_then(|entry| entry.get("disabled"))
        .and_then(Value::as_bool)
        == Some(false);
    if enabled {
        return Ok(false);
    }
    patch.set_enabled(PLUGIN, true);
    patch.write(cfg)?;
    Ok(true)
}

/// Point the Source at the wrapper, keep its other settings, and turn it on.
fn apply_source(api: &Api, cli: &Path) -> Result<(), String> {
    let deadline = Instant::now() + RESTART_TIMEOUT;
    loop {
        let (mut source, revision) = match api.source_block(SOURCE) {
            Ok(found) => found,
            Err(error) if retryable(&error) && Instant::now() < deadline => {
                sleep(Duration::from_millis(600));
                continue;
            }
            Err(error) => return Err(format!("cannot read the Feishu Source: {error}")),
        };
        source["enabled"] = json!(true);
        if !source["config"].is_object() {
            source["config"] = json!({});
        }
        source["config"]["cli"] = json!(cli.to_string_lossy());
        match api.save(&format!("source:{SOURCE}"), &source, revision) {
            Ok(_) => return crate::intake::wait_ready(api),
            Err(error) if retryable(&error) && Instant::now() < deadline => {
                sleep(Duration::from_millis(600));
            }
            Err(error) => return Err(format!("cannot save the Feishu Source: {error}")),
        }
    }
}

fn enable_source(api: &Api) -> Result<(), ApiError> {
    let (mut source, revision) = api.source_block(SOURCE)?;
    if source.get("enabled").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    source["enabled"] = json!(true);
    api.save(&format!("source:{SOURCE}"), &source, revision)
        .map(|_| ())
}

fn change_source(api: &Api, change: &dyn Fn(&mut Value)) -> Result<(), ApiError> {
    let (mut source, revision) = api.source_block(SOURCE)?;
    change(&mut source);
    api.save(&format!("source:{SOURCE}"), &source, revision)
        .map(|_| ())
}

fn retry_api<T>(action: &str, mut run: impl FnMut() -> Result<T, ApiError>) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match run() {
            Ok(value) => return Ok(value),
            Err(error) if retryable(&error) && Instant::now() < deadline => {
                sleep(Duration::from_millis(600));
            }
            Err(error) => {
                return Err(format!(
                    "cannot {action}: {error}{}",
                    missing_source(&error)
                ))
            }
        }
    }
}

/// The Source is registered by a plugin the profile layer has to enable; name the command.
fn missing_source(error: &ApiError) -> &'static str {
    match error {
        ApiError::Protocol(detail) if detail.starts_with("no source") => {
            " — run `booth feishu login` first"
        }
        _ => "",
    }
}

fn retryable(error: &ApiError) -> bool {
    match error {
        ApiError::Transport(_) => true,
        ApiError::Status(code, _) => *code == 409 || *code >= 500,
        ApiError::Protocol(detail) => detail.starts_with("no source"),
    }
}

/// Whether the plugin resolved the bot after a restart; the deadline bounds an unresolved setup.
fn wait_feishu(api: &Api, deadline: Instant) -> Option<crate::model::SetupProblem> {
    loop {
        if let Ok(state) = api.state() {
            if state.setup.accounts.is_some() || state.setup.problem.is_some() {
                return state.setup.feishu_problem;
            }
        }
        if Instant::now() > deadline {
            return None;
        }
        sleep(Duration::from_millis(500));
    }
}

/// The missing scopes a bot needs to read chats, as Feishu's own console names them.
///
/// The check is advisory: the app may be configured correctly while permissions are still
/// pending in the developer console, and the Source reports its own health either way.
fn permission_hint(wrapper: &Path) -> Option<String> {
    let output = run(
        wrapper,
        &["im", "+chat-list", "--page-all", "--page-size", "100"],
        None,
        None,
    )
    .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value = parse_json(&stdout).or_else(|| parse_json(&stderr))?;
    if value.get("ok").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    let error = value.get("error")?;
    let scopes = error
        .get("missing_scopes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|scopes| !scopes.is_empty());
    let url = error.get("console_url").and_then(Value::as_str);
    Some(match (scopes, url) {
        (Some(scopes), Some(url)) => format!("申请 {scopes} 并发布应用版本：{url}"),
        (Some(scopes), None) => format!("需要申请 {scopes}"),
        _ => error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("飞书拒绝了聊天列表读取")
            .to_owned(),
    })
}

/// The event type the Source consumes; until the console publishes it, no message ever arrives.
///
/// `event consume --dry-run` reports each precondition without starting a second consumer, so
/// this checks the subscription without taking an event from the running service.
fn event_hint(wrapper: &Path) -> Option<String> {
    let output = run(
        wrapper,
        &[
            "event",
            "consume",
            "im.message.receive_v1",
            "--as",
            "bot",
            "--dry-run",
        ],
        None,
        None,
    )
    .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value = parse_json(&stdout).or_else(|| parse_json(&stderr))?;
    let conditions = value.pointer("/data/decision/preconditions")?.as_array()?;
    let published = conditions.iter().find(|condition| {
        condition.get("name").and_then(Value::as_str) == Some("console_event_published")
    })?;
    if published.get("status").and_then(Value::as_str) != Some("blocked") {
        return None;
    }
    Some(
        published
            .get("hint")
            .and_then(Value::as_str)
            .unwrap_or("在开发者后台订阅 im.message.receive_v1（长连接）")
            .to_owned(),
    )
}

fn allow_in_policy(policy: &mut Value, open_id: &str) {
    if policy.pointer("/sources/feishu").is_none() {
        policy["sources"]["feishu"] = json!({"allowed": []});
    }
    let allowed = policy["sources"]["feishu"]["allowed"]
        .as_array_mut()
        .expect("feishu allowed list is an array");
    if !allowed
        .iter()
        .any(|entry| entry.get("id").and_then(Value::as_str) == Some(open_id))
    {
        allowed.push(json!({"id": open_id}));
    }
}

fn listen_in_source(source: &mut Value, chat_id: &str, all: bool) {
    if !source["config"].is_object() {
        source["config"] = json!({});
    }
    let config = &mut source["config"];
    if !config["chats"].is_array() {
        config["chats"] = json!([]);
    }
    let listen = if all { "all" } else { "mentions" };
    let chats = config["chats"]
        .as_array_mut()
        .expect("feishu chats is an array");
    match chats
        .iter_mut()
        .find(|chat| chat.get("chatId").and_then(Value::as_str) == Some(chat_id))
    {
        Some(chat) => chat["listen"] = json!(listen),
        None => chats.push(json!({"chatId": chat_id, "listen": listen})),
    }
}

/// `cli_` plus the characters the bundle's schema accepts, so a typo fails here, not in a save.
fn valid_app_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if id(value, "cli_") {
        Ok(value.to_owned())
    } else {
        Err(format!(
            "{value:?} is not a Feishu App ID; it starts with `cli_`"
        ))
    }
}

fn validate_app_secret(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("the App Secret is empty".into());
    }
    if value.chars().any(|c| c == '\n' || c == '\r') {
        return Err("the App Secret must not contain a line break".into());
    }
    Ok(value.to_owned())
}

/// Feishu identifiers are a prefix and ASCII alphanumerics, as the Source's schemas require.
fn id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn parse_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start)
        .then(|| serde_json::from_str(&text[start..=end]).ok())
        .flatten()
}

fn run(
    program: &Path,
    args: &[&str],
    config_dir: Option<&Path>,
    stdin: Option<&[u8]>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LARKSUITE_CLI_NO_UPDATE_NOTIFIER", "1")
        .env("LARKSUITE_CLI_NO_SKILLS_NOTIFIER", "1")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = config_dir {
        command.env("LARKSUITE_CLI_CONFIG_DIR", dir);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    if let Some(bytes) = stdin {
        let mut pipe = child
            .stdin
            .take()
            .ok_or_else(|| format!("cannot write to {}", program.display()))?;
        pipe.write_all(bytes)
            .map_err(|error| format!("cannot write to {}: {error}", program.display()))?;
    }
    child
        .wait_with_output()
        .map_err(|error| format!("cannot wait for {}: {error}", program.display()))
}

/// The CLI's own sentence about a failure, never the arguments (one holds the App Secret).
fn detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if text.is_empty() {
        format!("the CLI exited with {}", output.status)
    } else {
        text.chars().take(2000).collect()
    }
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot secure {}: {error}", path.display()))?;
    }
    Ok(())
}

/// Replace a file in one step, so a service that starts meanwhile never execs a partial script.
fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let staged = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|span| span.as_nanos())
            .unwrap_or_default()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    let result = (|| {
        let mut file = options
            .open(&staged)
            .map_err(|error| format!("cannot create {}: {error}", staged.display()))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write {}: {error}", staged.display()))?;
        // `open` honors the umask, so set the mode the wrapper needs explicitly.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged, fs::Permissions::from_mode(mode))
                .map_err(|error| format!("cannot make {} executable: {error}", staged.display()))?;
        }
        fs::rename(&staged, path)
            .map_err(|error| format!("cannot install {}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn executable(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(path.to_path_buf())
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| executable(candidate).is_some())
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[derive(Deserialize)]
struct BotIdentity {
    #[serde(rename = "appId")]
    app_id: String,
    identities: Identities,
}

#[derive(Deserialize)]
struct Identities {
    bot: Identity,
}

#[derive(Deserialize)]
struct Identity {
    status: String,
}

#[derive(Deserialize)]
struct BotInfo {
    bot: BotDetails,
}

#[derive(Deserialize)]
struct BotDetails {
    open_id: String,
    #[serde(default)]
    app_name: String,
}

struct Bot {
    open_id: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> service::Config {
        let root = std::env::temp_dir().join(format!("booth-feishu-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("profiles").join("superagent")).unwrap();
        service::Config {
            profile: "superagent".into(),
            port: 3082,
            dsh_home: root.clone(),
            state_dir: root.join("state"),
            bundle: root.join("superagent"),
        }
    }

    #[test]
    fn identifiers_match_the_source_schemas() {
        assert_eq!(valid_app_id(" cli_abc123 ").unwrap(), "cli_abc123");
        assert!(valid_app_id("cli_").is_err());
        assert!(valid_app_id("app_123").is_err());
        assert!(valid_app_id("cli_ab-c").is_err());
        assert!(id("ou_01a", "ou_"));
        assert!(!id("ou_", "ou_"));
        assert!(!id("oc_1", "ou_"));
        assert!(validate_app_secret(" s ").is_ok());
        assert!(validate_app_secret("  ").is_err());
        assert!(validate_app_secret("a\nb").is_err());
    }

    #[test]
    fn adds_a_sender_without_removing_other_policy_entries_or_duplicates() {
        let mut policy =
            json!({"rules": [{"id": "copy"}], "sources": {"github": {"allowed": [{"id": "a"}]}}});
        allow_in_policy(&mut policy, "ou_one");
        allow_in_policy(&mut policy, "ou_one");
        assert_eq!(
            policy["sources"]["feishu"]["allowed"],
            json!([{"id": "ou_one"}])
        );
        assert_eq!(policy["sources"]["github"]["allowed"], json!([{"id": "a"}]));
        assert_eq!(policy["rules"][0]["id"], "copy");
    }

    #[test]
    fn a_chat_is_added_once_and_its_scope_can_change() {
        let mut source = json!({"enabled": false, "config": {"directMessages": true}});
        listen_in_source(&mut source, "oc_one", false);
        listen_in_source(&mut source, "oc_one", true);
        assert_eq!(
            source["config"]["chats"],
            json!([{"chatId": "oc_one", "listen": "all"}])
        );
        assert_eq!(source["config"]["directMessages"], json!(true));
    }

    #[test]
    fn enabling_the_plugin_keeps_the_layer_and_is_idempotent() {
        let cfg = config("plugin");
        let patch = cfg.patch();
        fs::create_dir_all(patch.parent().unwrap()).unwrap();
        fs::write(
            &patch,
            "# operator notes\n- id: webserver\n  config:\n    port: 3082\n",
        )
        .unwrap();
        assert!(enable_plugin(&cfg).unwrap());
        let text = fs::read_to_string(&patch).unwrap();
        assert!(text.starts_with("# operator notes\n"), "{text}");
        assert!(text.contains("id: dsh-superagent-feishu"), "{text}");
        assert!(text.contains("disabled: false"), "{text}");
        assert!(!text.contains("feishu-watch"), "{text}");
        assert!(!enable_plugin(&cfg).unwrap());
        fs::remove_dir_all(cfg.dsh_home).unwrap();
    }

    #[test]
    fn reads_json_after_a_warning_line() {
        assert_eq!(
            parse_json("warn\n{\"ok\":true}\n").unwrap()["ok"],
            json!(true)
        );
        assert!(parse_json("no json").is_none());
    }
}
