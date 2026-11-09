# Superagent Forge

独立 Rust 2024 多 Agent 编排平台的架构与领域模型基线。
只借鉴 [DeepSeek DSH Superagent](https://github.com/deepseek-ai/dsh-superagent-prototype) 的领域概念；不 Fork Codex，不链接 DSH Runtime 或 SDK。

当前交付：16 个 Cargo crate，覆盖 F0–F5.2（领域门禁、持久化、Worktree/supervisor、OMP/DSH、DAG/MCP、主机 Review、签名合并授权、可选远程 push / OIDC HS256）。

```sh
cd superagent-forge
CC=/bin/false CXX=/bin/false cargo test --workspace --all-targets --locked
cargo run --locked -p forge-cli -- check --config config/default.toml
cargo run --locked -p forge-cli -- protocol
# optional: FORGE_MCP_DB=... FORGE_MCP_TOKENS='tok=owner:create,cancel' cargo run --locked -p forge-cli -- mcp-stdio
# optional OIDC: also set FORGE_OIDC_SECRET / FORGE_OIDC_ISSUER / FORGE_OIDC_AUDIENCE
```

这些命令不需要 Node.js 或模型凭据。F2 sandbox 测试需要系统 `git` 与（Linux）`bwrap`。F3 测试使用内置假 CLI。MCP 身份来自 `FORGE_MCP_TOKENS`（及可选 OIDC JWT），不是公开匿名接口。

本目录有独立 Cargo Workspace 与 Cargo.lock，可以单独复制并构建；父目录历史 Codex 源码和审计文件不参与依赖解析。

- [架构与边界](docs/ARCHITECTURE.md)
- [领域模型与状态机](docs/DOMAIN-MODEL.md)
- [协议设计](docs/TASK-PROTOCOL.md)
- [权限与隔离](docs/SECURITY.md)
- [依赖决策](docs/DEPENDENCIES.md)
- [构建和部署](docs/DEPLOYMENT.md)
- [后续实施与验收任务](docs/ROADMAP.md)
- [F0 验收](docs/ACCEPTANCE.md)
- [F1 存储验收](docs/ACCEPTANCE-F1.md)
- [F2 工作区与进程验收](docs/ACCEPTANCE-F2.md)
- [F3 Worker 验收](docs/ACCEPTANCE-F3.md)
- [F4 编排与接入验收](docs/ACCEPTANCE-F4.md)
- [F5 Review 与合并验收](docs/ACCEPTANCE-F5.md)
- [F5.1 持久化 Merge Auth](docs/ACCEPTANCE-F5.1.md)
- [F5.2 远程 Push / OIDC / 签名授权](docs/ACCEPTANCE-F5.2.md)
