# DSH headless live collector

状态：**工程 smoke 已通过；正式 Pilot 未运行。** Pure Rust collector 复用现有 `DshAdapter`，通过固定 executable + argv 启动外部进程，清空继承环境，仅传递显式 allowlist；stdin、stdout、stderr、超时、取消、输出上限和进程组终止均有界。它不声称提供 OS 隔离，调用方须提供 bwrap 等包装器；记录固定 `network_isolation=caller_supplied`，并始终 `billing_verified=false`。

## 验证结果

- 全套 Cargo 测试：**73 passed, 0 failed**。
- live collector 测试：**15 passed**，覆盖正常 JSONL、环境清理、stdin 背压、精确 stdout 边界、stderr 洪泛、协议错误、非零退出、超时、取消、后代管道清理、证据碰撞和 CLI exit code。
- Clippy `-D warnings`：通过；格式检查：通过。
- `check-ready datasets/pilot.json`：退出 1（预期阻断）。
- 手工行为变异：**3/3 killed**，无编译错误归因。

## 真实 DSH smoke

使用已安装的 DSH `0.2.1-rc.3`、`--profile headless --json -`，并由 bubblewrap 以 `--share-net` 运行工程 smoke。最终源码重跑于 2026-11-03，结果为退出 0、耗时 2,229 ms、1 session、1 turn、1 step、7 个核心事件，解析器 `completed=true`，self-reported usage 5,699。凭据仅进入私有进程环境；报告不保存 prompt、argv、credential 或原始 transcript。模型不可变 revision、账单和隐藏测试均未验证。

机器摘要见 [DSH-LIVE-COLLECTOR.json](DSH-LIVE-COLLECTOR.json)。该结果是单次工程连通性和解析验收，不能转化为 Observation、方法效果、预算账单或 BTAE 优势结论。

## 尚未完成

真实执行尚未接入五组调度、任务快照、Worktree、独立 evaluator、隐藏测试或受限 endpoint allowlist。当前 CLI 的 `collect` 是工程 smoke 入口；`network_isolation=caller_supplied` 是有意的安全边界，正式运行必须在可信 bwrap/cgroup/网络策略下配置。由于 OMP fresh review 在本次运行中未形成可用报告，源码状态没有独立审查 verdict；本任务的自有 gauntlet 也不等价于 Tier 3 验收。


## Scope, reproducibility and limitations

`collect` defaults to `--profile headless --json -` when no custom arguments are supplied. It permits a trusted launcher executable such as `/usr/bin/bwrap`; it does not provision that launcher or validate its policy. `--pass-env NAME` passes selected existing environment values, never values on argv. Raw child streams are discarded after parsing/hashing. `started.json` and `record.json` are private create-once receipts, not signed evidence; a missing record after setup/host interruption means incomplete collection. Executable hashing/input acquisition and final receipt fsync are outside the process deadline. An unkillable process or daemon that escapes its process group still requires caller PID namespace/cgroup enforcement.

The initial protocol spec was broader than this delivered supervisor; its appended scope revision is explicit. Nix's existing `fs` feature was enabled for nonblocking pipes; Cargo.lock/dependency versions did not change. Coverage threshold, fuzzing/properties, random order, minimum-version matrix, checker completeness and a successful independent review were not established. One temporary protocol-kill mutation survived because a second cleanup path preserved behavior; the final compound mutant disables both protocol-error kill paths and is disclosed as such. Some early audit helper runs failed on paths/output collisions and were corrected. Only `output/final/summary.json` is claimed for the final source; its parent output logs are an earlier checkpoint.

Re-run offline checks from the repository root with `python3 audit/dsh-live-collector/20261102-03/verify.py output/replay`; the destination must be new. The command runs full tests, Clippy, fmt, readiness(expected 1), source binding and three assertion-checked manual mutations. A real smoke additionally requires a trusted installed DSH bundle, dedicated profile and provider credential, so it is reported separately and is not silently repeated by that command.

Final verification source/input SHA256: `6d48bcee20bfce71f1a147f17765df6ccb08f8a0071e6c40020c307a17e2790c`. [Bound summary](../../audit/dsh-live-collector/20261102-03/output/final/summary.json) records tool versions and checker digests. The isolated worktree contains no landing checkout credentials or build output; these results apply to its public source/input manifest, not to arbitrary host state.
