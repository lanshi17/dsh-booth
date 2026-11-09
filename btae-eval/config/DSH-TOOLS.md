# DSH 0.2.1-rc.3 工具调度器部署检查

本机曾出现 `Cannot read properties of undefined (reading 'prepare')`。DSH 的顶层、`dsh-base` 与 `dsh-sdk-minimal` 各解析到一份 `dsh-tools`；其 28 个自身文件完全相同，但每个模块分别创建 `Symbol("@deepseek-ai/dsh-tools.scheduler")`。Agent Loop 用另一份 Symbol 访问 ToolRuntime 时取到 undefined。相同版本和文件摘要并不能保证模块实例共享。

本轮仅修复外部 DSH 安装的模块解析布局，不修改 Rust 实现、DSH 源码、模型路由或沙箱策略。适用范围为已验证的 Node 24.16.0 / DSH 0.2.1-rc.3 布局，其他版本或使用 `--preserve-symlinks` 的部署需重新验证。

## 检查模块身份

将下面代码保存为私有临时文件 `dsh-tools-identity.mjs`，用实际 Node 执行：`node dsh-tools-identity.mjs "$BTAE_DSH_BUNDLE"`。BTAE_DSH_BUNDLE 必须是完整 `node_modules` 目录的绝对路径。它只导入安装包并比较调度器 Symbol，不创建 Agent 或调用模型；缺失包、缺失导出或身份不一致均退出非零。

```js
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
const root = resolve(process.argv[2]);
const owners = ["@deepseek-ai/dsh", "@deepseek-ai/dsh-base/node_modules/@deepseek-ai/dsh-agent-loop", "@deepseek-ai/dsh-sdk-minimal/node_modules/@deepseek-ai/dsh-agent-loop"];
let canonical;
for (const owner of owners) {
  const require = createRequire(resolve(root, owner, "package.json"));
  const filename = require.resolve("@deepseek-ai/dsh-tools");
  const symbol = (await import(pathToFileURL(filename))).TOOL_RUNTIME_SCHEDULER;
  if (typeof symbol !== "symbol") throw new Error("missing scheduler symbol");
  canonical ??= symbol;
  if (symbol !== canonical) throw new Error("duplicate scheduler module identity");
}
console.log("DSH scheduler identity: shared by all three checked consumers");
```

这只检查列出的三个消费者；通过不等于所有插件均兼容。原始安装的负例退出 1，去重部署及修复后的本机安装均退出 0。

## 修复与回滚

先在独立部署副本中验证，再修改实际安装。不要在含应用数据的目录中执行批量去重，也不要更改 npm 包的 JavaScript 源码。对于本轮布局：

1. 确认顶层 `@deepseek-ai/dsh-tools` 和下面两处的 package.json 及包自身文件完全一致（排除各自 node_modules）。本轮三份版本均为 0.2.1-rc.3，28 个自身文件逐个 SHA256 一致。
2. 将 `@deepseek-ai/dsh-base/node_modules/@deepseek-ai/dsh-tools` 和 `@deepseek-ai/dsh-sdk-minimal/node_modules/@deepseek-ai/dsh-tools` 两个目录移到 node_modules **之外**的私有备份位置，不删除它们。
3. 在各自原位置创建相对目录符号链接，目标均为 `../../../dsh-tools`。确认 realpath 均等于顶层包，再运行上面的模块身份检查。
4. 通过[当前模型配置](README.md)运行真实工具探测，再运行隔离 collect-task。身份检查通过不能替代真实工具验收。

回滚时先停止使用该部署启动新任务，核对两处仍为本轮创建的符号链接，仅移除这两条链接，再将对应备份目录移回原位。本机备份在 `$DSH_HOME/backups/20261103-dsh-tool-identity/`，分别以 dsh-base、dsh-sdk-minimal 命名。升级或重新安装可能重新产生重复包，需重新检查；此修复没有修改包管理器锁文件。

## 独立工程契约检查

[本轮报告](../reports/DSH-TOOLS-RECOVERY.md)中的 69 条断言来自下面的公开诊断代码，未读取 Worker 编写的 checks.py。它检查 queue-r01 的公开需求，不是隐藏测试或研究 Ground Truth。

把代码保存为私有 `queue-priority-contract.py`，将 BTAE_SNAPSHOT 设为 collect-task 的 `run/workspace-snapshot`，在只读、无网络命名空间内执行：

```sh
bwrap --unshare-all --die-with-parent \
  --ro-bind /usr /usr --ro-bind /lib /lib --ro-bind /lib64 /lib64 \
  --proc /proc --dev /dev --tmpfs /tmp \
  --ro-bind "$BTAE_SNAPSHOT" /workspace --chdir /workspace \
  -- /usr/bin/python3 -B - < queue-priority-contract.py
```

```python
from domain import Job
from service import QueueService
from store import JobStore
import api

class RecordingStore(JobStore):
    def __init__(self):
        super().__init__()
        self.writes = 0
    def add(self, job):
        self.writes += 1
        super().add(job)

checks = 0
def check(condition):
    global checks
    assert condition
    checks += 1

check(Job("a", "x").priority == 0)
for priority in range(10):
    check(Job("a", "x", priority).priority == priority)
    store = RecordingStore()
    service = QueueService(store)
    check(service.submit("a", "x", priority).priority == priority)
    check(store.get("a").priority == priority)
    check(api.submit(QueueService(RecordingStore()), {"key":"a", "payload":"x", "priority":priority}) == {"key":"a", "payload":"x", "priority":priority})
check(api.submit(QueueService(RecordingStore()), {"key":"a", "payload":"x"}) == {"key":"a", "payload":"x"})
invalid = [True, False, -1, 10, 1.0, "1", None, [], {}]
for value in invalid:
    for target in ["domain", "service", "api"]:
        store = RecordingStore()
        service = QueueService(store)
        try:
            if target == "domain": Job("a", "x", value)
            elif target == "service": service.submit("a", "x", value)
            else: api.submit(service, {"key":"a", "payload":"x", "priority":value})
        except (TypeError, ValueError): pass
        else: raise AssertionError("invalid priority accepted")
        check(store.writes == 0)
print("independent public engineering contract:", checks, "assertions passed")
```

同一检查对未实现优先级的原始 queue seed 应退出非零，本轮生成快照退出 0。快照、Agent 轨迹和 session 均保留私有，不作为发布源码。上述 Python 代码只用于 Python 样例的公开工程检查；正式评估工具仍为独立 Rust Workspace。
