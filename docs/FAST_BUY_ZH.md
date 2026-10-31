# 单钱包 OpenFour 低开销路径（显式启用）

这一路径仅支持 **BSC `56`、以原生 BNB 支付的已知 WBNB 报价模板**：
`buyByBudget(address,uint256,uint256,uint256,bytes)`，`options=0`，空 proof。
不支持任意 ERC-20 支付、其他 options、迁移后的市场买单，也不发现 Core/Token。
调用方必须事先确认 Core、Token 与模板适用性；本功能不读取链上配置来替你确认。

**无 quote 的价格保护是绝对 `min-out-raw`，不是百分比滑点。**
预算为整数 wei；最低得到数量为代币最小单位整数，不能填人类可读小数。
`min-out-raw` 必填；零值必须额外给 `--allow-zero-min-out`（JSON 为 `allowZeroMinOut:true`），
这表示明确放弃最低数量保护。不要把 `1` 当成有意义的价格保护。

## 1. 纯离线编码：可以安全地用合成地址验证

```sh
B=./target/release/baw-rs
$B openfour encode-buy \
  --core 0x2222222222222222222222222222222222222222 \
  --token 0x3333333333333333333333333333333333333333 \
  --wallet 0x1111111111111111111111111111111111111111 \
  --amount-wei 100000000000000 \
  --min-out-raw 1000000000000000000
```

上述地址只是合成测试值，**不是可交易合约或钱包**。输出为标准 JSON envelope 中的
`binanceChainId/from/to/value/inputData`。这条命令不加载钱包、口令或会话，不发 RPC/HTTP，
也不使用 `BSC_RPC_URL`。`fast build` 接受相同意图的 stdin JSON：

```json
{"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"100000000000000","minAmountOut":"1000000000000000000"}
```

数量只接受十进制或 `0x` 十六进制**字符串**，不使用浮点；溢出 uint256、符号、空格、小数、
科学记数法、空字符串均拒绝，不截断高位。地址必须完整 160 位，`0x` 前缀，纯小写或有效 EIP-55，
不接受零地址。未知 JSON 字段拒绝。编码器由离线入口、fast 路径及原 `quote-buy` 最终 calldata 构建共享。
原 `quote-buy` 仍会询价；新入口不改变它的调用方式。

## 2. 常驻单钱包：一次进程，多次请求

以下是操作说明，**不是测试命令**；真实钱包执行会产生真实交易。本次交付未执行它们。
用户另行确认后，才可用自己的路径、地址、预算、最低数量和唯一操作 ID。

```sh
# 前台持久 JSONL 管道。没有每笔 CLI/Node 子进程，也没有每笔解密。
$B fast serve --wallet-dir /ABS/DEDICATED-WALLET \
  --instance-id-file /ABS/IDENTITY-FILE
```

输入每行一个 JSON 对象，以换行结尾；输出按相同顺序每行一个响应。请求形式（仅合成示例）：

```json
{"operationId":"UNIQUE-NO-REPLAY-001","confirm":true,"skipPrechecks":false,"buy":{"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"100000000000000","minAmountOut":"1000000000000000000"}}
```

- `confirm:true` 和唯一 `operationId` 必须显式提供。ID 为 1..64 个 ASCII 字母、数字、`-`、`_`。
- `skipPrechecks` 默认 **false**。严格模式读取开发者模式与钱包地址，并在 execute 前再次验证。
  最少 HTTP 请求模式必须对**每一笔**明确给 `skipPrechecks:true`：不做这些前置读取，但仍要求
  backend preview 没有显式 chain/from 冲突，且有发送方证据（顶层 from 或模拟余额变动 owner）、明确成功 simulation、有效 expiry；仍保留一次性日志及去重。
  不是关闭所有保护；也不是承诺后端一定允许。前置读取缓存仍受原客户端 `BAW_RS_CHECK_TTL_MS` 控制。
- 新路径直接传递请求内策略，**不读取或写入旧 `SKIP_PRECHECKS`/`SKIP_PRECHECKS_DEFAULT` 原子变量**，
  `BAW_RS_SKIP_PRECHECKS` 不能替新请求隐式开启跳过。不同请求的 skip 不会互相影响。
- 自动 preview→execute 之间没有人工风险审核，也没有策略文件白名单；风险和授权变更在响应中保留供记录。
  需要逐笔人工审核时，继续用原 `contract-call preview` / `execute --accept-preview-sha256`。
