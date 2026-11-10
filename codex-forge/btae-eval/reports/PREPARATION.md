> 本文保留最初准备阶段的15项测试结果；后续31项测试及隔离运行器见 [RUNNER-VALIDATION.md](RUNNER-VALIDATION.md)。原 validation.json 为该阶段历史源码快照，不代表当前源码摘要。

# BTAE Pilot 准备阶段报告

日期：2026-11-02。状态：**基础设施准备验证通过；真实方法实验 NOT RUN**。

## 本次实际交付

- 研究协议：唯一主要比较 E−C，主要终点为维护任务成功率，预先定义预算、信息公平性、分母、失败与成本口径。
- M1–M6 机制规格及五组 SOP 草案；不是六机制的生产实现，也不是五组已经运行的证据。
- 4 个可运行的 Python 标准库小型诊断种子；每个 6 条候选变更任务，共 24 条。
- 固定种子的五组排程：每任务每组 3 次重复，共 360 次计划 episode、60 条轨迹。统计上保留 4 个仓库簇，不能用 360 作为独立样本数。
- Rust 2024 独立 CLI：validate、check-ready、plan、score、bootstrap；不调用模型。
- 影响集合 Precision/Recall/F1、逐规则架构率、失败/预算判定、总成本和仓库配对 bootstrap 的计算骨架。
- 消融预案、独立标注规程、DSH 运行器隔离与证据接口，后续任务清单。

## 验证结果

| 项目 | 实测 |
| --- | --- |
| Rust 测试 | 15 项通过，0 失败 |
| Clippy | --all-targets -D warnings 通过 |
| 格式 | fmt --check 通过 |
| 公开种子检查 | 4 个仓库各 1 项 unittest 通过，仅检查未修改的初始代码 |
| 排程 | 两次输出一致；360 个 run ID 唯一；每单元五组平衡；同轨迹轮次有序 |
| 摘要 | 初始文件清单逐个匹配 SHA256；run/trajectory ID 绑定实验配置摘要 |
| 评分器夹具 | TP=7、FP=1、FN=3；Precision=0.875、Recall=0.7、F1=7/9，仅数学验证 |
| 准备门禁 | check-ready 按预期返回 1，列出 15 个未满足条件 |
| 依赖 | 独立单包 Workspace；无 Codex/Git/外部路径依赖，无 cc/cmake/SQLite C 引擎构建依赖 |
| 模型调用/真实 Agent 执行/消融运行 | 0 / 0 / 0 |

源码、配置和输入摘要见 [validation.json](validation.json)。本机 CLI 原始输出在 reports/local/（被 Git 忽略，含排程、score、bootstrap 和种子检查）。构建过程中一次 cargo 命令误在父仓库运行，因无 Cargo.toml 返回 101，已改到 btae-eval 目录，正确目录下所有最终检查通过；它不是被测 Agent 失败或研究结果。

## 不能据此得出的结论

不能证明 BTAE 优于 SDD、不能估计真实维护成本、不能宣称类级追溯改善、不能据 bootstrap 示例下研究显著性结论。没有结果被填入 reports/results-template.csv；仅保留 NOT_RUN 模板。

当前 Gold 是 Agent 起草的文件级影响清单；不独立、不含参考修复和隐藏验收，也不能代表类级语义真值。evaluator-private 在开发 checkout 内可见，目录名没有提供真实安全隔离。

所有种子规模 small，且架构简单、存在模板相似性。尚未覆盖 medium、真实仓库、长达20–30轮、多Agent或多语言泛化。

## 下一步的实际门槛

1. 验证 M1–M6 和 C/D 的实际工具流程，不把 prompt 文本当实现。
2. 增补中型/真实任务，由独立双人完成类级真值、隐藏测试和参考修复审核。
3. 建立隔离 DSH runner、可信成本/轨迹收集和计划配对审计。
4. 冻结模型版本或明确服务版本不可固定的降级设计，固定采样参数、公共 Profile、镜像、价格与人工单价。
5. 完成后再执行真实 Pilot；预实验只估计可行性/方差，正式优越性结论留给独立的预注册确认实验。

24条候选任务的当前预算上限是 2160 万输入+输出 token，若完全串行，每episode 1800秒的限额相加为180小时；这是上限规划，不是预期或实测消耗。实际货币成本在单价冻结前不填数字。

方法学参考已记录于 [SOURCES.md](../protocol/SOURCES.md)：使用 experimental-design skill 的随机化/聚类原则；其中 Scientific Agent Skills 的作者信息根据当日 arXiv HTML 读取。这不是 BTAE 效果来源。
