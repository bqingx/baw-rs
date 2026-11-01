# typed resident 通用合约执行器

适用：用户明确选择 baw-rs 的低本地开销 EVM 固定合约调用。依据当前 `src/resident.rs`、
`src/executor.rs` 与 `examples/persistent_client.py`、`examples/benchmark_persistent.py`，
不是已部署/实盘验收声明。仓库完整说明：`docs/resident-executor.md`。
不要把以下 op 编成 CLI 子命令，也不要把原生事件任务通道当作通用执行路径。

## 接口与准确字段

连接已授权使用的 resident Unix socket；多钱包为 `<REGISTRY>/daemon.sock`，名字取启动 registry 快照。
传统单钱包 `serve` 的名字为 `single`，使用其自己的 socket。名字/ID 错误无默认值、无回退。
每帧 UTF-8 JSON 加 `\n`，请求不含换行最多 65536 字节，响应最多 8 MiB。
顶层请求与 `call` 禁止未知字段。typed op 都要求字符串 `id`，响应原样回传；它只做关联、不防重复。

| op | 必填字段（除 op） | 可选 |
|---|---|---|
| `ping` | `id:string` | 无 |
| `prepare` | `id:string`, `name:string`, `operation_id:string`, `deadline_ms:u64`, `call:object` | `policy:object或null` |
| `trigger` | `id:string`, `name:string`, `operation_id:string`, `confirm:boolean` | 无 |
| `disarm` | `id:string`, `name:string`, `operation_id:string` | 无 |

`call` 精确结构（**合成地址，仅供离线 fixture，非实盘目标**）：

```json
{
  "binance_chain_id":"56",
  "from":"0x1111111111111111111111111111111111111111",
  "to":"0x2222222222222222222222222222222222222222",
  "value":"0",
  "input_data":"0x12345678",
  "gas_limit":100000
}
```

前五个字段为必填字符串；value 单位 wei、用无符号整数字符串，推荐十进制，不用 float。
`gas_limit` 是可省略/null 的 JSON u64 整数，不是字符串。只支持 EVM。
`operation_id` 是 1..64 位 ASCII 字母/数字/`-`/`_`，与普通写操作共享该钱包持久 journal 命名空间。
`deadline_ms` 是 Unix epoch 毫秒绝对时间，须在未来 300000 ms 内，不能给相对秒数。

Python helper：

```python
# 方法签名；非交易执行示例。
client.prepare(name, operation_id, deadline_ms, call, policy=None)
client.trigger(name, operation_id, confirm=True)  # 默认 False，必须有用户授权
client.disarm(name, operation_id)
```

prepare 没有 confirm 字段。不要因为它不提交交易就擅自准备真实资金策略/独占钱包。
prepare 会校验固定 call、加载/复用会话和 HTTP client、检查发送方/开发者模式并预约钱包，可能访问只读 HTTP；
**没有预先创建交易 preview**。trigger 仍需新 preview、风险/意图检查、持久消费与 execute。
前置查询可能命中缓存，但不承诺触发时没有只读 HTTP。
没有 trigger patch、动态 calldata/value/gas/target。若要改固定调用，只能在尚未触发时 disarm，
经授权换新 ID 准备；不能用于重发未知结果。

policy 省略/null 时为固定链/目标/value 的 version 1 策略，默认拒绝预览风险项与权限/allowance 变化。
显式字段沿用策略解析器：必填 `version:1`、`chainId:string`、`to:string`；可选 `valueWei:string`（默认 `"0"`）、
`selector:string`（0x+4字节）、`calldataSha256:string`（calldata **文本 UTF-8** SHA-256，64位小写hex）、
`maxGasLimit:u64`、`allowRisks:string[]`、`allowAuthorityChanges:boolean`。
非 null `maxTotalWei` 被 resident 拒绝，不能宣称总费用护栏已实现。policy 内部不是 deny_unknown_fields，
不要把未知字段当生效约束；固定 call 本身仍不可被 trigger 改写。

## 持久连接与争用

- 同 UID IPC：目录为本用户所有的 0700，socket 为 0600；双方检查 peer UID。Python helper 在连接时校验权限。
- 同一连接**逐请求执行并按序回响应**；没有多路复用。要并行不同钱包需不同连接，helper 实例不支持多线程并发调用。
- 读下一帧限 3 秒，响应写限 3 秒；空闲不能永久保持。可在小于 3 秒间隔 ping 并读响应。
  最多 16 条持久连接，入口另有 64 条接纳限制；BUSY/断线不是可盲重发信号。
- `run/status/stop` 是旧单请求 op，不带 id；不要通过自动附加 id 的 `PersistentClient.request()` 发这些 op。
- 每钱包最多一个 PREPARED。预约期间同钱包普通 `run` **包括只读查询**被拒 BUSY；准备/触发锁争用也直接 BUSY，
  不等普通 FIFO 队列。已经执行的普通命令不能被抢占。别为了腾出通道停服务、复制会话绕锁。
- 其他钱包可独立连接工作。预约因 disarm、过期或 trigger 完成释放，过期清理按后续请求惰性发生。
  状态/ping 不提供逐任务状态查询能力。

## 截止与终态

1. 同一活动任务，相同 operation ID/call/deadline/身份/policy 的重复 prepare 可以返回 PREPARED；
   不同参数或其他活动任务报冲突/BUSY。验证仍执行，不是无限续期。
2. trigger 必须 confirm=true；通过校验后移除活动任务，先留 UNKNOWN 墓碑再做异步/可失败工作。
   后续失败也不自动重新武装。执行中重复 trigger 因锁忙返回 BUSY，结束后重复 trigger 返回内存缓存终态，不再次执行。
