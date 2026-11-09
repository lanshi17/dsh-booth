# 领域模型

| 参考概念 | 本项目模型 | 关键含义 |
| --- | --- | --- |
| Source | Source | CLI、MCP 或仓库来源 |
| Trigger | Trigger | 手动、计划或事件触发 |
| Policy | application::Policy | 主控侧授权者、Reviewer、合并授权者集合 |
| Ticket | Task | 持续存在的任务聚合 |
| Mandate | TaskMandate | 后端白名单、环境名、网络、时限和过期时间，不包含合并权限 |
| Executor | AgentExecutor | Worker 统一 Rust trait |
| Assignment | Assignment | 一次派发，拥有独立 ID、Worker 和 Worktree |
| Report | ExecutionReport | 一次执行的结果，关联 Task/Assignment 与 HEAD |
| Session | AgentSession | 后端进程/会话的句柄 |
| Orchestrator | TaskOrchestrator | 主控应用与后端的组合入口 |

接受 Task 不授权执行。Accepted → Authorized → Running；成功退出进入 AwaitingReview，失败进入 Failed。独立 Review 后进入 Approved 或 Rejected；只有 Approved 才能由具备合并角色的身份申请 MergeAuthorization。

Failed 或 Rejected 可产生新的 Assignment，同一 Task 保留历次 Assignment、Report 与 Review。新派发必须使用新的目录和分支，重新检查 Mandate 是否过期。Running 不允许第二次派发。取消后的迟到报告不能使任务复活。

Review 绑定当前 Assignment、Report ID 和完整 HEAD commit。Worker 即便被误授 Reviewer 角色也不能审查自己的 Assignment。Approved 只是验收通过；MergeAuthorization 是限时一次性合并授予（可签名、可持久化），`execute_merge` 在 HEAD/base CAS 后执行真实 Git 合并并进入 `Merged`，可选再 push 到配置 remote。

## 不变式和信任边界

模型字段的反序列化只用于可信存储快照，不能直接接受 Worker 提供的整个 Task。外部 Worker 只能提交受限的 Report，主控从受控 Session 绑定身份和 Assignment，再调用 Application。

PrincipalId 是身份标签，不是身份认证。网络/本地调用方不能通过请求字段自称 owner/reviewer；真正的身份解析和 ACL 装配属于后续主控接入工作。

WorkspaceLease.validate 只检查路径、分支前缀和 commit 格式。它不能证明目录存在、Git 分支合法、符号链接安全或 OS 隔离有效。未来 WorktreeManager 必须根据真实 Git 元数据签发 Lease，防止多个 Task 复用目录/分支。

执行报告的 HEAD 是 Worker 声明；独立 Reviewer 必须从真实 Worktree 观察 HEAD 并生成 ReviewEvidence。证据不是 Worker 自报字符串。当前用例验证 Reviewer 角色、证据门禁及报告关联，不执行测试命令。
