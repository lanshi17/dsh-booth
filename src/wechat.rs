//! Terminal setup for the iLink WeChat Bot and its sender allow-list.

use crate::{
    api::{Api, ApiError},
    service,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

const ILINK: &str = "https://ilinkai.weixin.qq.com";
const QR_LIFETIME: Duration = Duration::from_secs(170);
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
struct QrCode {
    qrcode: String,
    #[serde(default)]
    qrcode_img_content: Option<String>,
    #[serde(default)]
    ret: Option<i64>,
}

#[derive(Deserialize)]
struct QrStatus {
    status: String,
    #[serde(default)]
    bot_token: Option<String>,
    #[serde(default)]
    ilink_bot_id: Option<String>,
    #[serde(default)]
    ilink_user_id: Option<String>,
    #[serde(default)]
    baseurl: Option<String>,
    #[serde(default)]
    ret: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct IlinkCredentials {
    bot_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ilink_bot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ilink_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseurl: Option<String>,
}

/// Obtain and print terminal-friendly login links, refreshing expired QR codes automatically.
pub fn login(cfg: &service::Config, api: &Api) -> Result<(), String> {
    service::create_private_state_dir(cfg)?;
    let credentials_path = cfg.state_dir.join("weixin-credentials.json");
    let (credentials, reused) = cached_or_connect(&credentials_path, connect)?;
    if reused {
        println!("已读取本机保存的 iLink 会话凭证，复用现有登录，不重新扫码。\n");
    } else {
        println!("微信 Bot 已绑定，凭证已安全保存。\n");
    }
    activate(api, &credentials);
    Ok(())
}

fn connect() -> Result<IlinkCredentials, String> {
    println!("微信 iLink 登录：请用微信扫描下面的链接并在手机上确认。按 Ctrl+C 可退出。\n");
    loop {
        let qr: QrCode = get_json(&format!("{ILINK}/ilink/bot/get_bot_qrcode?bot_type=3"))?;
        if qr.ret.is_some_and(|ret| ret != 0) || qr.qrcode.is_empty() {
            return Err("iLink did not return a QR code".into());
        }
        let url = qr
            .qrcode_img_content
            .unwrap_or_else(|| format!("{ILINK}/ilink/qrcode?qrcode={}", query(&qr.qrcode)));
        println!("扫码链接（二维码约 3 分钟有效）：\n{url}\n");
        let deadline = Instant::now() + QR_LIFETIME;
        loop {
            sleep(Duration::from_secs(2));
            match qr_status(&qr.qrcode) {
                Ok(status)
                    if status.status == "confirmed"
                        && status
                            .bot_token
                            .as_deref()
                            .is_some_and(|token| !token.is_empty()) =>
                {
                    let credentials = IlinkCredentials {
                        bot_token: status.bot_token.unwrap(),
                        ilink_bot_id: status.ilink_bot_id.filter(|value| !value.is_empty()),
                        ilink_user_id: status.ilink_user_id.filter(|value| !value.is_empty()),
                        baseurl: status.baseurl.filter(|value| !value.is_empty()),
                    };
                    return Ok(credentials);
                }
                Ok(status) if status.status == "expired" || status.ret == Some(3) => {
                    println!("二维码已过期，正在获取新链接……\n");
                    break;
                }
                Ok(status)
                    if status.status == "pending"
                        || status.status == "waiting_confirm"
                        || status.status == "scaned" =>
                {
                    print!("\r等待微信确认（{}）…", status.status);
                    let _ = std::io::stdout().flush();
                }
                Ok(status) if status.status == "confirmed" => {
                    return Err("iLink confirmed the QR code without returning a bot token".into())
                }
                Ok(status) => {
                    print!("\r等待微信确认（{}）…", status.status);
                    let _ = std::io::stdout().flush();
                }
                Err(_) => {
                    print!("\r正在连接 iLink，继续等待扫码…");
                    let _ = std::io::stdout().flush();
                }
            }
            if Instant::now() >= deadline {
                println!("\n二维码超时，正在刷新……\n");
                break;
            }
        }
    }
}

fn cached_or_connect(
    path: &Path,
    connect: impl FnOnce() -> Result<IlinkCredentials, String>,
) -> Result<(IlinkCredentials, bool), String> {
    if let Some(credentials) = read_credentials(path)? {
        return Ok((credentials, true));
    }
    let credentials = connect()?;
    save_credentials(path, &credentials)?;
    Ok((credentials, false))
}

fn activate(api: &Api, credentials: &IlinkCredentials) {
    if let Some(user_id) = credentials.ilink_user_id.as_deref() {
        match allow(api, user_id) {
            Ok(()) => println!("扫码账号 {user_id} 已加入 Portal 允许名单。"),
            Err(_) => println!(
                "扫码账号是 {user_id}；服务未就绪时，启动后运行：booth wechat allow {user_id}"
            ),
        }
    } else {
        match retry_api("enable the WeChat Source", || enable_source(api)) {
            Ok(()) => println!(
                "微信 Source 已启用。允许发送者后再发消息：booth wechat allow <微信用户 ID>。"
            ),
            Err(_) => println!(
                "凭证已保存；启动服务后运行 booth wechat allow <微信用户 ID> 添加允许发送者。"
            ),
        }
    }
    println!("iLink 会话已保存到本机，之后运行 booth wechat login 会复用它，不再要求扫码。\n");
}

/// Add a sender to the iLink allow-list without discarding other Policy settings.
pub fn allow(api: &Api, user_id: &str) -> Result<(), String> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err("usage: booth wechat allow <微信用户 ID>".into());
    }
    retry_api("enable the WeChat Source", || enable_source(api))?;
    retry_api("update the WeChat allow-list", || {
        api.set("policy", &|policy| allow_in_policy(policy, user_id))
            .map(|_| ())
    })?;
    println!("已允许微信用户 {user_id} 联系 Portal。现在可以给 Bot 发消息。");
    Ok(())
}

