# 实施与验收任务

当前阶段 **F5 已通过**（见 [ACCEPTANCE-F5.md](ACCEPTANCE-F5.md)）。Forge 主线里程碑 F0–F5 全部完成。完成情况以 ACCEPTANCE 实测为准。

| 阶段 | 实施内容 | 独立验收门槛 |
| --- | --- | --- |
| F0 基线 | Workspace、领域模型、依赖边界 | [ACCEPTANCE.md](ACCEPTANCE.md) 已通过 |
| F1 存储 | 评估 Pure Rust SQLite 兼容引擎，实现 Repository、事务与迁移 | [ACCEPTANCE-F1.md](ACCEPTANCE-F1.md) 已通过 |
| F2 工作区和进程 | Git WorktreeManager、Runtime supervisor、Tokio、env 白名单、网络/文件隔离、超时资源限制 | [ACCEPTANCE-F2.md](ACCEPTANCE-F2.md) 已通过 |
| F3 Worker | 实现 OMP 和 DSH AgentExecutor，验证 CLI 版本与协议 | [ACCEPTANCE-F3.md](ACCEPTANCE-F3.md) 已通过 |
| F4 编排与接入 | DAG 并发队列、持久化事件/outbox、MCP stdio、可信身份映射 | [ACCEPTANCE-F4.md](ACCEPTANCE-F4.md) 已通过 |
| F5 独立 Review 和合并 | 真实 Diff/测试采集、限时一次性授权、HEAD/base CAS、授权消费 | [ACCEPTANCE-F5.md](ACCEPTANCE-F5.md) 已通过 |

F0–F5 已闭环；**F5.1 持久化 merge auth**与 **F5.2 远程 push / OIDC / 签名授权**已通过（见 [ACCEPTANCE-F5.1.md](ACCEPTANCE-F5.1.md)、[ACCEPTANCE-F5.2.md](ACCEPTANCE-F5.2.md)）。不把未实施的隔离策略当作已授权的安全保障。生产路径不得复用 Worker 自报验收。

本次不继续 Codex Fork 的 P0/P1 构建，不实现外部 Runtime 对 Codex 的集成。旧实验和报告保留在父目录 audit，便于追溯，不作为 Forge 验收结果。