- 相同 Client 实例和 reqwest 连接池跨请求复用；是否真正复用连接仍取决于服务器、代理、协议和连接寿命。
  后端不支持 keepalive 时不能强制它复用。本地测试证明 HTTP/1.1 fixture 的复用，不代表生产 TLS 延迟。
- 单钱包串行；帧上限 64 KiB，内存队列容量 16。建议调用方一问一答，不批量预先塞入不可撤回订单。
- 使用与现有 resident 相同的独占锁。钱包已由 daemon 持有时会拒绝启动，**不会停止、重启或接管**现有服务。
  多钱包日常入口 `vaults`、普通 CLI 与旧 IPC 不变。
- 启动只加载一次身份/会话；每笔在开始时与 execute 之前分别检查目录及文件身份（device/inode/size/mtime 秒+纳秒/ctime 秒+纳秒）。
  权限、符号链接、替换或会话变化即拒绝继续，不能自动重放。它不是针对同一 OS 用户恶意篡改的隔离边界。

`examples/fast_driver.py` 演示持续持有一个 `fast serve` 子进程，一问一答，不是每笔起子进程：

```sh
# 真正执行入口；必须额外 --execute。不要拿合成示例去调用真实钱包。
python3 examples/fast_driver.py --binary "$B" \
  --wallet-dir /ABS/DEDICATED-WALLET --instance-id-file /ABS/IDENTITY-FILE \
  --execute < reviewed-orders.jsonl
```

driver 检查每行 `success` 与子进程退出状态；任何错误/EOF/超时就停止后续订单，不重启、不重试。
超时表示结果未知，仍等待现有子进程收尾并排空输出，避免杀掉正在提交的订单；它不是硬性取消交易。
直接使用 `fast serve` 时，进程正常 EOF 退出不代表每一行订单成功，调用方必须逐行检查 envelope。

## 3. 内存 preview 与持久化边界

新 fast 路径在同一调用中保留完整 input/output/requestId/expiry/binding record，直接验证后执行：
不把完整 preview JSON 写入 `signing-previews/*.json`，也不从该文件读回。
仍做显式成功的 simulation 验证、嵌套失败拒绝、chain/from 绑定、backend 顶层/受支持 tx 包装回显的 to/value/inputData 一致性、parsedTx 合约地址检查、
本地最多 300 秒 TTL 与所有出现的服务端 expiry 收紧、execute 前再次过期检查。

**不是完全无磁盘 I/O**：
- `.baw-rs-operations/<operationId>.json` 用 `O_EXCL` 拒绝重复操作，写入并同步意图；结束后同步状态及有限恢复标识。
- `signing-previews/<sha256(requestId)>.consumed` 在 execute 之前同步写入，跨进程/重启阻止相同 requestId 再用。
- 目录创建、锁、元数据检查、`sync_all` 都保留，没有偷偷去掉持久性。
- 同一次请求记录绑定 wallet inode/device/client、chain、sender、原始 input、preview 输出及 requestId；
  execute 发送该记录的 requestId，不接受调用者替换。
- 超时/断线/异常 execute 响应保持 `FAILED_OR_UNKNOWN`，不自动重试，也不把收到响应说成链上成交。
  操作 ID/请求 ID 去重不等于后端幂等；不要换新 ID 重放同一笔未知结果交易。

普通 `preview` 写缓存、人工审核 digest、独立 `execute` 的兼容路径保留。

## 4. 同一次调用的计时

全部用单调 `Instant`，单位毫秒。响应 `timings`：

| 字段 | 含义 |
|---|---|
| `encode_ms` | 参数校验、精确整数与 calldata 编码 |
| `local_prep_ms` | 本地 JSON/意图准备、缓存目录准备、digest 等；不包含 HTTP 和日志阶段 |
| `journal_ms` | 意图日志 begin/finish，包含同步落盘 |
| `consume_ms` | 一次性请求标记，包含同步落盘 |
| `prechecks_ms` | 钱包前置/复核读取或显式跳过的本地检查 |
| `validation_ms` | preview 结构、模拟、expiry 等检查 |
| `preview.full_roundtrip_ms` | preview HTTP 整次调用至 body drain + JSON decode 完成 |
| `execute.full_roundtrip_ms` | execute HTTP 整次调用至 body drain + JSON decode 完成 |
| `*.request_ms` / `*.body_ms` | send→headers 与 headers→body/decode 的细分，不要重复加到 full_roundtrip |
| `queue_ms` | 完整 JSONL 帧读取后至 worker 取出；不包含尚在 OS 缓冲区的等待 |
| `state_check_ms` | 请求开始与 execute 前的文件/目录固定身份检查总和 |
| `total_ms` | 帧读完至生成响应前；含队列，不含进程冷启动、首次解密、stdout 发送 |

