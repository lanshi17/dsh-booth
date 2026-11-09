# F4 编排与接入验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0` / `cargo 1.95.0`，原生 Linux；`CC=/bin/false CXX=/bin/false`。

| 验收项 | 实测结果 |
| --- | --- |
| DAG 并发队列 | `ConcurrentScheduler` / `DagRunner`：`max_concurrency` 限制同时 Running 数 |
| 依赖失败 | 上游 `mark_failed` 后下游进入 `Blocked`，不再 claim |
| 持久化 outbox | `save_with_outbox` 与 task CAS 同事务；崩溃重开后 `undelivered_outbox` 仍可见，标记 delivered 后清空 |
| 重复消息 | `mcp_inbox` 按 JSON-RPC method+id 缓存响应；重复请求返回相同 body |
| 断线恢复 | outbox 未投递事件与 mcp_inbox 缓存经 `reopen` 仍可用 |
| MCP stdio | Content-Length framing；`initialize` / `tools/list` / `tools/call` |
| 未知协议版本 | `protocolVersion != 2024-11-05` → JSON-RPC `-32600` |
| 越权调用 | 未知 `forgeAuthToken` 或无 create 权限 → `-32001`；身份来自宿主 token 映射，不信任工具参数自称 |
| Workspace | `cargo test --workspace --all-targets --locked`（42 tests）、`clippy -D warnings`、`fmt --check` 通过 |

证据：

- [f4-test.stdout.txt](../tests/evidence/f4-test.stdout.txt)
- [f4-clippy.stdout.txt](../tests/evidence/f4-clippy.stdout.txt)
- [f4-metadata-summary.json](../tests/evidence/f4-metadata-summary.json)

边界：MCP 为 Forge 自研最小 JSON-RPC/MCP 子集，未链接外部 MCP SDK。`forge assignment cancel` 仅协议层确认，完整取消与 Git 合并属 F5。可信身份映射依赖部署方注入的 token 表（`FORGE_MCP_TOKENS`），不是网络登录协议。
