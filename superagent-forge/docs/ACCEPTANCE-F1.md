# F1 存储验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0` / `cargo 1.95.0`，原生 Linux；`CC=/bin/false CXX=/bin/false`。

| 验收项 | 实测结果 |
| --- | --- |
| 引擎选型 | 采用 `turso_core 0.9.0-pre.2`，`default-features = false`，启用 `pure-rust-crypto`，**关闭 `simd`** |
| 拒绝的候选 | 高层 `turso` facade：默认拉 `simsimd`，并经 `turso_sdk_kit` 引入 `bindgen`；`CC=/bin/false` 链接失败 |
| 无 C 引擎 | 依赖图无 `libsqlite3-sys` / `simsimd` / `bindgen` / `cmake`；`cc` 仅作为 `aegis` 的 build-dep 出现，纯 Rust crypto 路径下未调用系统 C 编译器 |
| CC/CXX=false | `cargo build/test/clippy -p forge-persistence` 与 workspace 测试通过 |
| 空库升级 | 新文件应用 `001_initial.sql`，写入 `schema_migrations` |
| 迁移幂等 | 再次 open 不重复插入迁移版本 |
| CAS | `expected_version` 不匹配 → `ErrorKind::ResourceBusy`；重复 `None` 插入 → `AlreadyExists` |
| 回滚 | 事务内 FK 失败后 `ROLLBACK`，先前已提交版本保持不变 |
| 约束 | UNIQUE(directory/branch)、CHECK(version>=1)、FK(task_id) 拒绝非法写入 |
| 崩溃重启 | drop 全部句柄后 `reopen`，已提交 version/assignment 仍可读取 |
| Workspace | `cargo test --workspace --all-targets`、`clippy -D warnings`、`fmt --check` 通过 |

证据：

- [f1-test.stdout.txt](../tests/evidence/f1-test.stdout.txt)
- [f1-metadata-summary.json](../tests/evidence/f1-metadata-summary.json)
- [f1-persistence-deps.txt](../tests/evidence/f1-persistence-deps.txt)

边界：尚无 Worker 进程、真实 Git Worktree、OS sandbox、MCP transport 或 Git 合并。`turso_core` 为预发布版；WAL/多进程语义以该引擎实测为准，不声称等同官方 SQLite。`cc` 仍出现在解析图中，但本阶段验证路径未执行 C 编译。
