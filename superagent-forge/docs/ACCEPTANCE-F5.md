# F5 独立 Review 与合并验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0` / `cargo 1.95.0`，原生 Linux；`CC=/bin/false CXX=/bin/false`；系统 `git`。

| 验收项 | 实测结果 |
| --- | --- |
| 真实 Diff/测试采集 | `collect_evidence` 在 worktree 执行 `git rev-parse` / `git diff --stat` 与 argv 检查命令 |
| 失败测试拒绝 | host check exit≠0 时 Approve 被拒绝 |
| 自审拒绝 | Worker 即使有 reviewer 角色也不能审查自己的 Assignment（既有门禁，仍覆盖） |
| 审后改动拒绝 | 授权后 source HEAD 漂移 → `HeadMismatch`；授权未被错误消费 |
| 限时一次性授权 | `MergeAuthorization` 含 `expires_at_unix`；过期执行合并失败 |
| HEAD/base CAS | 合并前校验 target tip==base 且 source tip==authorized head |
| 授权消费 | 成功合并后 `try_consume`；重复合并同一授权失败 |
| 主控授权后真实合并 | `execute_merge` 对 Git 仓库执行 ff/merge，任务进入 `Merged` |
| Workspace | `cargo test --workspace --all-targets --locked`、`clippy -D warnings`、`fmt --check` 通过 |

证据：

- [f5-test.stdout.txt](../tests/evidence/f5-test.stdout.txt)
- [f5-clippy.stdout.txt](../tests/evidence/f5-clippy.stdout.txt)
- [f5-metadata-summary.json](../tests/evidence/f5-metadata-summary.json)

边界：F5 验收使用进程内 `MemoryMergeAuthStore`。持久化实现见 [ACCEPTANCE-F5.1.md](ACCEPTANCE-F5.1.md)。未实现远程 push/PR、签名授权或跨仓库 merge。Worker 仍不能调用合并入口。
