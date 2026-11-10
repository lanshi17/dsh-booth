# F5.1 持久化 Merge Auth 验收

结果：**通过**。在 F5 之上将一次性合并授权落到 Turso，支持崩溃恢复。

| 验收项 | 实测结果 |
| --- | --- |
| 迁移 003 | `merge_authorizations` 表；`schema_migrations` 含版本 1/2/3 |
| 插入与读取 | `insert_merge_auth` / `get_merge_auth` 跨 `reopen` 仍可见 |
| 一次性消费 | `try_consume_merge_auth` CAS：成功后再次消费 → `AlreadyExists` |
| 过期拒绝 | `expires_at_unix <= now` → `TimedOut` |
| 应用适配 | `DurableMergeAuthStore` 实现 `MergeAuthStore`；真实合并后任务 `Merged` |
| Workspace | `cargo test/clippy --workspace`（`CC=/bin/false`）通过 |

证据：

- [f5.1-test.stdout.txt](../tests/evidence/f5.1-test.stdout.txt)
- [f5.1-clippy.stdout.txt](../tests/evidence/f5.1-clippy.stdout.txt)
- [f5.1-metadata-summary.json](../tests/evidence/f5.1-metadata-summary.json)

边界：本阶段仍无远程 push、签名授权或 OIDC（见 [ACCEPTANCE-F5.2.md](ACCEPTANCE-F5.2.md)）。内存 `MemoryMergeAuthStore` 保留给单测。
