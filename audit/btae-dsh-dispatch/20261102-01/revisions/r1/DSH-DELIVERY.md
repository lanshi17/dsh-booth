# BTAE-DSH-ADAPTER-01 R1 交付记录

## 本轮修正

- 撤销“cache 必须是 raw input 子集”的旧假设。按真实 DSH 0.2.x headless
  schema 处理：`inputTokens` 是 uncached input，`cacheReadTokens` /
  `cacheWriteTokens` 是额外且不重叠的 prompt bucket，`reasoningTokens` 是
  output 子集；canonical input/total 只计一次。
- 成功解析只读 fixture
  `tests/fixtures/dsh-live-development-sanitized.jsonl`（141 事件、23 steps），
  验证 canonical input `1_871_486`、output `45_968`、total `1_917_454`、
  uncached input `92_414`、cache read `1_779_072`，且
  `input + output == total`。
- 增加逐字段 coverage（包含所有 started steps，未结束 step 计入未知）。
  canonical input/output/total 只有在所有 started step 都结束且都有完整
  canonical 字段时才发布；缺 usage、部分 usage 或未结束 step 时保持 `None`。
  `observed_usage` 单独报告 raw 字段的已观察小计，不冒充 canonical 总量。
- `feed` 首次失败后进入终态：后续 `feed` / `finish` 均返回
  `DshError::Poisoned`，不能忽略错误再喂 suffix 得到成功。
- 递归拒绝重复 JSON key（顶层/status/usage/嵌套 tool payload），拒绝
  camel/snake 同义字段同时出现，拒绝未验证的 `cachedInputTokens` /
  `cacheReadInputTokens` 旧别名；错误信息不回显 text/tool payload。
- `sessionId` 要求非空、无控制字符、≤512 字节；CRLF 行长限制文档与实际实现
  一致（`max_line_bytes` 不含 `\r`/`\n`，最多 1 字节 lookahead）。
- 新增/更新 22 个 adapter 测试，覆盖真实 cached fixture、cacheWrite/reasoning、
  字段覆盖、缺 usage、未结束 step、sticky 失败（含超限）、重复 key/别名冲突、
  cache/总量矛盾与溢出。

## 文件

- 修改 `src/dsh.rs`：解析、规范化、coverage、终态、严格 JSON key 检查。
- `src/lib.rs`：未改；已有 `pub mod dsh;` 保持最小接口。
- 修改 `tests/dsh_adapter.rs`：22 个测试。
- 重写 `protocol/DSH-ADAPTER.md`：schema 标识、字段语义、规范化公式、coverage、
  失败终态和未实现边界。
- 修改 `DSH-DELIVERY.md`：本记录。
- 只读使用 `tests/fixtures/dsh-live-smoke.jsonl` 和主控提供的
  `tests/fixtures/dsh-live-development-sanitized.jsonl`；未修改 fixture、manifest、
  runner、既有测试、readiness、数据集或 baseline。

## 实际命令与退出码（本隔离副本）

```text
cargo fmt --all
exit 0

cargo test --test dsh_adapter --locked --offline -j2 -- --test-threads=4
exit 0
22 passed; 0 failed

cargo test --lib --test cli --test invariants --test dsh_adapter \
  --locked --offline -j2 -- --test-threads=4
exit 0
38 passed; 0 failed
(1 lib + 4 cli + 22 dsh_adapter + 11 invariants)

cargo test --all-targets --locked --offline -j2 -- --test-threads=4
exit 101
40 passed; 13 failed；failure 全部在 tests/runner.rs。
runner 15 项中 2 passed / 13 failed；其余 38 项全部通过
（1 lib + 4 cli + 22 dsh_adapter + 11 invariants）。

cargo clippy --all-targets --locked --offline -- -D warnings
exit 0

bwrap --unshare-user --ro-bind / / -- true
exit 1
bwrap: No permissions to create a new namespace, likely because the kernel does
not allow non-privileged user namespaces.
```

13 个 runner 失败的直接表现是 worker `WorkerFailed` / exit 1（或下游
`run_diagnostic(...).unwrap()` 找不到产物），根因是本沙箱不允许 nested
non-privileged user namespace；直接 bwrap 探针同样 exit 1。未修改 runner、未改
权限、未安装依赖。runner 的 15 项须在允许该 namespace 的宿主复验。

## 未实现 / 未验证边界

- 只验证 CLI `0.2.1-rc.3` `--profile headless` 的脱敏事件形状；其他版本必须重新
  采集样本，当前 schema 标识为 `dsh-headless-0.2.1-rc.3.usage-v1`。
- 所有 token 数是 CLI 自报，不是服务商账单；`usage_source` 固定为
  `dsh_cli_self_reported`，`billing_verified=false`。未验证价格、账单或隐藏测试。
- 未启动 DSH/模型/工具，未执行或批准任务，未生成正式 `Observation`，未改变
  readiness，未运行五组实验。
- 未声称正式 Pilot、账单已验证、代码正确或评测通过；`final.text` 中的
  `approved`/`tests passed` 不改变任何结论。

任务在此结束，不扩大范围。
