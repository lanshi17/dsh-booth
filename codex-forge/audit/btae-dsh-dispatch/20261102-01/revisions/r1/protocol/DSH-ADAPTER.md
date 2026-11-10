# DSH JSONL 离线适配器（BTAE-DSH-ADAPTER-01 R1）

## 范围

`src/dsh.rs` 是一个**只读、离线、纯 Rust** 的 DSH `--profile headless`
JSONL 解析器。它只消费已有字节流：不启动 `dsh`、不启动模型、不执行或批准工具、
不访问网络/凭据、不修改 workspace，也不生成 `Observation`。

已验证的 CLI/schema 标识为 `dsh-headless-0.2.1-rc.3.usage-v1`（CLI
`0.2.1-rc.3`，`--profile headless`）。验证输入只有：

- `tests/fixtures/dsh-live-smoke.jsonl`（1 turn / 1 step，真实脱敏 smoke）；
- `tests/fixtures/dsh-live-development-sanitized.jsonl`（141 个真实事件的脱敏
  结构，保留顺序和全部数值，仅替换 text/tool 内容，23 steps）。

`tests/fixtures/dsh-event-key-schema.json` 只是历史事件键摘要，不是 schema
承诺。任何新增事件类型、status/session/usage 字段或不同字段语义都不会被静默
解释：未知字段和未知事件直接失败。没有证据支持本适配器可无条件用于其他 DSH
版本；其他版本必须重新采集样本并新增验证。

## 公共接口

```rust
let mut adapter = dsh::DshAdapter::new(dsh::DshLimits::default());
adapter.feed(chunk)?;                  // 任意字节 / UTF-8 边界
let summary = adapter.finish(Some(0))?;

let summary = dsh::read_dsh(reader, limits, Some(0))?; // Read 辅助接口
```

`finish` 对“缺 final / 非零退出 / turn 未完成 / step 未结束”等**返回可审计的
summary**（`completed=false` 与 `completion_reasons`），不伪造成功。坏 JSON、
非 UTF-8、重复 key、未知 schema、协议/usage 错误和超限返回 `DshError`。

## 有界输入

默认 `DshLimits`：

- 总流 `16 MiB`，包括换行符和 CR；
- 单条 JSONL `1 MiB`，**不含**末尾 `\n` 和可选 `\r`；
- 最多 `100_000` 条非空 JSONL 记录；
- JSON 嵌套深度受 `serde_json` 默认 128 层递归限制约束。

字节在保留前检查上限；不使用 `read_to_end`、`read_line` 或 `read_until`。
`read_dsh` 以 8 KiB 块读取。单行最多额外保留 1 个字节，用于判断末尾是否可能是
待剥离的 `\r`。UTF-8 多字节字符可以跨 `feed` 边界，因为只在完整行上做 UTF-8
和 JSON 解析。最后一行没有 `\n` 时在 `finish` 中处理；空行立即报协议错误。

## 事件

| `type` | 处理 |
| --- | --- |
| `session` | 必须是第 1 条且唯一；`sessionId` 必填、非空、无控制字符、≤512 字节；`sessionId`/`cwd` 只保存 SHA256 |
| `status` | `turn_start`、`step_start`、`step_end`、`turn_end` |
| `text` | 只累计文本 SHA256 和条数 |
| `final` | 至多一条，必须在所有 turn 结束之后；只保存文本 SHA256 |
| `thinking` | opaque：只计数 |
| `tool_call` / `tool_result` | opaque：只计数，不解析/保存 `input`、`result` |
| `trace` / `tool` | opaque：只计数 |

未知 `type` 返回 `UnsupportedEvent`，不会静默跳过。`final` 之后再有事件、session
不是首事件、重复 session、嵌套 turn/step、步号或 turn 号不连续、`step_end`
重复、`turn_end` 缺 reason、`final` 早于 `turn_end`、`turn_end` 携带 usage
等均报协议错误。`status`/`session` 的可选字段若显式写成 `null`（而不是省略）
也报协议错误。`reason.kind` 的未识别值会标记为 `Other` 并使 turn 不完成，
不会被当作成功。

## usage 语义（已纠正）

真实 DSH 0.2.x headless 的 `step_end.usage` 字段：