未开始的 HTTP 阶段为 null。失败路径也尽量返回已完成计时。
**HTTP full roundtrip 是客户端观察到的往返总耗时，不是“币安后台处理时间”。**
这些计时不能推导生产下限、链上打包延迟或最快成交保证。

## 5. 仅离线 / loopback 验证

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib fast::tests -- --nocapture
cargo test --test fast_cli
python3 tests/test_fast_driver.py
cargo clippy --all-targets -- -D warnings
cargo test --release --lib benchmark_fast_mock -- --ignored --nocapture
```

测试使用合成 credentials 和临时目录；生产 origin override 仅存在于 `#[cfg(test)]`。
覆盖独立 Python `eth_abi`/Keccak 编码 fixture、uint256 边界、无 RPC listener 接受、未知字段与地址失败、
一 preview+execute 对、磁盘毒化 preview 文件不影响内存路径、一次性消费、重放、simulation 失败、expiry、
请求回显不匹配、完整 body 延迟、串行队列、断线不重试和会话元数据变更。

连接测试由本地 keepalive HTTP server **计数实际 accept() 与请求路径/body**：两单每次新 Client 为 2 sockets，
持久 Client 为 1 socket，均 4 HTTP requests；不是根据 `ss` 静态连接数作结论。
benchmark 每组 100 单，固定 0ms / 5ms 每 HTTP 人工延迟；逐单验证 success，总请求数和 sockets 都有断言，
输出原始 samples。没有真实交易、凭据读取、生产服务部署/重启，也没有把 mock 数字当生产性能。

## 已知旧路径问题（本次不扩大重构范围）

- 已修复普通 dispatch 忽略 CLI skip 的原有失败：整个异步 dispatch 使用 task-local 策略，
  不再被其他请求修改旧原子变量影响；该回归测试已通过。新 fast 路径仍直接传递请求局部 bool。
- `resident.rs::session_stamp` 只存 `mtime_nsec`，缺少秒和 ctime；新 fast serve 独立采用完整 stamp。
- 旧 quote 解析 `words()` 对大数保留低 128 位，属于旧询价路径的风险；新离线/fast 编码不调用它，
  支持完整 uint256 且溢出拒绝。本次没有修改旧 quote 读取布局，避免不相关协议回归。
- 未验证真实后端响应兼容性、生产 TLS/网络延迟、链上执行、token 模板适用性或实际成交。
  没有授权真实测试，不应为“证明快”而下单。

## 复核修复

- 后续坏帧不再取消已经开始的 execute：停止接收后排空已接受订单，再回报 framing 错误。
- execute 必须有可识别的非空 orderId 或完整 txHash；未知响应归为 FAILED_OR_UNKNOWN，不自动重发。
- simulationCode=000000000 为明确成功；失败码拒绝，模拟结果数组每一项都必须明确成功。
- 兼容历史真实预览的字段结构（合成地址/数额），但不等于本次做过生产实测。
- 仍信任币安把返回的 requestId 绑定到本次认证请求；本地摘要不是后端执行内容的独立密码学证明。

## 本次交付验证结果

全量 Rust 测试 326 通过、0 失败；1 项基准默认忽略，已单独执行通过。Python driver 3 项通过。
release 构建、fmt、clippy `-D warnings` 通过。原始基准样本与源码哈希见
`verification/fast-group1.json`。基准每组 100 单：

- 本地 HTTP 无人工延迟：总耗时 P50 10.79 ms、P90 15.66 ms。
- 每次 HTTP 人工延迟 5 ms：总耗时 P50 21.54 ms、P90 24.88 ms。
- 两组均 200 次 HTTP、实际 accept 1 次；没有 RPC 报价请求。

以上包括保留的持久化日志和消费标记，不含生产 TLS、币安后端、链上打包，不能当作真实买入耗时。
未切换线上 daemon，未读取真实凭据，未真实下单。