3. 入口和 **execute 提交前**检查期限与会话/identity 绑定，期限同时受墙钟与单调时钟限制。
   execute 前过期返回 EXPIRED；此时仍可能已发 preview、消费预览、写 journal。
   **deadline 不是远端接收/广播/打包/完成期限。** 本地检查后仍有调度/网络延迟，execute 发出后
   不因到点而取消。签名流程另有 360 秒超时，Python socket 默认 400 秒，超时不证明远端未执行。
4. disarm 只解除尚未触发的任务并存 DISARMED。已终态时返回缓存结果；执行中可能 BUSY，不能取消提交/链上交易。
5. 身份/会话文件绑定变化使任务失效，无静默换钱包。终态表每钱包最多 256 个，满后需先对账、再获授权安排重启。

## UNKNOWN 与重启 journal

断线/丢响应不会自动取消已受理任务。结果不明时**不重发、不换 ID、不回退 CLI/另一钱包**，先核对 journal、
订单和 receipt。success=true/orderId/txHash/RESPONSE_RECEIVED 都不等于成交，仍查业务状态和链上结果。

内存 PREPARED/终态不跨重启恢复，没有自动重新武装。prepare/disarm 不写持久防重，trigger 前置失败也可能尚无 journal。
trigger 在签名/preview 前以排他创建写 `<WALLET_DIR>/.baw-rs-operations/<operation_id>.json` 并 fsync，
初态 SUBMITTING，结束为 RESPONSE_RECEIVED 或 FAILED_OR_UNKNOWN；有限的 orderId/txHash/status 可作为恢复线索。
重启后同 ID prepare 可能成功（不查磁盘 journal），但 trigger 发现已有 journal 会拒绝执行，**不会重放旧成功响应**。
未创建 journal 的旧任务也不能在结果不明时盲目重建。禁止删除 journal 绕防重。

## 结果与指标

typed 成功外层为 `success:true,data,id,metrics`；错误为 `success:false,error.message,id,metrics`。
prepare/disarm 的 data 含 status（PREPARED/DISARMED）、operation_id；trigger 成功含 preview、execution、policy、
timings、http_metrics、total_ms。错误不保证有分段指标；畸形/超大响应可能无关联 id。

- `metrics.ingress_decode_ms`：完整帧读完后的解码处理，不含收帧/客户端等待。
- `metrics.queue_ms`：固定 0；锁争用拒 BUSY，不测同连接前序等待，不等于所有层面从不排队。
- `metrics.handler_ms`：完整帧读完至封装响应前，含 HTTP await/fsync，不含响应序列化/写 socket。
- `data.total_ms`：trigger 授权检查后至成功组装，不含 prepare。
- `http_metrics.first_request_gap_ms`：完整帧读完至首个 HTTP send 前，含意图 journal；不是外部事件延迟。
- `network_wait_ms`：send 与 body chunk await 累计，含调度/传输，不是后端计算或纯网络时间。
- `response_to_next_request_ms`：前一 HTTP 响应处理完到下一 send 的间隙数组，最多32项，不是纯签名耗时。
- `requests`：进入 HTTP send 的次数，不是连接/握手数。
- `timings.preview/execute`：HTTP `request_ms/body_ms/full_roundtrip_ms` 可为null；body_ms含响应体读取与解码，另有 validation_ms、
  local_prep_ms、prechecks_ms、consume_ms。prechecks可能含网络，consume含持久化；这些不是完整无重叠总耗时分解。
- 重复 trigger 的 data 指标是缓存原结果，外层 metrics 是新请求，不能混算为新执行。

## 仅离线 benchmark

**隔离实测已完成：**release，5轮预热/100样本，原CLI P50/P95/P99 为16.208/23.043/33.706 ms；typed prepare+trigger 为12.232/16.398/19.280 ms；预先准备后仅trigger为11.833/15.990/18.940 ms；接单到首HTTP为3.550/4.649/5.571 ms。保留同步持久化；不能当作生产延迟、吞吐、连接复用收益或历史版本比较。全量348通过/0失败/2 ignored，源码未部署。在仓库根目录复现实验，构建到scratch，不覆盖生产二进制：

```sh
export CARGO_TARGET_DIR="$(mktemp -d "${TMPDIR:?}/baw-rs-resident-bench.XXXXXX")"
cargo build --release
BAW_RS_CHECK_TTL_MS=3000 \
BAW_RS_BENCH_BINARY="$CARGO_TARGET_DIR/release/baw-rs" \
  cargo test --release persistent_executor_loopback_benchmark -- --ignored --nocapture
```

仅通过 Rust ignored fixture 运行 Python benchmark；它生成临时合成会话/回环 HTTP 后端并校验 token，
不需真实钱包凭据/RPC，不能直接换实盘 registry。
同一 synthetic 通用 call 比较真实 CLI 子进程 `--wallet NAME contract-call fire` 与持久 typed socket；保留 journal/preview fsync。
旧路径含进程启动，新 trigger 不含 prepare，同时报告 prepare 和 prepare+trigger。
5轮预热、100个记录样本、交替顺序、nearest-rank P50/P95/P99；脚本可设1..120，fixture固定100。
还报告首HTTP间隙、HTTP await、本地残余 handler-await、响应间隙、ingress、固定0的queue；
间隙分位数按间隙数量统计。不代表生产网络/链上耗时、吞吐或并发查询负载尾延迟；该脚本没有并发查询负载。