| 字段 | 语义 |
| --- | --- |
| `inputTokens` | **uncached** prompt token |
| `cacheReadTokens` | 额外的 prompt bucket，与 uncached input 不重叠 |
| `cacheWriteTokens` | 额外的 prompt bucket，与 uncached input / cache read 不重叠 |
| `outputTokens` | output token |
| `totalTokens` | canonical prompt（含所有 cache bucket）+ output |
| `reasoningTokens` | output 的子集，**不可**再加到 output/total |

原任务“cache 是 raw input 的子集、不能加在 input 之上”的假设不适用于真实
DSH，已撤销。`cacheReadTokens`/`cacheWriteTokens` 不是 `inputTokens` 的子集，
而是额外的 prompt bucket；但也不能重复相加：canonical input 只由
`uncached + cacheRead + cacheWrite` 或 `totalTokens - outputTokens` 得到一次。

`step_end.usage` 是**该步骤所有 attempt 的累计增量**。适配器按唯一
`(turn, step)` 关联并只累加 `step_end`；没有 turn 级 usage，`turn_end` 携带
usage 会被拒绝。重复 `step_end` 直接拒绝，避免重复计费。

### 单步规范化

1. 只接受 JSON 非负整数。负数、浮点、字符串、布尔、null 均拒绝；所有加法使用
   `checked_add`，溢出即 `DshError::Usage`。
2. 如果同时有 `totalTokens` 和 `outputTokens`：
   - `canonical_input = totalTokens - outputTokens`（`total < output` 拒绝）；
   - `canonical_input` 必须不少于已知 prompt bucket 之和（存在的 `inputTokens`
     + 存在的 cache read/write）；
   - 如果 `inputTokens`、`cacheReadTokens`、`cacheWriteTokens` **三个都存在**，
     则必须严格相等：`totalTokens - outputTokens == inputTokens +
     cacheReadTokens + cacheWriteTokens`。这是“两个 cache 字段都存在时必须相等”
     的检查；矛盾即拒绝。
3. 否则，如果 `inputTokens`、`cacheReadTokens`、`cacheWriteTokens` 三个都存在：
   `canonical_input = inputTokens + cacheReadTokens + cacheWriteTokens`。
4. 否则 canonical input 未知；不得把 `inputTokens` 直接当作完整 input，也不得
   把缺失 cache 当作 0。
5. `canonical_total = canonical_input + outputTokens`（仅当两者已知）；若原始
   `totalTokens` 存在，必须与它相等。
6. `reasoningTokens` 与 `outputTokens` 同时存在时必须
   `reasoningTokens <= outputTokens`；output 缺失时 reasoning 只作为 observed
   raw 字段保留，该 step 仍为 `Partial`。reasoning 只作为 output 子集单独记录，
   绝不再加到 output/total。

关键边界：

- `totalTokens == inputTokens + outputTokens` 且 cache 字段省略：canonical
  input/total 可由 `total - output` 完整得到；但 cache 明细仍是 **unknown**，
  summary 中保持 `None`，不假设为 0。
- 没有 `totalTokens` 且 cache bucket 不全：canonical input/total 不保证完整，
  必须是 `None`（`Partial`），即使 `inputTokens + outputTokens` 看起来“自洽”。
- 有 `totalTokens` 但缺 `outputTokens`：无法由 total 反推 canonical input；
  只有三个 prompt bucket 全知道时才可能得到 canonical input，且 canonical
  total 仍因 output 未知而保持 `None`。

真实 development fixture 的 23 个 step 规范化后：

- raw `inputTokens` 小计 `92_414`（uncached）；
- `outputTokens` 小计 `45_968`；
- `cacheReadTokens` 小计 `1_779_072`；
- raw `totalTokens` 小计 `1_917_454`；
- canonical input = `1_917_454 - 45_968 = 1_871_486`；
- 与 `92_414 + 1_779_072 = 1_871_486` 一致；
- canonical total = `1_917_454`；`cacheWriteTokens`/`reasoningTokens` 在样本中
  未出现，保持 `None`。

首个真实 step 的样例为 `inputTokens=25193`、`outputTokens=327`、
`totalTokens=36272`、`cacheReadTokens=10752`：
`36272 - 327 = 35945 = 25193 + 10752`。

