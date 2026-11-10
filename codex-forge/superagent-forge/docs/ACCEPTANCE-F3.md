# F3 Worker 验收

结果：**通过本阶段验收**。实测 `rustc 1.95.0` / `cargo 1.95.0`，原生 Linux；`CC=/bin/false CXX=/bin/false`。未链接 Node / DSH SDK。

| 验收项 | 实测结果 |
| --- | --- |
| OMP AgentExecutor | `OmpExecutor`：`--version` 探测 `omp-cli ` 前缀，stdin 下发 `ExecutionRequest`，stdout 解析 canonical JSONL |
| DSH AgentExecutor | `DshExecutor`：`--version` 探测 `dsh-cli ` 前缀；DSH JSONL 方言翻译为 canonical 事件 |
| 启动 / 等待 | 两种假 CLI 均完成 start→wait，产出 `ExecutionReport`（exit/summary/HEAD/files） |
| 取消 | sleep 模式 worker 可 cancel；会话从 executor 移除，进程组被终止 |
| stderr 拒绝 | worker 写 stderr → `BackendError::Execution` |
| 坏 JSON 拒绝 | 非法 JSONL → 协议错误 |
| 超量日志拒绝 | stdout 超过 1 MiB → 捕获限额错误 |
| 无 Node | `cargo tree -p forge-agent-omp -p forge-agent-dsh` 无 node/napi/neon；workspace 在 `CC=/bin/false` 下构建 |
| Workspace | `cargo test --workspace --all-targets --locked`（34 tests）、`clippy -D warnings`、`fmt --check` 通过 |

证据：

- [f3-test.stdout.txt](../tests/evidence/f3-test.stdout.txt)
- [f3-clippy.stdout.txt](../tests/evidence/f3-clippy.stdout.txt)
- [f3-metadata-summary.json](../tests/evidence/f3-metadata-summary.json)
- [f3-worker-deps.txt](../tests/evidence/f3-worker-deps.txt)

边界：`forge-fake-omp` / `forge-fake-dsh` 为协议相容的可选 CLI 替身，用于验收启动/等待/取消与拒绝路径；**不是**上游生产 OMP/DSH 发行版。对接真实二进制时需替换可执行路径并复核版本横幅与方言。尚无 MCP transport、DAG 编排循环或 Git 合并。假 CLI 仅用于测试，不进入生产路径。
