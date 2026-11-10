# 构建与部署

当前仅部署架构验证 CLI。Rust 主控不要求 Node、DSH Runtime、OMP 或模型 API Key。

```sh
cd superagent-forge
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo run --locked -p forge-cli -- check --config config/default.toml
cargo run --locked -p forge-cli -- protocol
```

default.toml 的绝对路径是部署示例。check 不创建目录、数据库或进程；配置错误返回非零退出码。

F2 supervisor 在启用 sandbox 时依赖系统 `git` 与 Linux `bwrap`；`network_allowed=false` 通过 `--unshare-net` 强制。未安装 bwrap 或未传入 `SandboxMounts` 时，不得声称已禁网或已阻止写主仓。

未来独立部署目录应包含 forge、配置、数据库和受控 Worktree 根（主仓之外）；Worker 置于权限受限环境。OMP/DSH CLI 是可选外部后端，按其自身安装要求部署。选择 DSH 不改变 Rust 主控与 SDK 的依赖关系。

F0–F5.2 提供持久化（含带签名的 merge auth）、Worktree/supervisor、OMP/DSH 适配、DAG/MCP、主机侧 Review、本地合并与可选远程 push，以及静态 token / OIDC HS256 身份。生产应使用 `DurableMergeAuthStore` + merge 签名密钥，并加固沙箱与 push 凭据。不要将本仓库直接当作多租户 SaaS 发布。父仓库遗留 Node 配置和 Codex 文件不属于本 Workspace 的构建输入。