### 字段覆盖与汇总

- `steps_total` 是**已 started** 的 step 数，包括已 `step_start` 但未
  `step_end` 的 step；`steps_ended` 是已结束 step 数。
- `usage_coverage` 对每个字段分别记录 `observed_steps`、`total_steps` 和
  `complete`。`total_steps` 始终等于 started steps，未结束 step 计入未知。
- `DshTokenUsage.input_tokens` 是**所有 input（含 cache）**。canonical
  input/output/total 只有在 `usage_status == Complete` 时才有值：
  - 每个 started step 都已结束；且
  - 每个 step 都有 canonical input、output、total。
- 任一 step 缺 usage、字段不全或未结束：canonical input/output/total 全部为
  `None`，同时 `usage_status` 为 `Partial`。
- 完全没有 usage：`usage_status == Unknown`，所有字段 `None`，绝不填 0。
- `observed_usage` 只报告实际观察到的 raw 字段小计，可以覆盖少于全部 step。
- `DshTokenUsage` 中的 raw 字段（`uncached_input_tokens`、`cache_read_tokens`、
  `cache_write_tokens`、`reasoning_tokens`）只在该字段覆盖所有 started steps
  时才是全 transcript subtotal，否则为 `None`。它们不是 canonical
  input/output/total，也不会被重复加到 canonical total。
- 例如：step 1 有 cache、step 2 没有 cache，则 `usage_status` 仍可能因
  canonical 总量完整而为 `Complete`，但 `cache_read_tokens` 为 `None`，
  `observed_usage.cache_read_tokens` 报告已知小计，coverage 显示 1/2。
- 例如：step 2 已 `step_start` 但 EOF 未 `step_end`，即使 step 1 完整，
  `usage_status` 也不能是 `Complete`，canonical 总量为 `None`。

## 失败必须终态

`feed` 首次返回错误后 adapter 进入终态：后续任何 `feed` 和 `finish` 都返回
`DshError::Poisoned`。调用方不能忽略坏 JSON / 非 UTF-8 / 超限 / 协议或 usage
错误后再喂入 suffix 得到 `completed=true`。`finish` 也拒绝在终态上返回 summary。

错误信息只包含固定类别、事件序号和上限值，不保留或回显 text/tool payload。

## 重复 key 与别名

JSON 行先通过递归 `StrictValue` 解码，再转成 `serde_json::Value`；任何对象
（包括顶层、`status`、`usage`、tool payload 内嵌对象）出现重复 key 即
`InvalidJson`，不会静默 last-wins。覆盖：

```json
{"type":"status","phase":"step_end","turn":1,"step":1,
 "usage":{"inputTokens":99,"inputTokens":1,"outputTokens":1}}
```

会因重复 `inputTokens` 被拒绝。

usage 字段接受 camelCase 和对应 snake_case 别名，但同一语义字段的两种拼写
**同时出现**即 `Usage` 歧义错误，不因值相同而放行。未验证的旧别名
`cachedInputTokens`、`cacheReadInputTokens` 等作为未知字段拒绝，不会被当作
`cacheReadTokens` 累加。

## 完成判定

`DshSummary.completed == true` 仅当同时满足：

1. `exit_code == Some(0)`；
2. 收到 `final`；
3. 至少一个 turn，且所有已打开 turn 都以 `reason.kind == "completed"` 结束，
   没有未关闭的 turn/step；
4. session 存在且有效。

`usage_status` 与 `completed` 是独立维度：CLI 可以完整结束但 usage 缺失；
usage 完整也不代表代码正确、隐藏测试通过、Review 通过、实验被批准或账单已核实。
`final.text` 即使包含 `approved`/`tests passed` 也不会改变任何实验结论。
`usage_source` 恒为 `dsh_cli_self_reported`，`billing_verified` 恒为 false。

## 明确未实现

- 进程启动、实时读取、取消/超时控制（属于 runner，不在本模块）；
- 凭据、网络、模型调用、工具执行或 tool 参数/结果语义；
- 完整 DSH 所有版本/所有事件；未识别事件/字段一律失败；
- provider 账单、价格、隐藏测试、正式 `Observation` 或数据可信性认证；
- readiness、实验 arm 或五组运行中的任何改变。
