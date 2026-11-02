# 原生任务（已出块日志 / 待处理交易 → 合约调用）

本文对应 `src/native_events.rs`、`src/native_pending.rs`、`src/route_action.rs` 当前源码。旧版日志任务已在本机部署实跑；**本次新增的相对有效期与待处理交易监听尚未切换至运行中的后台**。旧版实跑不代表新增功能已完成生产验收，部署与真钱验证另行授权。

## 本次变更：有效期与待处理交易

- 必须填写 `valid_for_ms` 正整数毫秒时长，不填、0、负数或数值溢出拒绝；无固定五分钟上限。
- 从首次 `arm` 启用尝试开始计时，准备检查也计入；`load` 不开始计时。重复启用、重连和重启不能延长原授权。
- 新配置拒绝绝对 `deadline_ms`，也拒绝混用；内部计算的截止点只用于执行与状态展示。
- 到期停止等待并释放钱包；执行提交前仍检查有效期，已提交的交易不因到期撤回。
- `source` 默认 `{"kind":"logs"}`。待处理模式需显式配置 `{"kind":"pending","subscription":"hashes","from":"0x发送方","to":"0x合约","selector":"0x四字节方法","args":[]}`。
- pending 的发送方、目标合约和方法必须固定；`args` 显式数组可为空，静态ABI参数可按 `index`、`abi_type`、完整32字节 `equals` 精确匹配，不能按不存在的日志topics匹配。
- `hashes` 订阅 `newPendingTransactions` 后查交易详情；`full` 显式订阅完整对象扩展。节点不支持、链ID错误、消息畸形明确失败，无日志回退。
- pending 不接受日志专用filter或非零的start_block/confirmation_depth/max_backfill_blocks/chunk_blocks。其 `words` 动作只允许实际交易input静态槽位和constant，不允许topic/data/事件区块read。
- pending断线存在无法补齐的可见性缺口，明确失败，不用日志补读伪装恢复。队列过载失败，不静默丢通知。节点只能看到自身可见的交易池，不保证全网覆盖。
- 目标交易可能取消、替换或失败；自己已提交的抢单不会因此自动撤回。看到开盘pending交易仍可能因当前链上状态未开放而模拟失败，不绕过预览、不保证排序。
- 日志与pending的去重都是滑动窗口：满窗口只淘汰最早记录，不再因为累计数量把长任务判失败。pending匹配只按配置的参数位做完整32字节比较，未配置参数时动态尾部数据只是不匹配，不会让整个任务失败；但如果动作是`words`并要取交易输入里的某个字，而这次命中的交易数据本身不足以定位该字，构造阶段会明确失败并释放钱包（不会发送交易）。
- 终态写盘失败时，持久化失败信息会合并进已有结果；已经提交的交易不会被改写成“未提交”，需按未知去查链。写临时状态文件失败会清理残留临时文件。

全量门禁（fmt、clippy、268项测试、release构建）与独立复审均已通过，复审零阻塞项；以上不构成已部署或已生产验证的声明，部署与真钱验证需另行授权。

## 定位

把「订阅链上事件 → 满足条件后执行合约调用」直接做进 baw-rs 的多钱包常驻后台：

```text
WS 已出块日志 / 待处理交易通知
  → 过滤与去重
  → 构造本次合约调用
  → 策略校验
  → 复用现有签名/预览/执行核心
  → 持久化结果
```

外部推送、Shell、`eval`、运行期脚本**都不是**必需部件，也不被接受作为动作来源。

动作是**通用合约调用**，不限于买卖：mint、claim、授权、质押或任意已授权调用都走同一条链路。
买、卖只是构造方式不同：

| 动作类型 | 说明 |
|---|---|
| `fixed` | 事件命中即执行预先固定的调用 |
| `words` | 事件字段（topic/data）或事件区块上的只读 `eth_call` 结果，经**受控**算术与 ABI 编码拼入 calldata |
| `route` | 事件发生后**实时**向路由服务取一笔新报价构造调用；**不在触发前预取或复用** |

`route` 当前只内置 OKX V6 适配器，它是可选构造器，不是所有动作的必经步骤。

## 命令行

