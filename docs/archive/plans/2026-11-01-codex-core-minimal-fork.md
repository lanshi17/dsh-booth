**Status:** retired
**Created:** 2026-11-01
**Completed:** not implemented; superseded by independent Pure Rust design
**PR:** none

> 历史方案：已被用户最新的独立 Pure Rust Superagent Forge 方向替代，停止实施。活动计划见 ../../../superagent-forge/docs/ROADMAP.md。

# Codex Core Minimal Fork：上游源码审计与精简实施方案

来源：用户于 2026-11-01 在会话中提供的计划原文。本文保存其实施要求，省略原消息中的 Mermaid 页面样式及重复链接。
状态：执行 P0 → P1；P2–P4 待前置验收后推进。

## 固定决策

- 唯一上游 `openai/codex`，基线 `67f0994b532b8740f14b5b738af7b7b2a915b1d6`；Rust 2024。
- 复用原生 codex-core、Agent、Thread、Tool、Session、权限和 Sandbox 体系；不使用独立 Agent Bridge。
- 入口采用 codex-exec，保留 arg0 分发和 Sandbox/apply_patch 辅助执行能力；不自行重建 CodexThread 启动流程。
- 保留 AgentControl、CodexThread、Session、ThreadManager 的公共 API 和主要行为，完整保留 Native Subagent。
- 保持上游目录布局和 Apache-2.0 许可证及第三方声明，保留可追踪上游关系。

## P0 源码与依赖审计

以 codex-core、codex-exec 为根计算 Cargo 依赖闭包；不得只保留 core。

1. 固定提交、检查当前树与上游关系。
2. `cargo tree -p codex-core --edges normal,build --prefix none`；同样计算 codex-exec，以及 `--all-features` 变体。保存原始结果和退出码。
3. `cargo metadata --format-version 1 --locked` 获取真实解析图，按 dep kind 区分 normal/build/dev。无完整解析时不得把静态路径图称为 Cargo 运行时闭包。
4. 生成 core-required、headless-required、removable-candidates。仅测试依赖保留测试所需源码；依赖闭包中的能力默认保留。

## P1 Minimal Core（当前实施范围）

1. 保留 Core/Headless 必需 Workspace crate 和测试支持；排除 CLI/TUI/产品前端与发布入口。
2. 允许排除：Node codex-cli、Rust tui/cli、Desktop、Web/SDK、独占 cloud-tasks/cloud-tasks-client、voice-host/realtime-webrtc、独占发布配置。任何仍被正常依赖的包先保留，包括 app-server、协议/客户端/transport，以及经闭包确认必需的 app-server-daemon/codex-chatgpt。
3. Core 内的 analytics/feedback/guardian/memories/plugins/skills/connectors/code-mode 等先保留；不在首阶段大规模 feature gating，不以目录名删除依赖。
4. 首先得到可编译、能运行的精简版本，再扩展 OMP/DSH。
5. 验收：干净构建环境 Core 编译/相关原测试通过；codex-exec 无 TUI 启动，文本/JSONL、目录选择和 resume 保持上游参数；原生 Agent API 无破坏、Native Subagent 正常、安全默认不关闭。

构建前可在隔离副本验证，并清楚区分副本与原树；试验不能代替最终精简树的真实构建/功能验收。

## P2 External Agent Runtime（本轮暂不实施）

新增独立 crate codex-external-agent，不能反向依赖 codex-core。Core 仅做工具与调度集成；外部 crate 负责进程、协议、工作区、生命周期，含 OMP/DSH adapters。

保留原 AgentControl 的 spawn/send/resolve/interrupt/list/mailbox；新增独立 ExternalAgentControl 的 spawn_external/send_external/resolve_external/interrupt_external/list_external/wait_external。不得将外部进程伪装为 CodexThread。

独立 ExternalAgentId/Handle/Result 记录 backend、workspace、branch、status、exit_code、summary、changed_files、diff。

## P3 调度与安全（后续）

独立 Worktree/分支，白名单环境，按任务授权网络，可配置超时，清理子进程组，日志限额与脱敏。可执行文件+参数数组，禁止把任务串直接 sh -c。

生命周期 Created/Preparing/Running/Waiting/Completed/Failed/Cancelled/AwaitingReview/Approved/Rejected；Worker 不得自行 Approved。主控独立 Review/测试通过后才按授权合并。任务持久化、取消、崩溃与重启恢复、并行隔离均需测试。

## P4 进一步瘦身（后续）

逐项可选化 Core 产品能力，优化编译时间/体积，跨平台构建与上游同步。拟议 headless/native-agent/external-agent/mcp/sandbox/plugins/skills/connectors/code-mode/memories/guardian/telemetry feature schema 为目标设计，并非上游现有开关；native-agent 必须保留，sandbox 不默认关闭。

## 总体验收矩阵

Core 原有测试；Headless 无 TUI；AgentControl API 保留；Native Agent；OMP/DSH 启动/等待/取消；多 Worker 隔离；未审核不能合并；重启恢复；Worker 无法绕过主控审批。P2/P3 项不能因为 P1 通过而提前标记通过。