fn enable_source(api: &Api) -> Result<(), ApiError> {
    let (mut source, revision) = api.source_block("weixin")?;
    if source.get("enabled").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    source["enabled"] = json!(true);
    api.save("source:weixin", &source, revision)?;
    Ok(())
}

fn retry_api<T>(action: &str, mut run: impl FnMut() -> Result<T, ApiError>) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match run() {
            Ok(value) => return Ok(value),
            Err(error) if transient(&error) && Instant::now() < deadline => {
                sleep(Duration::from_millis(600));
            }
            Err(error) => return Err(format!("cannot {action}: {error}")),
        }
    }
}

fn transient(error: &ApiError) -> bool {
    match error {
        ApiError::Transport(_) => true,
        ApiError::Status(code, _) => *code == 409 || *code >= 500,
        ApiError::Protocol(_) => false,
    }
}

fn allow_in_policy(policy: &mut Value, user_id: &str) {
    if policy.pointer("/sources/weixin").is_none() {
        policy["sources"]["weixin"] = json!({"allowed": []});
    }
    let allowed = policy["sources"]["weixin"]["allowed"]
        .as_array_mut()
        .expect("weixin allowed list is an array");
    if !allowed
        .iter()
        .any(|entry| entry.get("id").and_then(Value::as_str) == Some(user_id))
    {
        allowed.push(json!({"id": user_id}));
    }
}

fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let response = ureq::get(url)
        // iLink's QR status endpoint may hold one GET open until a state change or QR expiry.
        // This is intentionally longer than its long-poll window, so a successful phone scan is
        // not mistaken for a transport failure halfway through confirmation.
        .timeout(Duration::from_secs(90))
        .call()
        .map_err(|error| format!("cannot reach iLink ({error})"))?;
    response
        .into_json()
        .map_err(|error| format!("iLink returned invalid JSON ({error})"))
}

fn qr_status(code: &str) -> Result<QrStatus, String> {
    get_json(&format!(
        "{ILINK}/ilink/bot/get_qrcode_status?qrcode={}",
        query(code)
    ))
}

fn query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn read_credentials(path: &Path) -> Result<Option<IlinkCredentials>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!("{} must be private (mode 0600)", path.display()));
        }
    }
    let credentials: IlinkCredentials = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
    if credentials.bot_token.trim().is_empty() {
        return Err(format!("{} has no iLink bot token", path.display()));
    }
    Ok(Some(credentials))
}

fn save_credentials(path: &Path, value: &IlinkCredentials) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("credential path has no parent directory")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".weixin-credentials-{}-{suffix}.tmp",
        std::process::id()
    ));
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("cannot encode iLink credentials: {error}"))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp).map_err(|error| {
            format!(
                "cannot create private credentials {}: {error}",
                temp.display()
            )
        })?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot save iLink credentials: {error}"))?;
        fs::rename(&temp, path).map_err(|error| {
            format!(
                "cannot install iLink credentials at {}: {error}",
                path.display()
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("cannot secure {}: {error}", path.display()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_a_sender_without_removing_other_policy_entries_or_duplicates() {
        let mut policy =
            json!({"rules": [{"id":"copy"}], "sources": {"github": {"allowed": [{"id":"a"}]}}});
        allow_in_policy(&mut policy, "wxid-one");
        allow_in_policy(&mut policy, "wxid-one");
        assert_eq!(
            policy["sources"]["weixin"]["allowed"],
            json!([{"id":"wxid-one"}])
        );
        assert_eq!(policy["sources"]["github"]["allowed"], json!([{"id":"a"}]));
        assert_eq!(policy["rules"][0]["id"], "copy");
    }

    #[cfg(unix)]
    #[test]
    fn saves_credentials_atomically_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("booth-wechat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("weixin-credentials.json");
        let credentials = IlinkCredentials {
            bot_token: "secret".into(),
            ilink_bot_id: Some("bot".into()),
            ilink_user_id: Some("wx-user".into()),
            baseurl: None,
        };
        save_credentials(&path, &credentials).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(read_credentials(&path).unwrap(), Some(credentials));
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reuses_a_saved_session_without_requesting_another_qr_scan() {
        let dir = std::env::temp_dir().join(format!("booth-wechat-reuse-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("weixin-credentials.json");
        let credentials = IlinkCredentials {
            bot_token: "x".into(),
            ilink_bot_id: Some("bot".into()),
            ilink_user_id: Some("wx-user".into()),
            baseurl: None,
        };
        save_credentials(&path, &credentials).unwrap();
        let mut requested_qr = false;
        let (loaded, reused) = cached_or_connect(&path, || {
            requested_qr = true;
            Err("QR must not be requested".into())
        })
        .unwrap();
        assert_eq!(loaded, credentials);
        assert!(reused);
        assert!(!requested_qr);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saves_a_new_session_for_the_next_login() {
        let dir = std::env::temp_dir().join(format!("booth-wechat-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("weixin-credentials.json");
        let credentials = IlinkCredentials {
            bot_token: "y".into(),
            ilink_bot_id: None,
            ilink_user_id: None,
            baseurl: None,
        };
        let (_, reused) = cached_or_connect(&path, || Ok(credentials.clone())).unwrap();
        assert!(!reused);
        let (loaded, reused) = cached_or_connect(&path, || Err("must reuse".into())).unwrap();
        assert!(reused);
        assert_eq!(loaded, credentials);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn escapes_qr_codes_as_query_parameters() {
        assert_eq!(query("a b/?"), "a%20b%2F%3F");
    }
}