```sh
# 钱包名是全局参数；任务控制是钱包作用域命令。
# 载入配置（绝对路径，父目录需为当前用户所有的私有目录）
baw-rs --registry <REG> --wallet <钱包名> task load --config /absolute/path/task.json
# 装载并开始监听（需确认）
baw-rs --registry <REG> --wallet <钱包名> task arm <task_id> --confirm
baw-rs --registry <REG> --wallet <钱包名> task status
baw-rs --registry <REG> --wallet <钱包名> task disarm <task_id>

# 兼容写法（隐藏子命令，语义完全相同）：
baw-rs vaults --registry <REG> task <钱包名> status
```

`route` 动作会自动追加一条**授权额度检查**：装载时生成 `allowance(user, expected_spender)` 的
`eth_call`，`arm` 时要求结果 ≥ 本次 `amount`，不足即装载失败并报出实际值。原因是实测（2026-11-02：
U 余额 20.39、授权 0，OKX 仍返回空的 `signatureData`）——响应里没有任何字段能告诉你"缺授权"，
只有链上读得到；没有这条检查，任务会照常 arm，然后在第一次真实事件上以通用的模拟错误报废。
用 `skip_allowance_check: true` 可关闭（例如输入币是原生 BNB，本来不需要授权，这种情况会自动跳过）。

额度不足时用同一条 CLI 的 `approvals approve` 补授权（calldata 本地拼、走 fire 同一条预览→执行核心），
不要临时换脚本或聚合器接口：

```sh
baw-rs --wallet <名> approvals approve --binanceChainId 56 --tokenContract <输入币> \
  --spender <OKX dexContractAddress> --amount <最小单位> --from <钱包地址> \
  --confirm --operation-id approve-$(date +%s) --json
```

授权是长期有效的链上状态，`--amount max`（无限授权）必须额外加 `--unlimited`；补完授权先用
`eth_call allowance` 核对再 `arm`，别看 `BROADCASTED` 就当成了。

`arm` 会检查会话、开发者模式、发送方和链，并执行显式 prerequisites；余额/gas/allowance 未配置就不能声称检查过。启用会预约该钱包，
避免日常查询占住执行通道。任务与钱包绑定；身份或会话变化即失效。

## 配置

```json
{
  "task_id": "sell-on-launch",
  "valid_for_ms": 1800000,
  "chain_id": "56",
  "rpc_url": "https://...",
  "ws_url": "wss://...",
  "filter": { "address": "0x合约", "topics": ["0x事件签名", null] },
  "start_block": 130000000,
  "confirmation_depth": 0,
  "max_backfill_blocks": 100,
  "chunk_blocks": 10,
  "queue_capacity": 4,
  "action": { "kind": "fixed", "call": { "binance_chain_id": "56", "from": "0x钱包", "to": "0x目标", "value": "0", "input_data": "0x", "gas_limit": 200000 } },
  "policy": { "version": 1, "chainId": "56", "to": "0x目标", "valueWei": "0" },
  "prerequisites": []
}
```

- `filter.topics` 的 `null` 表示该位任意；未命中的日志不触发。
- `confirmation_depth` 为 0 表示事件到达即可触发；大于 0 时等待对应确认数。
- `queue_capacity` 有上限；溢出时任务直接判 FAILED（**不静默丢事件**，也不会自动恢复）。
- `policy` 走现有策略解析器：目标、价值上限、selector、calldata 摘要、gas 上限、风险与授权变化许可。
  未显式给出的动态字段不会被放大。

### `words`：受控动态参数

```json
"action": { "kind": "words", "call": {"...": "selector 与静态部分"},
  "args": [
    { "source": {"kind": "topic", "index": 1}, "abi_type": "uint256", "max": "1000000" },
    { "source": {"kind": "data", "index": 0}, "abi_type": "address", "max": "0xffff" }
  ] }
```

- 只支持 `selector + 1..64` 个静态 ABI word，类型限 `uint256/address/bytes32/bool`。
- `source.kind` 可为 `topic`、`data`、`constant`、`read`（事件区块上的只读 `eth_call`，带 `requireCanonical`）。
- 可选的 `multiply`/`divide`/`add` 只接受整数，结果必须 `<= max`，否则整笔拒绝。
- **不支持的动态形式会明确报错**，不会退化成“随便透传”。

