# 旧钱包级原生任务已移除

旧 `native_events` / `native_pending` / `native_feeds` / `native_block_time` 引擎和跨钱包复制控制器已从源码删除，不再存在 legacy 单钱包任务入口。

- 旧 `--wallet NAME task ...` 拒绝。
- `task` 现在只是实例级 `pipeline` 的别名，不接受钱包选择器。
- 旧配置中的 source/filter/action 等字段不能按新 schema 自动解释；须明确重建为 steps/actions 定义，重新审核后启用。
- 新系统支持时间、区块和已出块日志；旧 pending 交易监听没有隐式恢复或回退。
- 不读取、删除或自动迁移旧任务状态文件，不重发旧动作。

当前唯一任务执行路径见 [PIPELINES.md](PIPELINES.md)，示例见 `examples/pipeline-contract.json`。历史实现/测试可从 Git 历史查看，不能作为当前构建的能力或实盘验收依据。
