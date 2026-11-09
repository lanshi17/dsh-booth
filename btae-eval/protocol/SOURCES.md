# 来源与使用边界

方法问题、六项机制、五组比较和长期演进目标来自用户提供的 BTAE 研究方案。本轮没有将用户方案中的示意百分比、成本和 Pareto 图作为实测数据。

2026-11-02 读取 https://www.swebench.com/ ：官网列 Verified 500 instances、human-filtered subset；它衡量真实 issue 修复，不能单独证明某工程方法长期优越。

尝试读取 https://github.com/github/spec-kit 超时；正式基线尚未冻结源码 commit/模板。因此当前明确称 Spec Kit 风格流程，不声称已经审计或复现官方版本。

Scale Labs 的 SWE-bench Pro 页面因 robots/403 无法读取；未核验用户所述 Pro V2 的 2026-10 发布时间或规模，本包不将这些信息作为已确认事实。外部基准正式加入时需独立核验版本、许可和任务可得性。

实验随机化、阻断与伪重复审查采用本机 experimental-design skill 的原则，未运行其 Python DOE 代码；实现为 Rust。参考：Agarwal, V., He, Y., Patel, D., & Brueckner, A. M. (2026). *Scientific Agent Skills: A Library of Procedural Knowledge for Research Agents*. https://doi.org/10.48550/arXiv.2609.00065 。作者与日期根据当日 https://arxiv.org/html/2609.00065 页面读取；论文报告其技能库文档特性，不是 BTAE 有效性的证据。
