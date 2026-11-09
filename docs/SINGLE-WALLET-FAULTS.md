# 单钱包：离线故障验收

只使用 `/tmp` 独立私有目录、合成加密会话、127.0.0.1 HTTP/WS 对端；不读取真实钱包、不访问外部钱包/OKX/RPC、不切生产后台、不授权或广播真实交易。以下证明的是当前故障模型下的处理行为，不是对外部服务/真实链上结果的保证。

## 新发现并修复

1. **仅 orderId 被误当成 Task submitted 完成。** 新测试先复现 `BROADCASTED + orderId`、没有 txHash 时 Task 进入 COMPLETED。现在 Task 的 submitted 判据必须包含有效 32 字节 hex hash，订单号本身不能推动完成或后继步骤。
2. **未知准备可表面 resume 成功再暂停。** 原准备执行层已有 `preparation_started` 防重，不会真的重新授权，但 resume 控制层仍可能接受启动。现在 Pending 动作已有准备起始记录却无 ready proof，立即拒绝 resume，不调度、不查询或补授权。

OKX 独立 swap 也把缺失/无效 txHash 的 Execute 回复标成 `UNCERTAIN`，保留 `orderId`、`backendReportedStatus` 和 `retrySafe=false`。明确的 PENDING_CONFIRMATION 不被改写。只给有效 hash 写 swap_broadcast checkpoint；hash 仍仅是提交证据，不是成交成功。

## 覆盖矩阵

| 故障 | 本地注入方式 | 验收断言 |
| --- | --- | --- |
| Execute 回复丢失 | 模拟 HTTP 对端完整收到 Execute POST 后直接关连接，不发送回复 | route/Preview/Execute 各一次、结果 UNCERTAIN、Task PAUSED、重新加载后 arm/resume 拒绝、无辅助 HTTP |
| Execute 请求超时 | 对端完整收到 POST 后持有回复，不发数据，等待现有客户端超时 | 不修改生产 timeout；同样只一次 Execute、未知结果暂停，无刷新/重发 |
| orderId 无 hash | 返回 BROADCASTED/orderId，省略 hash | Task 第一阶段 uncertain，第二阶段 pending，后继没有准备或执行；独立 swap 也为 UNCERTAIN |
| 无效 hash | 返回短 hex txHash | 独立 swap 为 UNCERTAIN，原 operation ID 禁止复用；Task 各层嵌套 hash 判据也拒绝 |
| 授权回复丢失 | 模拟对端收到精确授权 Execute 后关连接 | 不进入事件监听、不发 swap；重新加载后立即拒绝 resume，不重复授权 |
| App 确认/后台拒绝 | 复用并扩展现有严格 controller 用例 | 暂停；新 Scheduler 读取原文件后仍不能沿旧授权重跑；监听后不补查询 |
| 真实进程中断 | 本地对端完整收到 contract-call Execute 后，对独立测试子进程调用 SIGKILL，回复始终未发送 | 已落盘 RUNNING/in_flight；新 OS 进程取得已释放的目录锁后读取为 INTERRUPTED/uncertain |
| 重启后只读对账 | 新进程通过本地 WSS 只读检查 chain ID，无 hash 可查询回执 | UNRESOLVED、writesSubmitted=0、PAUSED；对账前后 resume 和 arm 均拒绝；重启阶段任何钱包 HTTP 请求都失败测试 |

这不是仅仅丢弃 Tokio future 或重建内存对象：进程中断用例启动同一 Rust 测试二进制的独立 OS 子进程，确实终止它并启动另一个进程，读回原路径的磁盘文件。临时身份/session 的 bytes 前后完全一致，原 deadline 不变。父进程持有子进程清理 guard，断言失败也 kill/wait，避免留下测试进程。

其余 controller 故障用例在同一进程中使用新 Scheduler 模拟重载；不把它们冒充成真实进程重启。测试入口 helper 正常全量测试时无环境开关即返回，仅由父测试显式选中；子进程校验临时目录前缀和本地 HTTP origin。

## 边界

- 模拟对端收到 POST，不等于真实提供方已经签名/广播；正因无法确定，必须暂停。
- 没有新增生产重试、自动重连、订单号反查 hash、授权修复、期限续期或自动重新执行。
- 完成条件 submitted 只允许有效 hash 的提交证据，不保证成功 receipt；确认完成仍需共享 WSS 的规范回执校验。
- 没有真实币安风控/App 确认、真实断网、真实税币成交验收；不报告实盘延迟或成功率。

## 文件与复验

- `src/single_wallet_fault_tests.rs`：SIGKILL/新进程/对账/磁盘与 HTTP 防重断言。
- `src/task_prepare_runtime_tests.rs`：单钱包精确准备、live 事件、三核心 HTTP 与异常恢复矩阵。
- `src/okx_three_call_tests.rs`：独立 swap 的 order-only/无效 hash/持久 operation 防重。
- `src/task_validation_tests.rs`、`src/task_recovery_validation_tests.rs`：嵌套证据与准备续跑前置拒绝。
- 原多步骤成功 fixture 改为提供有效合成 hash，仍检验原来的 6 次执行及顺序，不恢复 orderId 完成捷径。

全量 `cargo test --locked --all-targets -- --test-threads=1`：359 passed / 0 failed / 1 manual benchmark ignored；其中一个正常返回的子进程 helper 是测试基础设施。Clippy -D warnings 通过。日志 `/tmp/baw-single-fault-tests.log`、`/tmp/baw-single-fault-clippy.log`，实际 Release 构建 `/tmp/baw-single-fault-release.log`。这些计数覆盖整个当前树，不代表 359 个新增故障场景。
