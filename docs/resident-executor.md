# 通用合约常驻执行器（typed resident IPC）

本文对应 `src/resident.rs`、`src/executor.rs` 及 `examples/persistent_client.py`、
`examples/benchmark_persistent.py` 当前源码，不代表已部署到现有服务，也不代表已实盘验收。
它是现有 resident 的 Unix socket 协议扩展，不是新的 `prepare` CLI 子命令，
也不是原生事件任务通道。仅支持 EVM 合约调用。

## 路径与授权边界

- 策略/WS 监听器决定何时触发；执行器只负责具名钱包绑定、固定调用、预览→执行、防重和结果。
- `prepare` 做静态校验、加载/复用内存会话和 HTTP Client、核对发送方/开发者模式并预约钱包。
  它可能发出只读 HTTP 请求，但**不获取交易预览、不签名、不提交 execute**。
- `trigger` 不启动 CLI、不重新解析 argv；仍会进行必要的前置检查、生成新预览、验证风险/意图、
  消费预览再提交 execute。前置查询可能命中 resident 缓存，但不能承诺触发时完全没有只读 HTTP。
- 参数在 prepare 时固定，没有 calldata、金额、gas、目标等动态 patch API。
  更改意图要在尚未触发时 disarm，再用新的 operation ID prepare；不得借此重发未知结果。
- `prepare` 没有 `confirm` 字段；`trigger` 必须显式传 `confirm:true`。
  Python helper 默认 `confirm=False`，不会自动授权。软件确认不能代替用户对钱包、完整地址、金额、
  费用、风险和触发条件的授权。现有服务的升级/停启需要单独授权。

## 连接、权限与分帧

使用现有 `baw-rs wallets serve` 所拥有的 `<REGISTRY>/daemon.sock`，`name` 必须是启动快照中存在的名字，
错误名字直接失败、无默认钱包/回退。传统单钱包 `serve` 使用它自己的 socket，钱包名固定为 `single`。
不要为演示连接真实钱包或重启生产服务。

协议是 UTF-8 JSONL：每条 JSON 后一个 `\n`，不是长度前缀，也不以 EOF 分帧。
请求 JSON 最多 65,536 字节（不含换行），响应最多 8 MiB（不含换行）。未知顶层字段和未知 `call` 字段被拒绝。
同一 socket 可以处理多条 `ping` / `prepare` / `trigger` / `disarm` 请求；这些请求都必须带字符串 `id`。
服务器原样回传 `id` 用于关联；它不是去重键，也没有业务 operation ID 的格式约束。

- socket 目录要求本用户所有且 0700，socket 为 0600；服务端检查连接者 UID，Python 客户端也检查服务端 UID。
  这是同用户 IPC，不是跨用户/网络认证接口。
- **每个连接按请求顺序完整执行、写完响应后才读下一条**，没有请求多路复用。
  即使提前写入多帧，后续请求也要等前一条完成；不同钱包要并行需独立连接。
  Python `PersistentClient` 没有线程锁，同一实例不要被多个线程并发调用。
- 最多 16 条持久连接；入口另有 64 条连接接纳上限。满额会关闭连接或返回 BUSY，不是无界排队。
- 每次读下一帧最多等 **3 秒**，响应写入也限 3 秒；“持久”不等于永久空闲存活。
  空闲监听时可在小于 3 秒间隔发 ping 并读回响应。连接已失效时可为未来新请求重建连接，
  但若 trigger 已发出或是否发出不明，重连绝不等于获准重发。
- `run` / `status` / `stop` 保持旧的一次请求连接语义，没有 `id` 字段，不要用自动附加 `id` 的
  `PersistentClient.request()` 直接调用这些旧 op。`run` 继续走 argv 分发，不是 typed trigger。

## 精确请求结构

下表字段名区分大小写。所有示例地址都是**仅供离线 fixture 的合成地址，非实盘目标**。
`deadline_ms` 是 Unix epoch 毫秒绝对时间，不能照抄过时常量。

| op | 必填字段（除 `op`） | 可选字段 |
|---|---|---|
| `ping` | `id: string` | 无 |
| `prepare` | `id: string`, `name: string`, `operation_id: string`, `deadline_ms: u64`, `call: object` | `policy: object或null` |
| `trigger` | `id: string`, `name: string`, `operation_id: string`, `confirm: boolean` | 无 |
| `disarm` | `id: string`, `name: string`, `operation_id: string` | 无 |

`call` 字段：

