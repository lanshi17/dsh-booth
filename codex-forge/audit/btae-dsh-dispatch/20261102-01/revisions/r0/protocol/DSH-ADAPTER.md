# DSH JSONL 离线适配器（BTAE-DSH-ADAPTER-01）

## 范围

`src/dsh.rs` 是一个**只读、离线、纯 Rust** 的 DSH `--profile headless` JSONL
解析器。它消费已有字节流，不启动 `dsh`、不启动模型、不执行或批准工具、不访问
网络/凭据、不生成 `Observation`。

已验证的输入是 CLI `0.2.1-rc.3` 生成的脱敏 smoke
（`tests/fixtures/dsh-live-smoke.jsonl`）。`tests/fixtures/dsh-event-key-schema.json`
只是历史事件键摘要；适配器不据此假设完整 schema。没有证据支持它可无条件用于
其他 DSH 版本，当前声称的兼容范围仅为 **0.2.x headless 中与下列事件形状一致的
输出**；其他版本必须重新采集 smoke 并以本适配器的新测试验证。

## 公共接口

```rust
let mut adapter = dsh::DshAdapter::new(dsh::DshLimits::default());
adapter.feed(chunk)?;                 // 任意字节/UTF-8 边界
let summary = adapter.finish(Some(0))?;

let summary = dsh::read_dsh(reader, limits, Some(0))?; // Read 辅助接口
```

`finish` 对“缺 final / 非零退出 / turn 未完成”等**返回可审计的 summary**
（`completed=false` 与 `completion_reasons`），而不是伪造成功；坏 JSON、非 UTF-8、
协议顺序错误和超限返回 `DshError`。`finish` 消费 adapter，不能在结束后继续喂入。

## 有界内存与输入规则

默认 `DshLimits`：

- 总流 `16 MiB`（包括换行符）；
- 单条 JSONL `1 MiB`（不含 `\n`，允许 CRLF 并去掉 `\r`）；
- 最多 `100_000` 条非空 JSONL 事件。

字节在保留前检查上限；不使用 `read_to_end`、`read_line` 或 `read_until`。
`read_dsh` 以 8 KiB 块读取。最后一行没有换行符时在 `finish` 中处理。空行立即报错。
UTF-8 多字节字符可以跨 `feed` 边界，因为只在完整行上做 UTF-8 和 JSON 解析。

## 事件

支持的真实事件：

| `type` | 处理 |
| --- | --- |
| `session` | 必须是第 1 条且唯一；`sessionId`/`cwd` 只保存 SHA256，不保存原值 |
| `status` | `turn_start`、`step_start`、`step_end`、`turn_end` |
| `text` | 只累计文本 SHA256 和条数 |
| `final` | 至多一条，必须在所有 turn 结束之后；只保存文本 SHA256 |
| `thinking` | opaque：只计数 |
| `tool_call` / `tool_result` | opaque：只计数，不解析/保存 `input`、`result` |
| `trace` / `tool` | opaque：只计数 |

未知 `type` 返回 `UnsupportedEvent`，不会静默跳过。`final` 之后再有事件、session
不是首事件、重复 session、嵌套 turn/step、步号或 turn 号不连续、`step_end`
重复、`turn_end` 缺 reason、`final` 早于 `turn_end` 等均报协议错误。

## usage 规则

`step_end.usage` 是对应 `(turn, step)` 的**步骤增量**。适配器按唯一键计数；
重复 `step_end` 直接拒绝，避免重复计费。可用字段：

- `inputTokens` / `input_tokens`
- `outputTokens` / `output_tokens`
- `totalTokens` / `total_tokens`
- `cachedInputTokens` / `cacheReadInputTokens` / 对应 snake_case

规则：

- 只接受 JSON 非负整数；负数、浮点、字符串由 serde 拒绝。
- 所有累加使用 `checked_add`；溢出即 `DshError::Usage`。
- `totalTokens` 不直接累加。每个完整步骤要求其等于 input + output；只有
  `usage_status == Complete` 时 summary 的 total 才由累计 input + output 派生，
  否则保持 `None`，避免把部分步骤的和当成完整总量。
- cache 必须与 input 同一步出现且 `cache <= input`；cache 是 input 的子集，
  **绝不**加到 input 或 total 之上。
- 某步缺 usage 或字段不全：summary 中未知字段保持 `None`，不填 0；状态为
  `Unknown`/`Partial`。只有每个已结束步骤都报告 input 与 output 才是 `Complete`。
- 工具调用次数只来自实际出现的 opaque 事件计数，不估计；没有价格字段，也不
  推断费用。

`usage_source` 恒为 `dsh_cli_self_reported`，`billing_verified` 恒为 false。
smoke 中 `inputTokens=34746`、`outputTokens=10`、`totalTokens=34756` 是 CLI 自报，
不是服务商账单。

## 完成判定

`DshSummary.completed == true` 仅当同时满足：

1. `exit_code == Some(0)`；
2. 收到 `final`；
3. 至少一个 turn，且所有已打开 turn 都以 `reason.kind == "completed"` 结束，
   没有未关闭的 turn/step。

这不表示代码正确、隐藏测试通过、Review 通过、实验被批准或计费已核实。
`final.text` 即使包含 `approved`/`tests passed` 也不会改变任何实验结论。
summary 中没有任何 `hidden_test_passed`、`approved` 或实验成功字段。

## 明确未实现

- 进程启动、实时读取、取消/超时控制（属于 runner，不在本模块）；
- 凭据、网络、模型调用、工具执行或 tool 参数/结果语义；
- 完整 DSH 所有版本/所有事件；未识别事件一律失败；
- provider 账单、价格、隐藏测试、正式 `Observation` 或数据可信性认证；
- readiness、实验 arm 或五组运行中的任何改变。
