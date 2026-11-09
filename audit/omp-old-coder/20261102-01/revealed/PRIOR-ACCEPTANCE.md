# DSH 开发任务与适配器验收

日期：2026-11-02。状态：**离线适配器经主控补正后验收通过；正式 BTAE 五组实验 NOT RUN。**

已实际调用 `dsh --profile headless --json -`，使用用户确认的 `ark / deepseek-v4.1-flash` 配置，在隔离源码副本派发 `BTAE-DSH-ADAPTER-01`。产品侧交付为独立纯 Rust JSONL 解析器，不引入 DSH SDK、Node.js 或 DSH Runtime 依赖。配置中的模型别名没有被证明是不可变模型版本。

## 实际结果

| 阶段 | 进程结果 | 独立判定 |
| --- | --- | --- |
| 真实 smoke | 退出 0，7 个事件，收到 BTAE_DSH_READY | headless 调用可用；不是研究数据 |
| R0 开发派发 | 退出 0，23 个步骤、35 个工具调用，正常结束 | 宿主 44 项测试通过，但独立检查发现统计与解析失败状态问题，退回修改 |
| R1 修正派发 | 600 秒超时，退出 124；73 步开始、72 步结束、75 个工具调用 | 文件已交付但未正常结束；末尾 final 早于 turn_end，解析器拒绝该日志，不能宣称 CLI 完成 |
| 主控 R2 | Worker 退出后审查、补正并导入 | 54 项项目测试及 16 项独立本机检查通过；Clippy 和格式检查通过 |

R1 中 DSH 的开发沙箱不能再创建 Bubblewrap user namespace，Worker 自测因此报告原 runner 测试失败。主控在支持该 namespace 的宿主完整复验，31 项原测试均通过。当前 54 项项目测试包含这 31 项和 23 项适配器测试。独立 16 项检查位于本机审计目录，另行依赖导入后的主项目验证，不是研究样本。

## 接收的变更

- [src/dsh.rs](../src/dsh.rs)：有界流式解析、CLI 完成判定、增量计量、字段覆盖、敏感载荷摘要、重复键检查和失败终态。
- `src/lib.rs` 仅新增模块导出；[适配器测试](../tests/dsh_adapter.rs)及两个真实脱敏事件夹具、历史事件键摘要。
- [协议说明](../protocol/DSH-ADAPTER.md)：固定已验证 CLI schema、公开接口与未实现边界。

未改 Cargo 依赖、既有运行器、数据集、五组 SOP 或 readiness；未提交或推送。原源码在 Worker 运行期间保持基线摘要不变，导入只包含审查过的文件。README 与路线图随后更新以反映这一阶段的实际范围。

## 发现与纠正

1. **主控原任务的缓存假设有误。** 安装的 DSH headless/token-meter 源码和全部 23 条真实 usage 表明，raw `inputTokens` 是未缓存输入，`cacheReadTokens`、`cacheWriteTokens` 是额外且互不重叠的输入分项。已撤销错误要求，更正独立测试的输入期望并保留旧版与失败输出。
2. R0 把已观察到的小计当作完整输入/输出，且未结束步骤不影响计量完整性。现按所有 started steps 计算逐字段覆盖；不完整总量保持 null，小计单独报告。
3. R0 在 feed 错误后仍可接受后续输入，并会静默覆盖重复 JSON 键。现任何 feed 错误使解析器终止，递归拒绝重复键和冲突别名，错误不回显载荷。
4. R1 已通过宿主 53 项测试及最初 14 项独立检查；补充检查仍发现部分 usage 可以上报低于已知分项的总量。主控 R2 加入已知 token 下界和溢出检查、回归测试。R0、R1 原产物及各阶段失败证据均保留。
5. R1 超时会留下一个 final 事件，但 turn/step 仍未结束。真实超时日志被明确拒绝；final 的存在不代表任务成功或正常 CLI 结束。

## Token 与成本边界

smoke 的 CLI 自报总量是 34,756。R0 完整开发日志的未缓存输入为 92,414、缓存读取为 1,779,072、全部输入为 1,871,486、输出为 45,968，总量为 1,917,454。只累加唯一 step_end 的统计，reasoning 不再次加入 output。原始明细缺失保持未知，不假定缓存写入等于零。

这些数字全部标注 `dsh_cli_self_reported`，`billing_verified=false`；没有核验服务商账单、价格或失败请求计费。R1 日志截断且协议不完整，不能发布完整成本。该开发派发的统计也不是正式 Pilot 的执行预算或方法效果数据。未来真实运行仍需处理 Profile 固定开销与步骤上报滞后，不能直接沿用普通无开销调用的预算假设。

## 验证与剩余工作

最终在主项目执行 `cargo test --all-targets --locked --offline -j 2 -- --test-threads=4`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo fmt --all --check`，均退出 0。`check-ready datasets/pilot.json` 按预期退出 1，仍有方法实施、独立真值复核、镜像/模型/Profile 冻结等开跑条件未完成。

适配器只消费已有字节流，尚未接入真实 DSH 进程监督、凭据/网络隔离、正式证据收集器或 M1–M6 干预。**五组对照、消融、长期演进研究都未运行，不能从本次验收得出 BTAE 优于其他方法的结论。**

机器摘要：[dsh-adapter-acceptance.json](dsh-adapter-acceptance.json)。本机审计材料位于 [20261102-01](../../audit/btae-dsh-dispatch/20261102-01)，含任务、原始事件、阶段归档、修正说明、独立检查及命令输出。原始事件可能包含完整开发载荷，留本机保存；主项目只包含脱敏夹具。
