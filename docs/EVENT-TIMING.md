# 事件到执行：本地时序观测

本轮只改源码、做离线回归与构建；不切换真实后台、不新增授权/交易。此前约 0.95–1.01 秒实测没有本页字段，不能追溯推算事件到启动/上链延迟。

## 测量链路

actor 取到完整 WSS 文本消息 → 订阅消费者取到指定通知 → 本地筛选 → WSS 规范块读取/验证 → 接受触发 → 查计划/凭据/本地绑定、获取状态锁 → 持久准入 → worker 首次运行 → 钱包锁排队 → 钱包本地准备/必要的会话解密 → 进入 dispatch → 路由/Preview/Execute HTTP → 钱包动作返回 → controller 收集结果 → 既有结果保存。

所有阶段使用进程内单调 Instant，只持久化推导出的毫秒数；它们不参与授权、恢复、截止或重复执行决策。

起点是 **WSS actor 取到完整文本消息、JSON 解析之前**，不是网卡/内核收包、服务端发消息、区块产生/打包时刻。不包含通知到达本进程之前的网络/提供方延迟，不能称为完整链上事件到成交时延。

## 输出位置与阶段

- `results["triggerTiming:<step>"]`：步骤触发/准入阶段。
- `results["<step>:<wallet>"].triggerTiming`：该钱包完整本地执行时序；失败/未知结果也尽可能附带，未发生的阶段为 null。
- 原 `walletExecuteMs` 仍从 worker 内部开始到钱包动作返回，仍排除触发/准入和 worker 首次运行之前的等待。新计时的序列化不被混入该旧边界。

主要字段：

| 字段 | 边界 |
|---|---|
| triggerSource | live_log / backfill_log / live_head / initial_head / immediate / time / resume_without_original_ingress |
| notificationQueueMs | actor 取到文本消息 → 消费者取到匹配订阅通知；含 JSON 解析、发布、缓冲及调度，不是仅 channel 等待 |
| notificationToAcceptedMs | 同一起点 → 本地筛选及规范块验证通过 |
| candidateValidationMs | 开始处理选定候选 → 接受；日志包含规范块 RPC，不是最终性保证 |
| canonicalRead | queue_wait_ms、throttle_wait_ms、exchange_ms、total_ms、cache_hit |
| acceptedToAdmissionStartMs | 接受触发 → 即将执行既有准入 save；含本地计划/绑定、状态锁及 claim 准备 |
| admissionSaveMs | 既有准入 save 的完整调用，包含同步持久化；结束后才允许任何 worker 执行 |
| admissionToEnqueueMs / workerDispatchMs | 准入完成 → 入队 / 入队 → worker 首次运行 |
| workerToWalletQueueMs | worker 首次运行 → 开始等待钱包锁 |
| walletQueueWaitMs | 等待钱包锁 → 实际拿到锁；取消/超时未拿到则 null |
| unacquiredWalletQueueMs | 动作结束时仍未取得钱包锁的等待时间，不冒充成功拿锁 |
| walletPrepareMs | 拿到钱包锁 → dispatch 调用；涵盖目录/身份/会话检查、必要解密与 client 构建 |
| acceptedToDispatchMs / notificationToDispatchMs | 接受触发 / WSS 本地起点 → dispatch；dispatch 是动作入口，不是币安 Execute |
| notificationToActionReturnMs | WSS 本地起点 → 钱包动作返回，含动作内部日志/绑定处理，不代表广播或成交时刻 |
| workerResultCollectMs | 钱包动作返回 → controller 收集；包括之后的观测组装及等待，可能受其他钱包的同步保存影响 |
| resultStateLockWaitMs / resultSaveMs | 结果保存的状态锁等待 / 既有结果 save 调用 |

`canonicalRead.exchange_ms` 包括 actor 命令发送、调度、网络及响应解析；不能当作 Alchemy 服务端时间。锁排队和原 250 ms 请求限流等待独立列出；缓存命中不新增请求，exchange/throttle 为 0。

## 核心 HTTP 在共同起点上的位置

`triggerTiming.coreHttp` 分 `okxSwap`、`binancePreview`、`binanceExecute`，记录：

- `sinceAcceptedStartMs` / `sinceAcceptedFinishMs`
- `sinceNotificationStartMs` / `sinceNotificationFinishMs`
- `localAttemptMs`

开始点紧邻实际 send，结束为本地 HTTP 尝试返回或被 drop；错误/取消也可结束，**不是承诺获得响应、广播成功或上链**。未尝试的请求保留 null，不复用之前请求的时间。与原 `hotPathTiming.httpTransport` 结合，才可区分连接/HTTP 等待和前置本地等待。

## 回补、恢复和持久化边界

- 历史日志回补没有原始 WSS 通知起点，notification* 均 null；只记录本次候选规范性/准入及钱包阶段。
- 初始 head 已满足的区块触发同样不冒充 live header。
- 已准入触发的 resume 标记 resume_without_original_ingress，不能把恢复时刻当原事件接受时刻；notification* 和 acceptedTo* 不伪造。原截止/恢复规则不变。
- Instant 不进入 Recovery，不跨进程拼接时间。历史结果原有指标可保留，新继续动作不继承旧 live 时刻。
- 准入 save 无法把自己的完成时间放进同一次磁盘记录；保存前该字段 null，完成后在内存填充，并随后续既有结果/状态保存落盘。不增加仅为计时的 fsync。
- resultSaveMs 同理；崩溃时可能缺失，不因此推断该动作没执行，也不允许重发。

## 保留行为和离线验证

没有新增 RPC/HTTP、连接、重试、重连、provider 文本日志或后台轮询；保留单 WSS、原限流/缓存、规范块验证、绑定/截止/取消及耐久防重。计时不存 URL、原始日志或凭据。

100 ms 的 subscription.recv 超时不是固定事件延迟：消息到达或已在缓冲时立即唤醒，它仅让空闲时周期检查取消/到期；不会因此把消息攒满 100 ms 后批处理。

回环测试验证缓冲通知戳不被消费时刻覆盖、同 socket/读集缓存/原限流不变、回补源不造 live 时刻、原准入文件已含 triggered/InFlight 才允许 swap、冷/暖钱包锁的排队及 dispatch 次序、排队取消不造后续阶段、恢复无原始起点、任务作用域隔离、失败/未知暂停及未尝试 HTTP 为 null。人工合成偏移只测算术，绝不当实盘延迟报告。