### `route`：装载时解析与空转演练

`route` 动作里两个由提供方决定的字段可以省略，`task load` 时自动补：

- `expected_spender`：OKX `/approve-transaction` 返回的 `dexContractAddress`（按链固定，全平台共用）
- `allowed_router_addresses`：当前报价的 `tx.to`

补完之后立刻用这次报价把整条构造+校验走一遍，**拿到的 calldata 直接丢弃**，只把结果写进装载响应的
`prepared` 字段（`to`、`selector`、`gasLimit`、`gas_quoted`/`gas_preview_cap`、`reported_minimum_output`、
`discovered`）。一次装载最多两次签字请求 + 一次报价，都发生在装载时，不占触发路径。

这么做的理由是一次性任务没有第二次机会：装不出来的东西，触发时同样装不出来，早失败远好过占用钱包两小时后
在真实事件上失败。装载时报错形如 `route discovery failed: …` 或 `route dry run failed: …`，会带 OKX 的
`code`/`msg`。

不想在装载时联网（例如用 mock origin 跑测试），在 route 配置里写 `"prepare_at_load": false`。

### `auto_approve`：启用时自动补授权

`route` 配置写 `"auto_approve": true` 后，`arm` 流程会：读一次链上 `allowance(user, spender)`；不足就发一笔
`approve(spender, amount)`（走和其它写操作同一条预览→执行核心），等回执、重新读额度，够了才进入 READY。

- **只在 arm 时补，不在触发时补**：授权是一整笔交易（数秒、gas、等确认），塞进一次性触发路径等于在计时窗口里
  多插一笔可能失败的操作。
- 授权额度 = 任务的 `amount`（不写死 max）；想长期免补用 `approvals approve --amount max --unlimited` 单独发。
- 补授权失败会在 arm 阶段落 `FAILED`（`submission_attempted:false`）并释放钱包，不会带着缺额度的状态进入监听。
- arm 返回里带 `autoApprove`：`{needed, txHash, allowanceBefore, allowanceAfter}`，便于对账。
- 默认关闭：它会花 gas 并写入永久链上状态，必须显式打开。

### `route`：事件后实时取路由

路由构造器把报价 gas 当**预览上限**使用：默认按 `max(报价×3, 报价+500000)`（`gas_buffer_percent=300`、
`gas_buffer_absolute=500000`）加余量，再被 `max_gas_limit` 与任务 `policy.maxGasLimit` 中较小者夹住。
`max_gas_limit` 可省略，省略即用内置上限 **5,000,000**。上限不是费用——链上只按 `gasUsed` 收费，
放宽上限不花钱，只减少"报价跳高就整单拒"的失败。实测同一笔 1 U→BNB 的报价 gas 在 310,620–1,013,123
之间跳，真实需求与报价的余量低到 0.2%。

校验以 OKX V6 文档（https://web3.okx.com/onchainos/dev-docs/trade/dex-swap）为准，只保留文档定义
且能挡住真实风险的项：`code`、单条 `data`、链 ID、`tx.from`、`tx.to` 白名单、
calldata 形状、`fromTokenAmount` 回显、`tx.value` 上限、`minReceiveAmount` 非零、`gas` 上限、
`slippagePercent` 回显、`signatureData` 非空即停。`gasPrice`/`maxFeePerGas`/`maxPriorityFeePerGas`
不再交叉校验（文档为参考值且不转发）；`minReceiveAmount` 不再用滑点重算下界（文档定义即为滑点极限
值）；`approveTransaction`/`approveAddress`/`approveContract` 不在文档 schema 内，一律忽略。
新增 `max_price_impact_percent`（默认 "2"）：文档要求提交前检查 `priceImpactPercent`，超过阈值即拒单；
字段缺失时记录但放行。

`route` 的动作在**事件命中之后**才发起请求，绝不会在装载或等待阶段预取报价或 calldata。
配置要求显式给出链、输入/输出代币、**固定数量**、滑点、钱包、期望授权方、允许的路由地址与 selector、
`value` 与 gas 上限、请求超时与重试上限、以及凭证文件路径（0600）。原生任务的action.config不填写`deadline_unix_ms`，填写会被拒绝；内部使用首次arm解析的同一任务截止点，触发时不重新给时长。独立路由构造器原接口不变。

