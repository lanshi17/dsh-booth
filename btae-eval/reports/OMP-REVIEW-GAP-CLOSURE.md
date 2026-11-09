# DSH 离线适配器：审查缺口回归加固

2026-11-02。四个回归测试与一处注释澄清已落地；运行逻辑、依赖、公开 API、数据集与 readiness 未改变。完整测试 **58/58**，其中 DSH 适配器 **27/27**；Clippy `-D warnings` 和格式检查通过。真实五组 Pilot **NOT RUN**，readiness 仍按预期退出 1。

这是既有正确行为的回归保护，不能表述为修复了四个已存在的运行故障，也不能据此声称 BTAE 优于其他方法。原始 [OMP 审查](OMP-OLD-CODER-REVIEW.md) 保留为历史。

## 规格与证据

隔离源码副本验证后只导回 `src/dsh.rs` 的注释及 `tests/dsh_adapter.rs` 的新增测试。spec approval: **not obtained (autonomous run)**；没有人工事前 SPEC 审批或审查发现分级。

| 规格 | 测试 / 故障变异 | 最终结果 |
| --- | --- | --- |
| S1 reasoning = output 合法 | `reasoning_equal_to_output_is_a_valid_subset_boundary`；`>` 改为 `>=` | 通过 / 变异被杀死 |
| S2 总字节上限含边界、含换行 | `exact_total_byte_limit_accepts_whole_and_chunked_real_transcripts`；总量 `>` 改为 `>=` | 511 字节夹具、1/2/7/整包分块通过；少 1 拒绝并 Poisoned / 变异被杀死 |
| S3 未关闭 step 不允许结束 turn | `turn_end_with_an_open_step_is_a_terminal_protocol_error`；删除 open_step guard | Protocol event=4 与消息、后续 Poisoned 通过 / 变异被杀死 |
| S4 schema 注释范围准确 | 仅修改注释：严格校验记录拒绝未知字段，opaque thinking/tool/trace 载荷不校验；标识不能验证生产 CLI 版本 | 第二轮 OMP 核对准确；无运行逻辑改动 |
| S5 恰好达到行上限，LF 后到 | `exact_line_byte_limit_accepts_content_before_a_later_lf`；push_segment 的 `>` 改为 `>=` | 完整合法 JSONL 按 1/7/整行分块成功；少 1 拒绝并 Poisoned / 变异被杀死 |

新增测试对原正确逻辑立即通过；对应一次性副本中的非等价变异均导致指定测试行为失败，**4/4** 退出 101，编译错误不计为杀死。S5 使用真实 smoke 夹具完整解析，补强了审查者仅用 `12345` 缓冲前缀证明的差异。最终源码输入清单含 53 个可执行验证所需文件，树摘要：

`acc07d47146063626590f5441e63f0307966370e8d2b1b01ff12676b9aa8bf62`

隔离副本与主工作区均实际运行全套测试、Clippy、fmt、readiness，结果一致。产物/私有真值不复制到验证副本；主工作区另作验证，代码及夹具集合、逐文件摘要均匹配。详细退出码与源码摘要见 [机器报告](OMP-REVIEW-GAP-CLOSURE.json)。

## 独立验证的范围

**Independent verification: not performed against the final source state.** 已进行两轮 fresh-context OMP 审查（上限 2）；第二轮审查状态为 `eee74ab153cc739b0855e1613b19bc9c846ed0f83700d66abbc7cfb610554f70`，复现 57 个测试与 3/3 定向变异，并找到 S5 的测试缺口。host OMP 18.15.0，模型 `commandcode/deepseek/deepseek-v4.1-flash`；仅降低任务上下文相关性，不保证模型独立性或不可变版本。

第二轮首次读取 TASK-CONTRACT.md 时文件尚未落盘；文件在约 20 秒后写入。这是主控准备错误，故完整盲审协议保持 **blocked/incomplete**。盲审记录封存后，同一会话追加契约与证据比对，确认 S1–S4 和执行数字一致；不能把事后核对当成完成了盲审。没有发起第三轮。之后新增 S5 与最终验证脚本，最终状态明确降低为 **not performed**，不继承之前状态。人工未对发现分级，也未批准停止独立复核；本报告交付有限自动验证证据，不声称完整 old-coder 验收。

盲审攻击覆盖运行复现、摘要绑定、测试变异、checker 失败路径、规格映射；原始结果和追加比较分别保留在审计目录 `output/ROUND2-BLIND.md`、`output/ROUND2-COMPARISON.md`。最终 checker 另外拒绝了被篡改输入和仍然通过 27 测试的等价注释编辑；这只证明这些已知反例到达失败路径，不能证明 checker 完备。

## 可复现命令及局限

从仓库根目录运行（Rust/Clippy/Python 版本见审计目录 toolchain.json；离线依赖需已缓存，隔离测试需 Linux bwrap/namespaces）：

```sh
python3 audit/omp-old-coder/20261102-02/final_verify.py output/replay-final
python3 audit/omp-old-coder/20261102-02/landing_verify.py output/replay-landing
python3 audit/omp-old-coder/20261102-02/checker_controls.py
```

输出目录必须不存在，历史结果不可覆盖。checker_controls 的固定输出目录为 output/checker-controls，重跑前保留原目录并移走它。从干净 checkout 复现旧审查状态，可先运行 `reconstruct_round2.py`，再运行 `verify.py output/replay-round2`；它从已推送的基线提交和两个封存覆盖文件恢复 53 个精确输入，不依赖忽略的源码副本。重建过程已验证，盲审封存哈希未改变。

曾因准备脚本假设错误的摘要 JSON 字段而在导入前中止，已按真实格式修正；此前验证脚本的重复输出路径碰撞也已修正。最终运行均使用新输出目录，未删除失败断言或改变预期退出码。

未运行 changed-line/branch coverage 阈值、fuzzing/proptest、随机顺序测试、最低 Rust 版本矩阵、全量自动变异或审查 canary；本次只添加边界回归和注释，定向手工变异不能替代这些层。依赖未变，未新增供应链审计。没有真实 DSH 进程收集/可信账单或五组效果实验结果。下一实施项仍是受隔离的真实 DSH 进程收集路径，正式 Pilot 仍需方法忠实度、独立真值和冻结条件。