| 字段 | 类型与含义 |
|---|---|
| `binance_chain_id` | 字符串，如 `"56"`，不是 CLI 的 `--binanceChainId` |
| `from` | EVM 发送方地址字符串，必须通过钱包身份检查 |
| `to` | EVM 目标地址字符串 |
| `value` | 无符号整数 **wei 字符串**；推荐十进制，不使用浮点数或 BNB 小数 |
| `input_data` | `0x` 十六进制 calldata 字符串；不是 `inputData` |
| `gas_limit` | 可省略或 null；给定时为 JSON u64 整数，非字符串 |

`operation_id` 为 1..64 个 ASCII 字母、数字、`-`、`_`，与普通写操作共享该钱包的持久 journal 命名空间。
`id` 是每次请求的关联 ID，`operation_id` 是业务防重复 ID，两者不要混淆。

以下代码**只构造并打印离线协议示例，不连接 socket、不交易**：

```python
import json
import time

# 合成地址，仅用于离线演示，不得发送到实盘钱包。
call = {
    "binance_chain_id": "56",
    "from": "0x1111111111111111111111111111111111111111",
    "to": "0x2222222222222222222222222222222222222222",
    "value": "0",
    "input_data": "0x12345678",
    "gas_limit": 100000,
}
requests = [
    {"op": "prepare", "id": "req-prepare", "name": "alice",
     "operation_id": "synthetic-demo-1",
     "deadline_ms": time.time_ns() // 1_000_000 + 60000, "call": call},
    {"op": "trigger", "id": "req-trigger", "name": "alice",
     "operation_id": "synthetic-demo-1", "confirm": True},
    {"op": "disarm", "id": "req-disarm", "name": "alice",
     "operation_id": "synthetic-demo-1"},
]
for request in requests:
    print(json.dumps(request, separators=(",", ":")))
```

这些是各 op 的结构展示，不是“trigger 后 disarm 撤单”的流程；触发后 disarm 不会取消执行。
正确业务顺序为 prepare → 等待事件 → **trigger 或 disarm 二选一**。
Python 对应方法签名是 `prepare(name, operation_id, deadline_ms, call, policy=None)`、
`trigger(name, operation_id, *, confirm=False)`、`disarm(name, operation_id)`。
独立运行 `python3 examples/persistent_client.py <SOCKET>` 只发送 ping；连接现有服务也须在授权范围内。

### policy

省略或 null 时生成 `{version:1, chainId:call.binance_chain_id, to:call.to, valueWei:规范化金额}`。
默认不允许预览风险项和权限/allowance 变化。显式 policy 使用现有策略解析器：
必填 `version:1`、`chainId:string`、`to:string`；可选 `valueWei:string`（默认 `"0"`）、
`selector:string`、`calldataSha256:string`、`maxGasLimit:u64`、`allowRisks:string[]`、
`allowAuthorityChanges:boolean`。`calldataSha256` 是 **calldata 文本字符串的 UTF-8 字节** SHA-256，
不是解码后的 EVM 字节；为 64 位小写十六进制。`selector` 为 `0x` 加 4 字节。
resident 明确拒绝非 null 的 `maxTotalWei`，因为当前预览手续费证据无法可靠执行该护栏；不要声称有总费用上限。
policy 内部不是与 Request/Call 相同的严格未知字段反序列化器；不要把未识别字段当作已生效约束。
即使没有显式 calldata digest，prepare 保存的整个 call 也不能由 trigger 修改。

## 截止时间不是链上取消

prepare 接受的 `deadline_ms` 必须严格晚于当前时间且在接下来的 **300,000 ms** 内。
准备期间若已过期也失败。任务同时保存 wall-clock 和 monotonic 截止时间，任一个到期即过期。

trigger 入口检查任务期限/身份；新预览及必要检查完成后，在调用
`/contract-call/execute` **之前再次检查**。到期返回 `EXPIRED; no execute submitted`，不会提交 execute。
这不保证此前没有发出只读/preview 请求，也不保证没有消费预览或写入 operation journal。

**期限仅约束 execute 提交前的本地检查，不是远端接收、广播、打包或完成期限。**
检查通过后可能发生调度/网络延迟；execute 一旦发出，即使墙钟跨过 `deadline_ms` 也继续等响应，
不会在到点时取消已提交交易。trigger 内签名流程另有 360 秒命令超时；Python 客户端默认 socket 超时 400 秒。
这些超时都不能证明远端没执行。返回 orderId/txHash 也不等于链上成功，需要另行对账 receipt/业务状态。

## 预约、争用与 operation ID

