# 旧 typed resident 执行器已移除

`src/executor.rs` 及其 prepare/trigger/disarm 协议、独立预约状态和专用客户端/基准示例已删除。旧 native 控制帧也不再接受。未知/旧帧明确返回错误，不能自动转换为立即交易。

目前只保留：

- 普通钱包命令的 resident client / 会话复用与每钱包队列。
- 实例级 pipeline/task 的统一调度、持久防重和恢复。
- 统一的 `contract-call fire` / 分步 preview-execute 签名核心。

提前准备与触发见 [PIPELINES.md](PIPELINES.md)，当前本地基准边界见 [PERFORMANCE.md](PERFORMANCE.md)。旧 benchmark 数字不适用于新架构，也不代表生产延迟下限。

移除旧执行器不会修改钱包身份文件或现有登录态；部署与真实交易仍需要另行授权。
