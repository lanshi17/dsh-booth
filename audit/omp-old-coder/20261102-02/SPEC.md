# SPEC — DSH 适配器审查缺口加固

Tier 3。用户要求“推送并开始下一个任务”；主控将下一任务收敛为修复 OMP 审查 F1–F4，未扩大为真实 Runner 或正式 Pilot。Spec approval: not obtained (autonomous run)。

隔离：仅在本目录显式源码副本中写测试和注释；主控通过验证后只导回两个目标文件。工具：复用 Rust/cargo/Clippy、现有依赖、Python 标准库作本机审计编排；不安装或新增产品依赖。Git：保存初始里程碑后另作正常提交，不改写历史、不强推。新增文件为本 SPEC、验证脚本和输出/新源码摘要/补充审查记录。

## 可执行规格（既有正确行为的回归保护）

S1 reasoningTokens == outputTokens 合法：单步 input=5、cacheRead=0、cacheWrite=0、output=5、reasoning=5、total=10，完成摘要 input/output/total=5/5/10，reasoning=5。测试应捕获将 > 错改成 >= 的变异。
S2 max_total_bytes 为包含换行的整个真实 smoke 夹具字节数时，完整及 1/2/7 字节分块全部可解析完成，bytes_received 精确等于上限。上限少1时返回 TotalBytesLimit 并终止解析器。测试应捕获将 > 错改成 >= 的变异。
S3 turn_start、step_start 后直接 turn_end(completed)，缺 step_end：feed 返回 Protocol，event=4、message=turn_end while a step is open；后续 feed/finish 返回 Poisoned。测试应捕获删除该 guard 的变异。
S4 USAGE_SCHEMA 注释明确未知事件类型及受校验记录字段被拒绝；opaque thinking/tool/trace 载荷字段不校验、不保留。只澄清注释，不改变运行逻辑。

## 验证和约束

新增三个测试先在未改实现上运行；它们应立即通过，因为正确行为已经存在。然后在一次性副本逐一应用三个非等价故障变异，必须观察到对应新测试的断言/行为失败而非编译失败，保留输出。恢复/确认原实现与原快照后才改注释，再执行完整测试、Clippy、格式、readiness 检查（仍应非零）。不弱化既有断言，不修改产品运行逻辑、Cargo、readiness、数据集或旧报告。

提供可重跑脚本；完整输入/输出与源摘要绑定。coverage/LLVM/proptest/fuzzer/随机序工具仍缺失，明确 unavailable/substituted，不能因杀死3个手工变异宣称全面 old-coder 通过。按既定上限，本次是第二轮 fresh-context OMP 独立审查；只验证该新状态，之前盲审保持历史记录。审批/独立人工分级仍未取得。

## 追加 S5 — 第二轮审查后的边界回归（自主范围调整，未取得人工审批）

S5 使用完整真实 smoke JSONL：max_line_bytes 等于最长行的内容字节数（不含 LF）；每行内容按 1/7/整行分块先 feed，LF 单独随后 feed，全部应完成且 bytes_received 等于夹具总长度。上限少 1 应报 LineBytesLimit，后续 feed/finish Poisoned。针对 push_segment 的 >→>= 故障必须观察对应新测试行为失败。实现已正确，不修改运行逻辑、依赖或既有断言。此测试及最终验证脚本在第二轮审查后增加，最终源码独立验证记为 not performed，不能继承先前状态；不发起第三轮。原 S1–S4 与审查记录保持历史。
