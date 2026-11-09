# 项目文档索引

计划按 planned、active、archive 管理；审查报告在所属独立项目 reports 目录保留，历史结果不覆盖。取消或被替代的方案归入 archive 并标明 retired，不等于实施完成。新增计划建议使用 YYYY-MM-DD-kebab-case.md。

| 日期       | 文档                                                                      | 状态                                                                           |
| ---------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| 2026-11-05 | [CI 验收规则](CI.md)                                                      | CI 仍未合格；父目录删除旁路已复现，初始化失败待定位，依赖例外待远端验收        |
| 2026-11-05 | [Windows 删除边界修复决策](active/2026-11-05-windows-token-boundary.md)   | in-progress；两条迁移路线待确定，安全修复未完成                                |
| 2026-11-02 | [BTAE-Eval](active/2026-11-02-btae-eval.md)                               | in-progress；正式实验未运行                                                    |
| 2026-11-03 | [下一阶段推荐进度](active/2026-11-03-next-iteration.md)                   | P1 方法忠实度优先；Pilot 继续阻断                                              |
| 2026-11-01 | [Superagent Forge 路线图](../superagent-forge/docs/ROADMAP.md)            | F0 基线完成，F1 起待实施                                                       |
| 2026-11-02 | [OMP old-coder 审查](../btae-eval/reports/OMP-OLD-CODER-REVIEW.md)        | 历史审查，边界已加固                                                           |
| 2026-11-02 | [DSH headless live collector](../btae-eval/reports/DSH-LIVE-COLLECTOR.md) | 73 测试、真实工程 smoke；正式 Pilot 未运行                                     |
| 2026-11-03 | [DSH 工具部署恢复](../btae-eval/reports/DSH-TOOLS-RECOVERY.md)            | 去重后工具/隔离任务通过，69 条公开契约断言通过；超时保留，正式预算门禁仍待完成 |
| 2026-11-03 | [DSH commandcode 路由](../btae-eval/reports/DSH-COMMANDCODE.md)           | 历史路由切换验收；工具失败已在后续部署恢复中解决                               |
| 2026-11-03 | [DSH isolated task collector](../btae-eval/reports/DSH-TASK-COLLECTOR.md) | 85 测试、首轮隔离收集；凭据/工具集成已后续复测，正式 Pilot 未运行              |
| 2026-11-02 | [DSH 边界回归加固](../btae-eval/reports/OMP-REVIEW-GAP-CLOSURE.md)        | 58 测试、4/4 定向变异通过；独立复核限制已披露                                  |
| 2026-11-01 | [旧 Codex Fork 方案](archive/plans/2026-11-01-codex-core-minimal-fork.md) | retired，被独立 Pure Rust 方向替代                                             |

上游 Codex 的接口与使用文档保持原路径；没有将普通参考手册当作计划批量移动。真实原始 Agent 轨迹、临时源码副本、编译产物和私有评估真值留在本机，不属于发布源码。
