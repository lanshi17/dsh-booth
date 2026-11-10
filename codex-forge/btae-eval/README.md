# BTAE-Eval：工程方法实证评估准备包

状态：**DRAFT / NOT RUN**。本次交付实验协议、五组操作流程草案、24 条候选连续变更任务、4 个可运行的小型诊断仓库、Rust 排程/指标计算器、Linux 隔离诊断运行器、DSH 离线事件适配器、headless 工程采集器及反例测试。没有正式五组 Agent 比较结果，也没有“BTAE 优于 SDD”的结论。另有真实 DSH 工程派发和离线事件适配器验收，独立于研究数据。

评估工具使用 Rust 2024；Python 只用于被测代码样例。工程是独立 Cargo Workspace，不依赖 Codex、DSH SDK 或 Forge 尚未实现的 Runtime。正式运行选择 commandcode / deepseek/deepseek-v4.1-flash、统一 DSH headless，是用户已确认的配置；尚未冻结模型不可变版本、容器和 Profile 摘要。

```sh
cd btae-eval
cargo test --locked --offline
cargo run --locked -- validate datasets/pilot.json
cargo run --locked -- plan datasets/pilot.json
cargo run --locked -- check-ready datasets/pilot.json  # 预期非零：列出未完成的开跑条件
cargo run --locked -- score examples/scorer-fixture.json
cargo run --locked -- bootstrap examples/bootstrap-fixture.json
```

score/bootstrap 示例是人工构造的评分器夹具，不能当作研究结果。plan 只输出计划，不执行任何模型、CLI 或网络请求。新增 diagnose 仅启动受隔离的假 Worker，绝不调用模型；它的退出成功只表示诊断记录已收集，须查看 outcome 字段。

| 材料 | 用途 |
| --- | --- |
| [研究协议](protocol/STUDY.md) | 比较对象、主要终点、统计单位、失败规则、成本和判定标准 |
| [机制规格](protocol/METHOD.md) | M1–M6 的输入/输出/约束与消融边界 |
| [基线操作流程](baselines/README.md) | 五组差异、共同信息和实施忠实度 |
| [DSH commandcode 配置](config/README.md) | 当前模型路由、专用 Profile、环境凭据与隔离任务命令 |
| [DSH 工具部署检查](config/DSH-TOOLS.md) | 重复模块身份、修复/回滚与独立公开契约检查 |
| [执行与隔离](protocol/EXECUTION.md) | DSH 配置冻结、运行记录、隐藏评估与禁止泄漏 |
| [标注手册](protocol/ANNOTATION.md) | 独立双人 Ground Truth、盲审和实体匹配 |
| [依次实施任务](protocol/ROADMAP.md) | 运行真实 Pilot 前的剩余交付 |
| [候选数据](datasets/pilot.json) | 4 仓库 × 6 轮 × 5 组 × 3 重复 = 360 次计划执行 |
| [运行器验证](reports/RUNNER-VALIDATION.md) | Linux 进程、预算、取消、隔离、快照与完整性验证；真实实验仍未运行 |
| [DSH 适配器验收](reports/DSH-ADAPTER-ACCEPTANCE.md) | 真实 headless 调用、独立复验、缓存统计更正与超时记录；不是五组效果实验 |
| [DSH headless live collector](reports/DSH-LIVE-COLLECTOR.md) | Pure Rust bounded external process collector；73 tests、工程 smoke 通过；正式 Pilot 未运行 |
| [DSH 边界回归加固](reports/OMP-REVIEW-GAP-CLOSURE.md) | 58 项测试、4/4 定向变异与独立复核限制；无正式效果结果 |
| [准备阶段报告](reports/PREPARATION.md) | 本次验证和未解决事项；没有效果数据 |

当前任务只有 small 合成仓库、6 轮、单 Agent；不等于用户设想的中型项目、20–30 轮、多 Agent 或外部真实仓库验证。24 个 task 不是 24 个独立仓库；360 次执行形成 60 条分组轨迹，推断时仍要尊重 4 个仓库簇。

evaluator-private 在开发 checkout 中可被读取；目录名不构成隔离。真实 Worker 只允许获得导出的公开输入，不能挂载本目录。所有候选真值仍为 agent-authored 草稿，必须独立复核后冻结。

## Linux 运行器诊断

需要 `/usr/bin/bwrap`、可用的 user/pid/network/mount namespaces；不允许失败后回退成宿主机无隔离执行。

```sh
cargo build --bins --locked --offline
mkdir -p runs
cargo run --locked -- diagnose --fixture-worker target/debug/btae-fixture-worker --output runs/smoke-01 --scenario retry
cargo run --locked -- verify-diagnostic runs/smoke-01 datasets/pilot.json
```

输出目录必须不存在，已有结果不可覆盖。run 的 data_kind 为 runner_diagnostic_only，hidden_test_result 与 empirical_task_success 恒为 null；诊断不能反序列化成正式 Observation。默认允许诊断第1轮公开种子，不将后续轮次偷偷重置到初始代码。

诊断的 token 是假 Worker 自报的计数，验证器检测累计超额后终止进程；这不是服务商硬配额或可信账单。已实现 [DSH 离线 JSONL 适配器](protocol/DSH-ADAPTER.md)，支持流式解析、缓存分项、字段覆盖和失败终态；真实 DSH 进程收集由独立 `collect` 提供；尚未接入正式调度、可信账单或六机制干预。


## Linux DSH headless 工程采集

`collect` 启动一次外部进程，输出脱敏收据。未指定 `--arg` 时参数默认为 `--profile headless --json -`；指定后使用完整自定义参数数组。输出目录必须尚不存在。prompt 从文件或 stdin 读取，上限 64 KiB；进程 wall 上限 300 秒，CLI 输入读取和最终收据写入不计入该时限。

```sh
cargo build --bins --locked --offline
# 无模型调用的本地夹具演示
./target/debug/btae-eval collect --executable "$PWD/target/debug/dsh-json-fixture" \
  --arg=--scenario --arg=success --prompt /dev/null --output runs/collect-demo
```

真实 DSH 应使用调用方配置的 bwrap/容器启动器、专用 headless Profile 和网络策略，通过重复 `--arg=...` 提供启动参数。`--pass-env NAME` 仅按名称选择已有环境值；凭据值不可放入 argv。该命令自身不设置 OS 隔离，始终记录 `network_isolation=caller_supplied`。退出 0 表示进程与 JSONL 完成，其他终态退出 1；它不产生正式研究 Observation。[验收与限制](reports/DSH-LIVE-COLLECTOR.md)包含可重跑入口。