- 每个钱包最多一个 PREPARED 任务；prepare 成功后，同钱包普通 `run` 命令（包括只读查询）返回
  `BUSY; wallet reserved by prepared task`。准备/执行中锁忙也直接 BUSY，不把触发塞入普通 FIFO 队列。
  其他钱包可经其他连接工作；`ping` / 后台状态不是钱包查询替代品。
- 已在执行的普通命令不能被抢占，prepare/trigger 的 try-lock 会失败；普通命令取 state 锁前后都检查预约。
  准备好不代表能抢占已经开始的工作。过期、disarm 或触发处理结束后释放预约；过期清理按后续访问惰性进行。
- 同一进程内，对同一仍有效的任务，以相同 ID、call、deadline、身份绑定和解析后的 policy 再 prepare
  可返回 PREPARED；不同参数或另一个活动任务报冲突/BUSY。其余校验仍执行，不能无限续期。
- trigger 要先通过确认/ID 校验，再取出活动任务并预先放入 UNKNOWN 墓碑，之后即使遇到身份变化、
  BUSY、过期或其他失败，任务也不会自动重新武装。执行中另一个 trigger 会因锁忙返回 BUSY；
  同一进程内任务结束后的重复 trigger 返回所存终态/错误，不再执行。
- disarm 只会移除**尚未触发**的活动任务并保存 DISARMED。对已有终态返回缓存结果，
  对执行中的任务可能 BUSY；它不是取消 HTTP、撤单或链上取消。
- 会话/identity 文件绑定发生变化会使旧任务失效，不静默切换身份。未知名字/operation ID 无回退。
- 每钱包内存终态最多积累 256 个；达到上限后拒绝新 prepare，提示先对账再安排 resident 重启。
  不为了清空历史擅自重启生产后台。ID 由调用方唯一分配，不自动换 ID 重试。

### 断线、未知结果与重启

连接断开不意味着钱包工作已取消。trigger 已受理后可能继续执行；丢响应、超时、journal 写回失败等都按
UNKNOWN/失败待对账处理：**不自动重试，不换 operation ID 绕过去，不回退到 CLI 或另一个钱包**。
检查返回 `success` 之外也要检查 `data.execution` 的业务状态；接口响应不是成交证明。

内存 PREPARED/终态表**不跨 resident 重启恢复**，没有自动恢复武装或任务查询 op。
prepare、disarm、以及触发前置失败并不保证创建磁盘 journal；不能把进程内去重宣传成所有状态都永久保存。
trigger 在签名/预览流程前调用 `OperationGuard::begin`，以排他创建方式写
`<WALLET_DIR>/.baw-rs-operations/<operation_id>.json` 并 fsync；记录初态 SUBMITTING，
结束时为 RESPONSE_RECEIVED 或 FAILED_OR_UNKNOWN，成功响应中有限的 orderId/txHash/status 等可作为恢复线索。
RESPONSE_RECEIVED 只说明获得响应，不说明交易成功。

重启后旧任务不存在；同 ID 的 prepare 当前可能成功（它不查 journal），但 trigger 在发现已有 journal 文件时
拒绝再执行，并非重放旧结果。已 journal 的 ID 不能靠重启复用；未 journal 的旧任务也不能在结果不明时盲目重建。
只读检查 journal、订单与链上状态后再决定后续授权，不删除 journal 解锁重发。

## 响应与计时口径

正常 typed 响应为 `{"success":true,"data":...,"id":"关联ID","metrics":...}`；错误为
`{"success":false,"error":{"message":"原因"},"id":"关联ID","metrics":...}`。
PREPARED/DISARMED 的 `data` 含 `status` 与 `operation_id`；ping 的 data 为 `{"status":"ready"}`。
trigger 成功的 data 含 `preview`、`execution`、`policy`、`timings`、`http_metrics`、`total_ms`。
这只是字段说明，不是实测输出。无效帧/超限等错误可能没有可关联的 id；超大响应替换错误也可能丢失 id/metrics。
错误响应不保证保留 fire/HTTP 分段指标，不要凭空补齐。

