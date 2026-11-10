# 架构决策：独立 Pure Rust 实现

状态：按用户最新决策替代 Codex Core Minimal Fork。旧 P0/P1 审计为历史资料，停止续建；已归档并恢复该阶段对 codex-rs/Cargo.toml 的修改。本项目没有 Codex crate 依赖。

参考来自用户给定的 DSH Superagent 领域设计，不声称已完成参考仓库源码级审计，不复制其 TypeScript 实现。

## 依赖方向

| crate | 职责与当前交付 |
| --- | --- |
| domain | Source、Trigger、TaskMandate、Task、Assignment、ExecutionReport、AgentSession，任务状态转换 |
| application | Policy、可信身份角色检查、独立 Review 与显式合并授权用例 |
| orchestrator | `TaskOrchestrator` + `DagRunner`（并发上限与依赖失败传播） |
| agent-runtime | `ProcessSupervisor` + 有界 IO 捕获 + 共享 `CliWorkerExecutor` |
| agent-protocol | `ExecutionRequest`、Worker JSONL 事件、OMP/DSH 方言解析与限额 |
| agent-backend | 对象安全 AgentExecutor，start/wait/cancel Future 接口 |
| agent-omp / agent-dsh | `OmpExecutor` / `DshExecutor`；版本探测、stdin 请求、方言翻译 |
| task-scheduler | DAG 排序 + `ConcurrentScheduler`（claim/成功/失败/Blocked） |
| worktree | `GitWorktreeManager`：主仓外 detached worktree 分配/释放 |
| review | `collect_evidence`（真实 git diff/HEAD/检查）+ `merge_approved_branch`（CAS 合并） |
| persistence | Turso 仓储；outbox；`mcp_inbox`（002）；`merge_authorizations`（003） |
| mcp-server | MCP stdio（Content-Length）+ JSON-RPC；可信 token→Principal 映射 |
| config | 严格 TOML 配置校验，拒绝未知字段 |
| cli | forge check / protocol，无执行或合并命令 |

Domain 不依赖应用、进程、存储或 UI。Application 依赖 Domain 与 Review；Review 仅依赖 Domain。Backend 依赖 Protocol 与 Domain，OMP/DSH adapter 依赖 Backend。没有循环依赖。

## 运行部署边界

主控、协议和生命周期使用 Rust；Git、可选 OMP/DSH CLI 是外部工具，通过进程协议调用。主控的编译和基础运行不要求安装这些 Worker。

若选择的外部 DSH CLI 自身需要 Node，这是该可选 Worker 的部署依赖；Rust 主控不链接它，也不因此要求所有部署安装 Node。完全禁止 Node 的部署应禁用该 Worker 并选择可用的非 Node 后端，或将该 Worker 放在独立执行环境。

F0–F5.2 已提供领域门禁、持久化、隔离进程、Worker 适配、DAG/MCP、主机侧 Diff/测试采集、限时一次性合并授权（可持久化、可 Ed25519 签名）、可选远程 push，以及静态 token / OIDC HS256 身份。未配置 sandbox 时不能把 supervisor 默认行为等同于 OS 安全边界。
