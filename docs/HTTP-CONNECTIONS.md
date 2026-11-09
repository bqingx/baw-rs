# HTTP 连接复用观测

观测版在用户新授权窗口已部署并完成一次真实事件兑换，实际连接数据见末节；此前独立只读探测也保留，它不是 Preview/Execute 的替代测量。此前真实 949.95 ms 样本来自 SHA256 `71801ce9…`，没有本页的新字段，不能追溯判断当时是否重建了连接。

## 三个位置

新结果的 `hotPathTiming.httpTransport` 包含：

- `okxSwap`：OKX swap 的连接观测。
- `binancePreview.connection`：Preview HTTP 连接观测。
- `binanceExecute.connection`：Execute HTTP 连接观测。

Preview/Execute 的外层同时包含 `endpoint`、`request_ms`、`body_ms`、`full_roundtrip_ms`。endpoint 只输出固定类别，不输出 URL；实际 POST 未发送时不将其他请求冒充该 POST。缓存返回/早期拒绝不沿用旧观测。

## 连接字段

| 字段 | 含义 |
|---|---|
| pool_id | 进程内不透明连接池编号；不同钱包/OKX/币安分池，clone 共享编号 |
| socket_id | 本池观测的本地/远端 socket 元组对应的不透明编号；不输出地址 |
| socket_seen_before | 该元组是否在有界历史中出现过 |
| http_protocol | 响应实际 HTTP 协议版本 |
| connect_attempts / successes / failures / cancellations | begin/finish 区间中该池的连接活动计数 |
| connector_elapsed_ms | 区间内已结束连接尝试的合计时间，含失败/取消 |
| observation_ms | begin 到响应头或请求错误的本地观测区间 |
| activity_scope | 固定 pool_interval_not_request_attributed，提醒并发归因限制 |
| reuse_evidence | 下述固定证据标签 |

标签：

- `same_socket_no_connect_attempts`：观测到相同元组，且此区间没有新的连接尝试。对串行请求是很强的复用证据。
- `same_socket_with_connect_activity`：相同元组，但池有连接活动；不能把并发的其他连接误归给本请求。
- `first_socket_observation`：首次见到该元组，不等于断言本请求刚创建它。
- `unavailable`：无响应或库未提供 socket 元数据；不能默认当成复用。

socket 历史最多 64 项，淘汰后编号不复用。操作系统可能回收相同 socket 元组，编号不是物理连接的永久身份证；进程重启也不保持编号。

## 时间边界与不能做的推断

- connector 层观测合计 **DNS＋TCP＋TLS**，不是三者各自时间。没有改 DNS resolver、TLS 或请求语义以伪造分段。
- 未暴露单独 DNS/TLS/服务端处理时间，不能用 0 填充冒充测量。
- shared-pool 时间区间可与其他请求重叠：某次连接可能在 begin 前开始、在 finish 前结束，多个耗时可累加超过单请求时间。
- 因此不能以 request_ms 减 connector_elapsed_ms 计算“服务端处理时间”。即使没有建连，该时间仍包含网络往返、发送、排队和响应等待。
- request_ms 是本地 send 到响应头；body_ms 是响应头后读取/解码或提前拒绝，不保证失败路径已完整 drain body；full_roundtrip_ms 不是完整签名流程时间。
- routeFetchMs/previewCallMs/executeCallMs 是更外层的操作时间，可能包括参数处理、本地绑定/持久化等，不应与连接计时混为一谈。

## 已完成的只读探测

`examples/http_readonly_probe.rs` 在独立进程加载既有加密会话，使用实际 Client 和 OKX quote 代码，同一对象各发 4 次固定 GET：首次、立即重复、空闲 120 秒后、再次立即重复。不接收任意命令/URL，不启动后台/Task，不调用 swap、approve、Preview 或 Execute。

