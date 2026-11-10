# 依赖选型与 Pure Rust 边界

基线使用 Rust 2024、Cargo resolver 3；声明 MSRV 1.85。当前实际测试工具链见 ACCEPTANCE，尚不声称已在 MSRV 或所有平台完成测试。Cargo.lock 固定此次解析结果。

| 能力 | 决策 | 状态 |
| --- | --- | --- |
| 模型序列化 | serde、serde_json | 已接入 |
| ID | uuid v4，新类型区分 Task/Assignment/Report/Session | 已接入 |
| 错误 | thiserror | 已接入 |
| 配置 | toml 0.9，deny_unknown_fields | 已接入 |
| Headless CLI | clap 4 | 已接入 |
| Async 执行 | Tokio 1.54（io-util/macros/process/rt-multi-thread/sync/time） | F2/F3 已接入 |
| Git Worktree | `std::process::Command` argv 调用系统 `git` | F2 已实现 `GitWorktreeManager` |
| 进程监督 | Tokio Command + Unix process group + 可选 `bwrap` | F2 已实现 `ProcessSupervisor` |
| Worker JSON | serde_json；OMP canonical JSONL / DSH 方言翻译 | F3 已接入；无 Node SDK |
| SQLite 持久化 | `turso_core`（非 `turso` facade）+ Repository 实现 | F1 已接入；见下方决策 |
| MCP | 自研 JSON-RPC + Content-Length stdio（不链接 rmcp/Node） | F4 已接入；协议版本 `2024-11-05` |
| 签名 / OIDC | `ed25519-dalek` + `hmac`/`sha2`/`base64`（`forge-identity`） | F5.2 已接入；无网络 JWKS |
| 日志 | tracing + 结构化脱敏层 | 执行阶段接入 |

## SQLite 与 Pure Rust 的取舍

SQLite 官方引擎以 C 实现。rusqlite/sqlx 的 SQLite 驱动最终依赖该引擎，不能把它们描述为严格的全 Rust 依赖链。本阶段不加入 libsqlite3-sys 或 bundled C SQLite，也不以文件 JSON 冒充 SQLite。

候选为 Rust 编写的 SQLite 兼容引擎 [Turso](https://github.com/tursodatabase/turso)。crates.io 上 `turso 0.9.0-pre.2` 为预发布 facade。

## F1 引擎决策（已采纳 `turso_core`）

| 方案 | 结果 |
| --- | --- |
| `turso` facade（default / no-default） | **拒绝**：默认启用 `turso_core/simd`→`simsimd`；`turso_sdk_kit` build-dep 含 `bindgen`；`CC=/bin/false` 下链接 `-lsimsimd` 失败 |
| `turso_core` + `default-features=false` + `pure-rust-crypto`，**不加 `simd`** | **采纳**：无 `libsqlite3-sys`/`simsimd`/`bindgen`；`CC=/bin/false` 构建与 F1 测试通过 |

Workspace 依赖声明见根 `Cargo.toml` 的 `turso_core`。禁止在未重新审计的情况下打开 `simd` 或改回 `turso` facade。`cc` 仍可能作为 `aegis` 的 build-dep 出现在解析图中；验收以“未调用系统 C 编译器、未链接 C SQLite”为准。

WAL/多进程语义按该引擎实测支持验收，不能照搬官方 SQLite 保证。若未来引擎变更导致 CC=false 失败，不悄悄换回 C SQLite；保留 port 并重新决策。

## 依赖验收

执行 `cargo metadata --locked --format-version 1`，确认全部本地包只来自本目录，外部包来自 crates.io，无 Codex/DSH Git 源、Node 包或 JS 构建步骤。未来新增依赖须重跑审计；当前小依赖图的通过不能外推至未来引擎或 SDK。
