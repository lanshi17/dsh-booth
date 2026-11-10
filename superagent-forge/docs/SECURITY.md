# 权限和执行安全

三个独立门禁：任务接受与执行授权分离；执行结果与验收分离；验收与合并授权分离。Worker 不可修改 Policy、数据库或任务状态快照。

当前已实现：授权角色检查，Mandate 的后端与过期限制，Worker 提交身份检查，Reviewer 独立身份检查，Report/Assignment/HEAD 绑定，host evidence 门禁，未通过 Review 拒绝合并授权，取消后拒绝迟到结果。

MCP 接入使用宿主配置的静态 token 映射，并可叠加 OIDC-profile HS256 JWT（iss/aud/exp/sub，可选 subject allowlist）。客户端不能通过工具参数自称身份。无网络 JWKS；部署方必须在进程外管理密钥与 token。不得将库 API 或反序列化快照直接暴露给 Worker。

## F2 已落实（进程与工作区）

- 每个 Assignment 一个真实 Git Worktree；`worktrees_root` 必须在主仓之外；cwd 来自 Lease。
- Supervisor `env_clear` 后仅放行白名单，并可注入指定键；本 crate 不记录 env 值。
- Linux 可选 bubblewrap：主仓只读、worktree 可写、默认 `--unshare-net`；绝对路径写主仓在验收测试中失败。
- 可执行文件与参数均为 argv 数组，不经过 `sh -c`。
- 超时与取消对进程组发送 TERM/KILL；验收确认无存活孙进程。恢复时仍不凭 PID 单独认领进程。

## F3 已落实（Worker 协议）

- OMP/DSH 适配器在 start 前探测 CLI `--version` 横幅。
- 请求经 stdin JSON 下发；结果仅接受约定 JSONL；非空 stderr、坏 JSON、超限 stdout/stderr 一律拒绝。
- Worker 不能提交 Task 聚合或 Approved 状态；主控只从 Session 绑定生成 `ExecutionReport`。

## F4 已落实（编排与接入）

- DAG `max_concurrency` 与依赖失败阻断。
- 审计事件写入 outbox，可在崩溃后继续投递。
- MCP 拒绝未知协议版本与未映射/越权 token；重复 JSON-RPC id 幂等返回。

## F5 / F5.1 / F5.2 已落实（Review、合并、签名与推送）

- 主控从 worktree 采集真实 HEAD/Diff/检查命令结果；失败检查不能 Approve。
- `MergeAuthorization` 含到期时间，成功合并前做 HEAD/base CAS，成功后一次性消费。
- 审后 source 漂移拒绝合并；重复使用同一授权拒绝；Worker 不能调用合并入口。
- `DurableMergeAuthStore`（Turso `merge_authorizations`）支持崩溃恢复；`MemoryMergeAuthStore` 仍可用于测试。
- 可选 Ed25519 签名 merge grant（`forge-identity` + 迁移 004 `signature`）；验签失败拒绝合并。
- 合并后可选 argv-only `git push`（无 force、无 shell）。
- MCP 可选 OIDC-profile HS256 JWT 身份解析。

## 仍须后续落实

- 全局 Policy 与 Mandate 对环境白名单的自动交集、凭据注入策略、cgroup 资源限额。
- 未启用 bwrap 的部署路径、数据库与共享 Git 管理目录的额外 OS 边界。
- 网络 JWKS / 完整 IdP 联邦与 push 凭据注入。

主控代码禁止 unsafe，但传递依赖的 unsafe、OS 系统库接口和外部可执行文件属于单独审计范围。Pure Rust 不代表无需操作系统隔离。
