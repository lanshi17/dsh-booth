# DSH 模型路由切换：commandcode

2026-11-03 按用户要求切换为 `/model commandcode/deepseek/deepseek-v4.1-flash`。headless 不接受交互式 `/model`，因此配置等价的 provider `commandcode`、model `deepseek/deepseek-v4.1-flash`。本机 headless 默认及 Pilot 草案五组的共同模型均已更新；历史 ark 结果不改写。新 manifest 生成新的 run/trajectory ID，不能和旧 manifest 收据混用。

真实单轮 DSH smoke 成功：退出 0、5,822 ms、stdout 624 字节、stderr 0；self-reported input 5,690、output 39、total 5,729 tokens。billing_verified=false，模型不可变 revision 未验证。单个已选凭据仅通过私有环境传入，未进入 argv 或公开报告。

真实 collect-task 返回 protocol_error，失败收据经 verify-task 离线校验通过；另一次最小工具探测确认 DSH Worker 退出 1，turn_end 为 error，固定错误为 `Cannot read properties of undefined (reading 'prepare')`。因此连通性已恢复，工具任务仍未通过；没有将 runtime 错误或 Worker 自报转化为实验成功。

已有 85 项 Rust 测试通过，readiness 按预期退出 1。无 Rust 实现、依赖版本、预算和 readiness 声明变更；仅模型配置、共同研究输入和说明更新。本轮未新增独立审查，不继承历史审查 verdict；正式 Pilot 未运行。

[配置与使用方法](../config/README.md) · [机器摘要](DSH-COMMANDCODE.json)。连通性与工具探测使用专用 profile、私有状态和 bwrap，显式共享网络。原始 Agent 输出、session、凭据及源码产物保留在本机，不发布。

后续更新：`prepare` 异常已定位为重复包的模块 Symbol 身份不一致；部署去重后工具探测、真实隔离任务和独立公开契约检查通过。见[恢复验收](DSH-TOOLS-RECOVERY.md)。上文保留切换路由时的失败记录。
