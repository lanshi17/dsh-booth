# BTAE-DSH-ADAPTER-01 R1 — 修正后重新验收

继续当前隔离副本任务。仅允许修改 src/dsh.rs、tests/dsh_adapter.rs、protocol/DSH-ADAPTER.md、DSH-DELIVERY.md；src/lib.rs只保留pub mod dsh最小差异。其他原任务约束全部继续有效：不访问/修改主项目、父目录日志、凭据，不提交推送，不启动其他Agent或模型，不修改manifest/runner/既有测试/readiness。读当前副本中的TASK.md可了解原任务。本轮需求覆盖原任务冲突项。

首轮宿主全套44项测试通过，但独立验收发现以下必须修复问题。原交付与失败证据已保存在父控归档；不要访问归档。不要仅修测试。

## 1. 更正主控原任务的缓存假设
原任务“cache必须是raw input的子集”不适用于真实DSH，现明确撤销。已从安装的DSH实现核实：inputTokens是uncached input；cacheReadTokens、cacheWriteTokens是额外且互不重叠的prompt bucket；reasoningTokens是output子集，不可再相加。步骤usage是该步骤所有attempt的累计增量，不能再把turn级重复相加。

新增只读 tests/fixtures/dsh-live-development-sanitized.jsonl 是首轮真实141个事件的脱敏结构，保留顺序及全部数值，替换text/tool内容。必须成功解析它，steps=23，原始inputTokens合计92414，outputTokens=45968，cacheReadTokens=1779072，totalTokens=1917454，规范化全部input=1871486。
真实样例：inputTokens=25193,outputTokens=327,totalTokens=36272,cacheReadTokens=10752。

规范化要求：保留raw uncached/cache-read/cache-write/reasoning字段各自的已观察小计和覆盖范围；summary.usage.input_tokens指所有input（含缓存）；input+output=total，无double counting。若有totalTokens与output，则可以total-output得到精确canonical input，并验证它不少于已知prompt buckets；两个cache字段都存在时必须相等；reasoning<=output。有total= input+output且cache省略时，canonical input/total可以完整，但省略cache明细仍未知，不能直接假设为0。无total且cache bucket省略时，canonical input/total不保证完整，必须None/partial。明确标识此CLI schema版本/语义，拒绝未知/歧义schema；未经验证的cachedInputTokens等旧别名应拒绝或明确不同schema，不要当作cacheReadTokens无条件相加。

## 2. 缺失usage与截断不能发布虚假全量
step1有完整usage，step2无usage：usage_status Partial；全程input/output/total均None，而observed subtotals可单独报告。不同字段覆盖独立，不能第1步有cache就把该cache小计标成全程cache总量。
step1完整，step2已开始但EOF未step_end：UsageStatus不能Complete，全程总量None；仍可保留已知小计。无usage时Unknown，不是0。
原始字段小计不能标成canonical full totals。每字段coverage需包含所有started steps，unfinished也计入未知。

## 3. 解析失败必须终态
feed收到坏JSON/非法UTF8/超限/协议或usage错误后，任何后续feed和finish必须返回错误。不能调用方忽略error再喂入suffix从而得到completed=true。不要retain raw payload进错误信息。

## 4. 重复JSON键必须拒绝
JSON解析不能先变Value导致重复key被静默last-wins。拒绝对象重复key，至少包括顶层/status/usage，推荐递归检测。案例 usage:{"inputTokens":99,"inputTokens":1,"outputTokens":1}。蛇形和驼峰别名同时存在也不能绕过。保持深度/字节/事件限制，错误不应泄露text/tool payload。

## 5. 其他
sessionId至少应非空有效；文档与CRLF行长限制实际行为一致。保持离线、有界内存、无新增依赖、无外部执行、无Observation/审批/评测通过转换。

## 回归验收
保留真实smoke测试并新增真实cached fixture测试、cacheWrite/reasoning测试、字段覆盖/缺usage/未结束step测试、sticky失败测试（含超限）、重复键/别名冲突测试、缓存及总量矛盾/溢出测试。主控会在宿主重新执行既有与独立测试。
执行fmt、adapter测试、clippy；你所在沙箱不允许nested userns，原runner测试会因Bubblewrap权限失败，这不是让你改runner的理由。记录实际退出码及边界后结束。不要宣称正式Pilot或账单已验证。