返回结果必须通过服务端回显校验（链、钱包、代币、数量、滑点、授权方、gas 单位等），
任何缺失或矛盾都失败退出。**报价结果不等于链上最低收到量**；两者不会被混用。

对于无法从 calldata 直接证明收款方与最低收到量的路由（多数聚合器 calldata 属于此类），
结果会携带 `requires_runtime_preview=true`。**这类路由默认被拒绝**，因为收款方、代币路径和
链上最低收到量都没有被绑定。要接受它，必须在任务配置里显式写：

```json
"allow_unbound_route_calldata": true
```

这只表示你知情并接受“这三项没有本地校验，只能依赖模拟与策略兜底”，**不等于**引擎验证过它们。
代码与文档都不会把这种路由称作“已完全验证”。如果路由声称 `requires_runtime_preview=false`，
则必须同时带有 `recipient_verified` 与 `calldata_min_out_verified`，否则任务失败。

## 旧状态迁移

升级读取旧`native-event-state.json`时，独立解码旧格式并原子迁移，不删除历史或给旧任务新时长。旧完整记录存入同文件`legacy_records[task_id]`（原配置/状态/操作编号/截止点/证据）；当前旧记录成为不可启用的墓碑：valid_for_ms为0仅用于历史内部状态，用户输入0仍拒绝。旧ACCEPTED/EXECUTING保守恢复UNKNOWN，旧等待/LOADED恢复DISARMED，终态保留。旧ID不能重用，新唯一ID相对时长任务可装载。迁移先安全写盘再发布内存，失败不能装作成功；证据扩张超过256KiB上限时明确拒绝迁移且不丢原文件，需要人工对账处置。测试只使用合成文件，未改真实钱包状态。

## 安全与失败语义

- 事件去重键绑定任务、链、`blockHash`、`txHash`、`logIndex`；重连或重启后不会为同一事件重复下单。
- `removed` 日志不会被当作触发，也不会让任务失败，收到后继续监听。
- 断线自动重订阅，并按块高做有界补读（`chunk_blocks` / `max_backfill_blocks`）；连续 5 次重连失败后任务判 FAILED，不会无限重试。
- 过滤或 RPC 出错一律 fail-closed，任务进入故障状态并停止接单，而不是“当作没发生”。
- WS 上**不是本订阅通知**的帧（别的订阅、RPC 错误对象、节点通知）只记录、不终止：前 20 条写入
  stderr（常驻服务的 `serve.log`），之后静默忽略。无法解析的帧和关闭/传输错误按协议故障处理，
  走重连与补读，不再把整条 24 小时监听判死。
- 非成功终态会把**将要提交的那笔调用**落盘：`call.binance_chain_id/from/to/value/gas_limit/input_data`。
  预览被拒时可以据此复现，而不是只剩一个错误码。
- 提交前失败可显式记录并停止；**已提交但结果未知不会自动重发，也不会重新取一条新路由再试**。
- 后端错误文本保留原因（如“The transaction ran out of gas…”），长串疑似凭证的字符被替换为
  `<redacted>` 并截断到 200 字符：`Binance API code 351805: <原因>`。
- 任务默认一次性执行；完成后保留终态。状态区分 `SUCCESS / FAILED / UNKNOWN`，
  `SUCCESS` 的含义是**后端已受理**，不是链上确认。
- 已受理的交易不会因为客户端断开而取消；优雅停止只排空已受理操作，不再接受新的触发。

## 计时

结果包含基于单调时钟的分段：事件到开始构造、动作构造耗时、构造到发起的间隙、事件到结果。
HTTP 等待单独统计，不混入本地处理。**这些是本地与端到端观测值，不是对延迟的承诺。**

## 当前边界

- 只有 EVM；不支持 Solana。
- `route` 只内置 OKX V6；其他路由需要新增适配器。
- `words` 不支持动态数组、嵌套元组或任意 ABI 解码。
- 旧版日志任务已实跑；本次相对时长/pending版本尚未部署，使用本地合成夹具验收，无新增真钱测试。