| 阶段 | 币安 settings 到响应头 | 币安连接 | OKX quote 到响应头 | OKX 连接 |
|---|---:|---|---:|---|
| 首次 | 149.2 ms | 建立，合计 12.4 ms | 92.3 ms | 建立，合计 12.6 ms |
| 立即重复 | 157.5 ms | 同 socket，无建连尝试 | 88.6 ms | 同 socket，无建连尝试 |
| 空闲 120 秒 | 162.9 ms | 重建，合计 4.8 ms | 81.1 ms | 仍为原 socket |
| 再次立即重复 | 122.8 ms | 新 socket 再复用 | 81.4 ms | 仍为原 socket |

均为 HTTP/1.1。币安 builder 未指定空闲时间，reqwest 当前默认 90 秒；120 秒后重建符合此上限，但不因此断言远端是否也会关连接。OKX 本地空闲上限 600 秒，本次观察到 120 秒后仍复用，不保证更长等待始终成立。

身份与加密会话文件的前后 SHA256 一致，8 次只读请求、0 钱包写入、0 Task。私有报告 `/root/wallets/tasks/http-readonly-1793942500821.jsonl`。新增诊断入口/报价元数据经过 2 项客户端和 4 项相关报价回归、all-target Clippy、fmt/diff 检查；后续部署前也已对包含这些新改动的源码完整重跑 319 项 Rust 测试，0 失败、1 手工基准忽略（/tmp/baw-observer-live-gate-tests.log）。

结论：实际同域复用确实工作；本次建连合计只有约 5–13 ms，不足以说明此前两次约 400 ms 的签名接口耗时。不能由只读接口的数字推导交易端提速或服务端处理时间，也没有因而增加后台保活请求。

## 已完成的实际核心调用观测

用户明确授权后，`027b996b…` 产物启动测试后台 PID 39475（进程映射 SHA256 已核实）。Task `bsc-usdt-observe-1793943463427` 已 COMPLETED/confirmed。

| 核心调用 | 外层调用时间 | HTTP 到响应头 | 连接观测 |
|---|---:|---:|---|
| OKX swap | 86.07 ms | 85.98 ms | pool 3 / socket 4，已见过，0 新连接尝试 |
| 币安 Preview | 433.86 ms | 427.96 ms | pool 1 / socket 2，已见过，0 新连接尝试 |
| 币安 Execute | 479.59 ms | 476.05 ms | pool 1 / socket 2，已见过，0 新连接尝试 |

均 HTTP/1.1，三个调用均为 same_socket_no_connect_attempts，connector_elapsed_ms 均为 0。Preview/Execute 明确使用同一观测 socket；不是“代码有池所以假定复用”。Preview/Execute body 读取约 0.068/0.020 ms，本地工作与 body 解码不是此次主要耗时。不能把剩余等待都归为服务端计算，网络/排队等仍不可独立分离。

钱包阶段 1013.15 ms，不含事件规范性验证与链上确认；不将其波动与此前 949.95 ms 比较为稳定退化或提速保证。触发期币安请求 2、policyReads/cacheHits 0；监听前精确授权，实际 USDT 扣款 1e18，两笔回执 status 0x1，用后 allowance 0。

私有链上核验 `/root/wallets/tasks/bsc-usdt-observe-1793943463427-verification.json`。本次授权只覆盖该单次任务，不续期、不盲目重发。结论是：本样本没有触发期重建 TCP/TLS，不能靠再次调整连接池消除约 0.9 秒的两次 HTTP 等待。

## 行为与隐私

在原有两个池上包 connector observer，不增加 client/连接/请求，不启用预热轮询、自动重试、代理、重定向、verbose 或原始网络日志。池原有配置不变，不能保证远端不主动关连接。

不输出 URL、域名、原始 socket 地址、Cookie、Client ID、API Key、签名、证书或原始连接错误。连接超时、失败与取消仍只尝试一次；计数不是重试机制。

离线回环覆盖 keep-alive、强制关闭/重建、并发归因、clone/分池隔离、失败/取消、历史上限和隐私；完整 controller 仍断言三次核心 HTTP，且 Preview/Execute 的 pool_id 相同、OKX 不同。实际 Preview/Execute 已在上述新授权窗口观测；只读同域请求仍不能冒充其测量。
