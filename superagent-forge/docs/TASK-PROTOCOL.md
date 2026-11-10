# 任务协议 v1（设计基线）

协议分为主控调用协议、后端执行协议。MCP/JSON-RPC 供主控接入；OMP/DSH 的子进程协议由各 Adapter 翻译，不假定两者有相同的 JSONL 格式。

ExecutionRequest 包含版本、主控生成的 Assignment、TaskMandate 和 prompt。后端 trait 使用 start(request)、wait(session)、cancel(session)，返回 AgentSession 和 ExecutionReport；Worker 不接收 Task 聚合的写权限，也不能提交 Approved 状态。

ExecutionReport 的字段是 id、task_id、assignment_id、exit_code、summary、head_commit、changed_files。主控必须从受控 Session 核对身份、版本、Task/Assignment 关联，限制输入字节数与路径范围，再入库。Worker 退出码 0 不等于 Review 通过。

Worker 子进程协议（F3）：OMP 使用 canonical JSONL（`hello`/`result`）；DSH 使用 `dsh_protocol` 方言并由适配器翻译。stdout/stderr/单行 JSON 设硬限额；非空 stderr 拒绝。版本通过各 CLI `--version` 横幅校验。

MCP 线路协议（F4）：JSON-RPC 2.0 + Content-Length framing；`initialize.protocolVersion` 必须为 `2024-11-05`；`forgeAuthToken` 由宿主身份表解析；未知版本/越权拒绝；`mcp_inbox` 对 method+id 幂等；帧大小上限 1 MiB。

MCP 工具 `forge_task_create` / `forge_task_status` / `forge_assignment_cancel` 已标记 implemented；不向 Worker 暴露 Review 或 Merge 工具。`forge mcp-stdio` 提供可运行的 stdio 服务。

任务接受等事件与 Task snapshot 经 `save_with_outbox` 同事务写入；投递状态可在断线后恢复。
