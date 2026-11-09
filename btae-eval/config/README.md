# DSH commandcode 模型配置

路由：`commandcode/deepseek/deepseek-v4.1-flash`。provider 是 `commandcode`，model 是 `deepseek/deepseek-v4.1-flash`。[公开补丁](dsh-commandcode.patch.yml)只引用凭据环境变量名 `COMMAND_CODE_API_KEY`，不包含值。该 endpoint/model 元数据与本机已用的 OMP 模型目录一致，并经真实 DSH 单轮 smoke 验证。

在 btae-eval 目录中给 DSH 应用补丁：

```sh
dsh --profile headless --patch "$PWD/config/dsh-commandcode.patch.yml" \
  "Reply exactly READY. Do not use tools."
```

先通过自己的凭据管理方式提供 `COMMAND_CODE_API_KEY`，或在 DSH credentials 服务保存同名 ref；不要将值写入命令、版本库或补丁。headless 的参数中没有 `/model` 或 `--model`，该补丁设置其默认模型。补丁不会安装 DSH 或改变 Rust 依赖。直接 DSH 命令使用调用方的当前环境和工作目录，不是 Rust task collector 的隔离入口。

为 `collect-task` 准备专用 Profile：

```sh
BTAE_PROFILE_DIR=$(mktemp -d /tmp/btae-profile.XXXXXX)
printf '[]\n' > "$BTAE_PROFILE_DIR/cordis.yml"
cp config/dsh-commandcode.patch.yml "$BTAE_PROFILE_DIR/cordis.patch.yml"
```

在已配置好的 Node/DSH 部署上，`--executable` 选择 Node 的绝对路径，`--bundle` 指向完整 node_modules 目录，容器内入口为 `/app/node_modules/@deepseek-ai/dsh/lib/bin.js`。在当前 shell 已安全导出 `COMMAND_CODE_API_KEY` 后调用（按实际安装位置设置 BTAE_DSH_BUNDLE）：

```sh
BTAE_TASK_DIR=$(mktemp -d /tmp/btae-commandcode.XXXXXX)
./target/debug/btae-eval collect-task \
  --manifest datasets/pilot.json --task queue-r01 --arm A --repeat 1 \
  --executable "$(command -v node)" --bundle "$BTAE_DSH_BUNDLE" \
  --profile "$BTAE_PROFILE_DIR" --pass-env COMMAND_CODE_API_KEY \
  --arg=/app/node_modules/@deepseek-ai/dsh/lib/bin.js \
  --arg=--profile --arg=headless --arg=--json --arg=- \
  --share-network --wall-ms 90000 --output "$BTAE_TASK_DIR/run"
./target/debug/btae-eval verify-task "$BTAE_TASK_DIR/run" datasets/pilot.json
```

`--share-network` 显式允许工程网络请求，没有 endpoint allowlist；所有输出目录必须新建。进程收据可能失败但仍能离线校验，校验通过不表示任务通过。当前单轮连通性和工具任务工程验收均已通过：重复 dsh-tools 包造成的 `prepare` 错误已通过部署去重修复，见[部署检查与回滚](DSH-TOOLS.md)和[恢复验收](../reports/DSH-TOOLS-RECOVERY.md)。上面的 90 秒只是调用示例；本轮真实任务在 90 秒超时，另一次 180 秒上限的工程复测在约 103 秒完成，token 用量超过 Pilot 草案预算。Profile/state/session 和源码快照可能含敏感任务信息，不应整体发布。
