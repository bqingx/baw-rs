# 旧 typed executor 已移除

旧 prepare/trigger/disarm/native IPC 与专用客户端已删除；不能使用旧帧执行或预约钱包。

提前准备与自动执行统一走 [实例流水线](pipelines.md)，普通单次已授权调用使用 `contract-call fire`。保留每钱包 client/会话复用，不再维护第二套 typed 执行状态。

历史 benchmark 不适用于当前路径；只引用当前选定二进制和明确口径的测试结果，不承诺生产延迟。
