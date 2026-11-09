# 可选任务结果通知

本轮实现只做代码/合成会话及回环 HTTP 验收，未部署、未读取通知 token、未向真实渠道推送。当前服务和已有钱包数据不变。

## 范围

通知实例 Task/流水线停止后的汇总，不逐请求/逐钱包刷屏：

- 全部链上确认完成，与显式 submitted 阶段完成分开；后者明确不代表链上成功。
- 等待 App 确认、执行结果未知、失败、原有效期已过、其他暂停分别提示；多钱包混合未知/待确认/失败时明确报告混合结果，不用单个 App 待确认掩盖其他未知动作。
- 取消提示已提交交易不可撤回；活跃请求先 drain，再发停止汇总。
- 显式只读 reconcile 得到新的状态可发汇总；普通 status 和启动时读取旧归档不发通知，不扫描补发历史消息。

只在原状态成功落盘后发布通知。通知事件不写入原 Task 授权、截止或签名上下文；不修改重试/恢复策略。PERSISTENCE_ERROR 不发送看似成功的通知。

## 默认关闭，渠道必须显式配置

只有启动 daemon 时设置 `BAW_RS_NOTIFY_CONFIG` 才启用，使用绝对路径；该 JSON 文件须本人属主、0600、普通文件、单硬链接且非符号链接，父目录须 0700。文件放在仓库外，禁止提交 token。

配置结构（以下 token 只是占位符，不能直接使用）：

```json
{
  "token": "REPLACE_WITH_PRIVATE_PUSHPLUS_MESSAGE_TOKEN",
  "channel": "app"
}
```

`channel` 必须明确为 `wechat` 或 `app`，没有隐式渠道默认。本地默认偏好是 PushPlus APP（显式 app），微信仍为可选；电话只使用 fwalert，本功能未接自动拨号、也不接 fwalert APP。当前只实现 PushPlus；不擅自选择用户其他场景的 Telegram 群组。endpoint 固定 `https://www.pushplus.plus/send`，不允许从配置覆写 URL，禁止重定向，禁用 HTTP 重试。token 只发送到通知服务，不写入日志或投递记录。

示意启动方式，本文不执行此命令：

```sh
BAW_RS_NOTIFY_CONFIG=/absolute/private/notifications/pushplus.json \
  /absolute/verified/baw-rs --registry /absolute/registry wallets serve
```

配置只在启动时读取，不在等待触发或执行时刷新。缺失时完全关闭，无额外目录、网络请求；配置不合法时静默禁用通知，不阻止钱包服务启动。`wallets status` 的 notifications.enabled 可确认是否启用，同时给出非持久化 droppedQueueEvents 计数；不显示 token、配置路径或 endpoint。测试默认忽略宿主通知环境，只有显式传入合成配置和 127.0.0.1 对端才能测试发送。

## 隔离与敏感信息

终态保存后，只向容量 32 的队列 try_send，不等待网络。独立 worker 使用独立 HTTP 客户端；连接上限时间 2 秒、整次请求上限 5 秒、回复读取上限 4096 bytes。队列满只做原子计数并丢弃通知，不同步写 stderr（避免管道满导致阻塞或写失败 panic），不阻塞钱包、不改任务结果。停止 daemon 后在资金请求 drain 完成基础上，最多额外等 2 秒尝试清空通知；不是送达保证。

发送内容只从 typed allowlist 构造：安全的 Task ID、钱包槽位名、固定中文状态及 confirmed/submitted/unresolved/failed/pending 计数。不转发原 argv、配置路径、RPC/HTTP URL、calldata、报价/模拟内容、客户端身份、会话、任意 backend error/response。接收方因此可以知道任务 ID 和钱包名字，不应把这些名字当作凭据。跨步骤大任务不因超过 128 个不同钱包而静默禁用；支持模型最多 4096 个槽位，正文最多展示前 16 个不同钱包并注明省略数量，去重仍使用完整名单/状态。

财务热路径仍是原来的 OKX swap → Preview → Execute。启用通知后会额外产生**终态保存之后的通知 HTTP**，不计入这三个财务核心请求，也不冒充零网络开销；没有把通知混入触发或签名请求范围。

## 去重与故障语义

通知是 **best effort / at-most-once attempt**，不是 exactly-once 送达：

1. 固定事件（Task ID、静态状态、完成类型、钱包/各动作 outcome 与计数）生成 hash，不把当前时间放进去。
2. worker 在 registry 的 `.task-notifications/`（0700）创建 `<hash>.json`（0600）并同步 `CLAIMED`，然后才请求通知服务。
3. 成功受理记录 `ACCEPTED`；失败或未知记录 `FAILED_OR_UNKNOWN`。不保存 token、消息全文或服务端正文。
4. 相同事件即使进程重启、上次失败/超时/只剩 CLAIMED，也不重发。新的状态/动作进度有不同事件，可产生新汇总。

PushPlus HTTP 成功且 code=200 只表示服务端受理，**不是手机送达证明**。断网、队列满、磁盘失败、进程在终态保存与发送之间退出，都可能丢通知。为避免重复提醒，不自动重试、不在启动时扫旧 Task 补发；财务结果始终以本地 Task/operation 及规范链上回执为准。需要手动核验的是任务状态，不要因未收到通知而重新提交交易。

## 离线验证

- 配置权限、显式渠道、未知字段拒绝及符号链接拒绝；外部 URL 不可作为测试入口。
- provider 503/业务错误、超大响应、禁止重定向及 token 边界。
- 内存重复、重新构造 notifier 后重复、只有 CLAIMED 的中断记录均只尝试一次。
- 队列满不 await/不写 stderr，只累积丢弃计数；默认关闭不创建通知目录。144 个不同钱包汇总可投递且截断展示，不丢弃整个事件。
- Task 回环用例中通知服务故意返回失败：原提交完成/等待 App/未知状态保持正确，精确授权、route/Preview/Execute 计数、防重/恢复与原截止不变。
- 状态分类测试证明 raw error/URL/calldata 不进入事件，并区分 submitted 与 confirmed、不把运行中/持久化失败报成完成。混合 App+未知或 App+失败必须报告 mixed。

全量当前树：375 项 Rust 通过（0 失败、1 手工基准忽略），Clippy -D warnings 通过；日志 `/tmp/baw-hotadd-notify-tests.log`、`/tmp/baw-hotadd-notify-clippy.log`，实际 Release 构建 `/tmp/baw-hotadd-notify-release.log`。计数不是新增测试数量。

相关文件：`src/task_notifications.rs`、`src/task_notifications_tests.rs`、`src/task_runtime.rs`、`src/task_recovery.rs`、`src/task_prepare_runtime_tests.rs`。
