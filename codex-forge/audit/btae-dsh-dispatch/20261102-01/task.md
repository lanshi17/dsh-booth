# BTAE-DSH-ADAPTER-01 — Rust DSH JSONL adapter

你是本次被明确调用的 DSH 开发 Worker。主控已经授权本任务，只在当前隔离源码副本工作。当前 cwd 应为 audit/btae-dsh-dispatch/20261102-01/workspace/btae-eval 的绝对路径。不能访问/修改原 btae-eval、superagent-forge、Codex 源码或其他项目。

## 目标
实现一个有界内存、纯 Rust 的 DSH JSONL 离线解析适配器与测试。它只收集真实CLI事件，不执行任务、不启动模型、不批准任务，不将它转换成可信实验 Observation。主控将在你退出后独立验收再导入原工程。

先阅读 src/lib.rs、src/runner.rs 中现有边界、protocol/EXECUTION.md、tests/fixtures/dsh-live-smoke.jsonl 与 dsh-event-key-schema.json。后者只是历史事件类型/键摘要，没有原工具参数或机密内容。本次真实 smoke 来自当前 dsh --profile headless，CLI版本0.2.1-rc.3；数据里 inputTokens=34746、outputTokens=10 是CLI自报，不是服务商账单。session/cwd 已脱敏。

## 允许写入（只有这些）
- 新增 src/dsh.rs。
- 修改 src/lib.rs 仅加入 pub mod dsh（不要改现有函数）。
- 新增 tests/dsh_adapter.rs。
- tests/fixtures/dsh-live-smoke.jsonl 只读；可新增明确标记 synthetic 的 tests/fixtures/dsh-synthetic-*.jsonl。
- 新增 protocol/DSH-ADAPTER.md。
- 新增 DSH-DELIVERY.md，说明改动、命令实际退出码、未实现边界。

不要修改 Cargo.toml/Cargo.lock/main.rs/runner.rs/其他既有测试、数据集、baseline、readiness或历史报告；不安装依赖，不加unsafe，不写Python产品代码，不提交、不推送、不删除文件。不能启动其他 Agent，不要再调用 dsh 或其他模型；不要读凭据文件、环境变量值或API key。

## 接口与行为
自行设计小而清楚的Rust公共接口，至少提供流式 feed bytes + finish(exit code) 和 Read reader 辅助接口。沿用现有 serde/serde_json/thiserror/sha2，不加新依赖。finish 返回来源标注为 dsh_cli_self_reported 的 summary，与正式 Observation 结构分离；不要输出完整文本/工具参数等敏感内容，可记录文本SHA256、事件计数、turn/step信息。

- 支持真实 session、status(turn_start/step_start/step_end/turn_end)、text、final 事件。能记录 tool/trace 等历史类型为 opaque 元数据，不执行其内容；不认识的事件要显式失败或标记未完全支持，不能默默宣称成功。
- 收到final、退出码0、完整turn completed 三者都必须满足才能认为 CLI完成；它仍不代表代码正确/隐藏测试通过/Review通过。取消/失败/截断/缺少final/非零退出不能被宣称完成。没有完整终态时必须可审计。
- step_end.usage 是每个步骤增量，用唯一(turn,step)关联；不同步骤累加，每步重复的usage不能双重计费。重复step_end应拒绝或明确错误。计数必须无负数/浮点/字符串代替整数、溢出检查、cache若支持必须是input的子集，不能加在input之上。
- 缺失usage或部分字段是 unknown/partial，不能填0假装已计量；工具调用次数和价格不能凭猜测补。明确未验证账单。final中即使包含“approved/测试通过”字样，也不能设置实验通过。
- 有总流字节、单行字节、事件数量上限，超限立即失败。不要先用read_to_end/read_line/read_until读入无限数据再检查。拒绝非UTF8/坏JSON/不合理协议顺序，支持输入在任意字节/UTF8位置分块，以及约定的最后一行无换行EOF。

## 测试门槛
真实脱敏smoke可解析，输入输出总量34746/10/34756；并覆盖多步骤、重复usage、缺失usage、部分usage、计数溢出、取消/失败、非零退出、截断/缺final、未知事件、恶意自称approved、坏JSON/非UTF8、长行/总流/事件数限制、分块读取。不要用mirror实现的无效测试。

cargo fmt --all；cargo test --all-targets --locked --offline -j2 -- --test-threads=4；cargo clippy --all-targets --locked --offline -- -D warnings。现有基线31项测试应保持通过。若dsh sandbox使全局cargo缓存只读导致命令无法运行，记录精确错误，不改全局权限、不安装依赖；主控会在宿主验证。

## 文档与退出
写清实际CLI版本范围、事件字段、增量统计规则、哪些未验证，不能把adapter解析器写成完整真实Runner。保持Pilot全部readiness原值，未运行五组实验，不造结果。DSH-DELIVERY.md中列出实际测试结果并结束本任务，不继续扩大范围。尽量在12次工具调用内完成，先批量读文件，再集中实现与检查。