| 指标 | 实际边界（毫秒） |
|---|---|
| `metrics.ingress_decode_ms` | 服务端完整读到一帧之后到解码/关联处理后的间隔；不含 socket 收帧、客户端写入或上游等待 |
| `metrics.queue_ms` | 当前固定为 `0.0`；typed 钱包争用返回 BUSY，没有计入同连接前序请求等待，不能据此断言系统从不排队 |
| `metrics.handler_ms` | 从完整帧读完到包装响应前，包含解码、HTTP 等待和同步持久化；不含响应 JSON 序列化/写 socket |
| `data.total_ms` | trigger 通过授权校验后开始，到成功结果组装时；不含 prepare、socket 传输及完整响应封装 |
| `http_metrics.first_request_gap_ms` | 从完整 trigger 帧读完到首个 HTTP send 调用前，包含本地处理/意图 journal；不是从外部事件产生开始 |
| `http_metrics.network_wait_ms` | 累加 HTTP send await 与响应 body chunk await；含传输和调度等待，不是纯网络 RTT 或后端计算时间 |
| `http_metrics.response_to_next_request_ms` | 上次 HTTP 响应处理完成到下次 send 前的各个间隔（最多记录 32 项），不是只有签名耗时 |
| `http_metrics.requests` | trace 中进入 send 的 HTTP 请求数；不直接证明 TCP/TLS 握手数 |
| `timings.preview` / `execute` | HTTP 分段对象：`request_ms`（send 到响应头）、`body_ms`（响应体读取与解码）、`full_roundtrip_ms`，可为 null；完整本地调用不是后端耗时 |
| `timings.validation_ms` / `local_prep_ms` / `prechecks_ms` / `consume_ms` | 相应代码块累计计时；prechecks 可含 HTTP，consume 含预览持久消费；这些不是互斥且穷尽的全部总耗时分解 |

重复 trigger 返回的是缓存业务结果，因此 `data` 内计时可能仍属于首次执行，外层 `metrics` 是本次请求。
不要把新外层耗时和旧 HTTP trace 拼成一次新执行。

## 离线基准：命令、范围与未测项

**本文没有基准数字；尚未在本文交付中实测，不提供加速倍数或伪造输出。**
在仓库根目录，使用新建的 scratch 构建目录，不覆盖在用二进制：

```sh
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:?}/baw-rs-resident-bench.XXXXXX")"
cargo build --release
BAW_RS_CHECK_TTL_MS=3000 \
BAW_RS_BENCH_BINARY="$CARGO_TARGET_DIR/release/baw-rs" \
  cargo test --release persistent_executor_loopback_benchmark -- --ignored --nocapture
```

Rust ignored fixture 建立临时合成钱包/会话和回环 HTTP 后端，再调用 `examples/benchmark_persistent.py`。
脚本校验 fixture token 和 `127.0.0.1` origin，不应直接对真实 registry 运行；不需要真实钱包凭据或 RPC。
两个路径对同一 mock 提交相同的合成通用 EVM call，保留 operation/preview 的持久化 fsync。
旧路径是**真实 CLI 子进程** `--wallet NAME contract-call fire`（旧写法 `vaults run NAME --` 仍受支持）；新路径复用 Python socket，并分开计 prepare 与 trigger。
这不是“新旧 resident 二进制版本”的历史对比。

默认 100 个记录样本，另有 5 轮预热；每轮交替路径先后顺序以减小偏差，输出 nearest-rank P50/P95/P99（ms）。
脚本允许 1..120 个样本，fixture 当前调用固定 100。主要列包括 old CLI、typed prepare、typed trigger、
两者之和、首 HTTP 间隔、HTTP await 累计、`handler_ms - network_wait_ms`（本地残余，非纯 CPU）、
响应到下一请求间隔、ingress 与固定为零的 queue。响应间隙的分位数按收集到的间隙数计算，不一定等于请求样本数。

仅对**本地回环、合成后端、预热后的顺序调用**有效。不能推出生产网络/链上延迟、广播或确认速度、
并发吞吐、查询争用尾延迟；本脚本没有并发查询负载。typed trigger 不含 prepare，公平比较时同时看
prepare+trigger；loopback HTTP await 仍含调度/传输，不冒充远端服务计算。

### 已验证的隔离基准

100 个记录样本、5 轮预热，release 构建，单位 ms，nearest-rank 分位数：

| 路径 | P50 | P95 | P99 |
|---|---:|---:|---:|
| 原 CLI 调用路径 | 16.208 | 23.043 | 33.706 |
| typed prepare | 0.313 | 0.694 | 1.361 |
| typed trigger | 11.833 | 15.990 | 18.940 |
| prepare + trigger | 12.232 | 16.398 | 19.280 |
| 接单到首 HTTP 请求 | 3.550 | 4.649 | 5.571 |

此处“原 CLI”是同一新增版本内的传统入口，不是历史旧版本二进制。保留同步 fsync；未证明亚毫秒执行、生产吞吐或连接复用收益。完整检查：348 项通过、0 失败、2 项 ignored（另有手动执行的本次 benchmark），fmt/clippy 通过。现有生产服务未部署本次代码。

