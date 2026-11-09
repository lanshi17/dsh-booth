# DSH-ADAPTER-01 交付记录

## 改动

- 新增 `src/dsh.rs`：有界流式 DSH JSONL 解析器，`feed` + `finish(exit_code)`、
  `read_dsh` reader helper、`DshSummary`、`DshError`、`DshLimits`。
- `src/lib.rs`：仅加入 `pub mod dsh;`，未改现有函数。
- 新增 `tests/dsh_adapter.rs`：13 个测试。
- 新增 `protocol/DSH-ADAPTER.md`：字段、增量规则、完成判定和未实现边界。
- 只读使用 `tests/fixtures/dsh-live-smoke.jsonl`；未新增或修改其他 fixture、
  数据集、baseline、readiness、`Cargo.toml`/`Cargo.lock`、`main.rs`、`runner.rs`
  或既有测试。

## 实际命令与退出码（本隔离副本）

```text
cargo fmt --all
exit 0

cargo test --all-targets --locked --offline -j2 -- --test-threads=4
exit 101
汇总：31 passed, 13 failed。
新增 tests/dsh_adapter.rs：13/13 通过。
tests/cli.rs：4/4 通过；tests/invariants.rs：11/11 通过；lib 单测：1/1 通过；
tests/runner.rs：2/15 通过，13 个失败。失败均为 Bubblewrap 无法创建
non-privileged user namespace，不是本次改动引入：
bwrap: No permissions to create a new namespace, likely because the kernel does
not allow non-privileged user namespaces.

cargo test --lib --test cli --test invariants --test dsh_adapter --locked --offline -j2 -- --test-threads=4
exit 0
29 passed, 0 failed（16 个原有非 runner 测试 + 13 个新增 dsh 测试）。

cargo clippy --all-targets --locked --offline -- -D warnings
exit 0

bwrap --unshare-user --ro-bind / / -- true
exit 1
bwrap: No permissions to create a new namespace, likely because the kernel does
not allow non-privileged user namespaces.
```

`runner.rs` 的 15 个测试需要在允许非特权 user namespace 的宿主上复验；未修改
权限，也未安装依赖。除此之外 `fmt`、非 runner 测试和 `clippy -D warnings` 均通过。

## 边界

- 只验证 CLI `0.2.1-rc.3` `--profile headless` 的脱敏 smoke；不声称适配所有
  DSH 版本。
- 真实 smoke 的 token 数为 CLI 自报（34746 / 10 / 34756），不是服务商账单；
  summary 的 `usage_source` 固定为 `dsh_cli_self_reported`，`billing_verified=false`。
- 未知事件、坏 JSON、非 UTF-8、重复 `step_end`、不合理顺序和超限均失败。
- 不启动 DSH/模型/工具，不生成正式 `Observation`，不执行或批准任务。
- 未修改 readiness 原值，未运行五组实验，未产生实验结论。

任务在此结束，不扩大范围。
