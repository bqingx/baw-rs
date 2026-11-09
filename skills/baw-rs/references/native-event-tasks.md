# 旧原生任务迁移

旧钱包级任务、日志/pending 监听器与跨钱包复制引擎已删除。不要使用 `--wallet NAME task ...` 或旧 source/filter/action 配置。

`task` 现在只是实例级 `pipeline` 的别名；一个 task 在 actions 中指定多个钱包，配置与恢复见 [流水线操作](pipelines.md)。新版本仅支持时间、区块和已出块日志，不包含 pending 交易监听。

旧状态不能自动迁移、重启或重放；确认原交易状态和新意图后再创建、审核并授权新流水线。不要删除已有会话或覆盖 identity 来“修复”迁移。
