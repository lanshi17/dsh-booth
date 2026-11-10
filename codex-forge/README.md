# Superagent Forge (`codex-forge`)

独立的 **Rust 2024 多 Agent 编排**实验仓库。主交付在 [`superagent-forge/`](./superagent-forge/)：领域门禁、Turso 持久化、Git Worktree / 进程监督、OMP·DSH Worker 适配、DAG / MCP stdio、主机侧 Review，以及可签名的合并授权与可选远程 push / OIDC HS256。

本仓库已重置为单根提交历史，**不以 OpenAI Codex 产品或上游贡献者为目标**。树内仍可能保留 `codex-rs/`、Bazel、审计材料等历史遗留目录，供对照或评测；它们**不参与** `superagent-forge` 的 Cargo 依赖解析。

## 快速开始（Forge）

```sh
cd superagent-forge
cargo build --locked -p forge-cli
cargo test --workspace --all-targets --locked
cargo run --locked -p forge-cli -- check --config config/default.toml
cargo run --locked -p forge-cli -- protocol
```

### MCP stdio（当前可运行入口）

```sh
export FORGE_MCP_DB=/tmp/forge.sqlite
export FORGE_MCP_TOKENS='tok-owner=owner:create,cancel'
cargo run --locked -p forge-cli -- mcp-stdio
```

可选 OIDC（三者同时设置）：`FORGE_OIDC_SECRET`、`FORGE_OIDC_ISSUER`、`FORGE_OIDC_AUDIENCE`。

协议：`initialize.protocolVersion` 必须为 `2024-11-05`，身份通过 `forgeAuthToken` 解析。已实现工具：`forge_task_create` / `forge_task_status` / `forge_assignment_cancel`。

不需要 Node.js 或模型 API Key。沙箱相关测试需要系统 `git`；Linux 上可选 `bwrap`。

更完整的说明见 [superagent-forge/README.md](./superagent-forge/README.md)。

## 仓库布局

| 路径 | 说明 |
| --- | --- |
| [`superagent-forge/`](./superagent-forge/) | **主工程**：独立 Cargo Workspace（F0–F5.2） |
| [`btae-eval/`](./btae-eval/) | 评测 / 隔离执行相关 Workspace（CI `forge-ci` 一并检查） |
| `codex-rs/`、`codex-cli/`、Bazel 等 | 遗留对照树；非 Forge 构建输入 |
| [`.github/workflows/`](./.github/workflows/) | 合并门禁以 `blocking-ci` → `forge-ci` 为主 |

## CI

合并门禁只要求：

- `forge-ci`（`superagent-forge` + `btae-eval`）
- codespell / cargo-deny / blob-size
- 汇总作业 `CI required`

上游 Codex 的 Bazel / `rust-ci` / SDK / postmerge 等已移出自动门禁，仅可手动 `workflow_dispatch`。详见 [docs/CI.md](./docs/CI.md) 与 [`.github/workflows/README.md`](./.github/workflows/README.md)。

## 文档（Forge）

- [架构与边界](./superagent-forge/docs/ARCHITECTURE.md)
- [领域模型](./superagent-forge/docs/DOMAIN-MODEL.md)
- [任务协议](./superagent-forge/docs/TASK-PROTOCOL.md)
- [安全](./superagent-forge/docs/SECURITY.md)
- [部署](./superagent-forge/docs/DEPLOYMENT.md)
- [路线图](./superagent-forge/docs/ROADMAP.md)

## License

见根目录 [LICENSE](./LICENSE) 与 [NOTICE](./NOTICE)。树内第三方与遗留组件的许可证声明以各子目录为准。
