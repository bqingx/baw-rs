# 实例级多钱包 Task / 流水线（开发中）

> 本页描述当前源码，不代表已部署或真实资金验收。用户已接受当前外部合约调用接口未暴露 MEV / Gas 价格控制的限制；保留明确拒绝，不将其伪装为已支持。

## 控制入口

```sh
baw-rs wallets serve
baw-rs pipeline load --config /absolute/private/pipeline.json
baw-rs pipeline status example
# 只有明确授权配置中全部钱包、全部动作后才启用：
baw-rs pipeline arm example --confirm
baw-rs pipeline cancel example
# 故障停止后，先只读对账，再明确确认续跑：
baw-rs pipeline reconcile example
baw-rs pipeline resume example --confirm
```

`pipeline` 不带 `--wallet`；`--registry` 选择实例。每个 step 是一个 task，actions 中指定钱包。流水线可以提前 load。arm 并行预加载成员会话，并在当前步骤开始监听前准备 OKX 兑换额度：仅显式 --auto-approve 才可在不足时提交精确授权。因此 arm --confirm 可能立即花费授权 Gas，不再只是本地预加载。准备完成后释放钱包执行队列，再开始等待触发；不同钱包并行准备。

旧钱包级任务、日志/pending 引擎和 typed executor 已从源码删除，旧 prepare/trigger/disarm/native IPC 直接拒绝。`task` 现在只是实例级 `pipeline` 的别名；两者都不能带 --wallet。旧配置不能原样装入新流水线，不自动迁移、删除状态或重放动作。普通单钱包目录登录/业务访问仍兼容，不包含旧任务引擎。

## 配置

父目录须为本用户 0700，文件为本用户普通单硬链接 0600。参考 `examples/pipeline-contract.json`，其中地址/RPC 是占位符，不能用于实盘。

```json
{
  "id": "example",
  "chain_id": "56",
  "rpc_url": "wss://YOUR_CHAIN.g.alchemy.com/v2/YOUR_KEY",
  "steps": [
    {
      "task_id": "task1",
      "trigger": {"kind": "immediate"},
      "valid_for_ms": 60000,
      "completion": "confirmed",
      "confirmations": 1,
      "actions": [
        {"wallet": "w1", "argv": ["contract-call", "fire", "--binanceChainId", "56", "--from", "0x1111111111111111111111111111111111111111", "--to", "0x2222222222222222222222222222222222222222", "--value", "0", "--inputData", "0x12345678"]}
      ]
    }
  ]
}
```

- 每个 task 支持 1..16 个不同钱包；流水线 1..256 个 task。
- 同一钱包可重复出现在不同 task；不同钱包在同一 task 内并行。
- actions 目前接受 `contract-call fire` 和 `market-order swap`（仅 OKX）。不能注入登录、钱包选择器、跳过检查或自选 operation ID。
- operation ID 由 pipeline ID / 步号 / 钱包确定性生成；同 ID 不覆盖或重新启用，保留执行历史。
- `valid_for_ms` 是正整数，从该步骤额度准备完成、启动监听时起算，覆盖监听建立/回补、触发等待、执行与回执，不包含之前的额度准备。准备阶段有独立最多 120 秒的固定预算（不是固定等待），两种截止时间均持久化，恢复不续期；若已有监听截止，准备请求也不得越过它。下一步有自己的预算，时钟校准仍依赖主机时间。
- `completion` 默认 `confirmed`：全部成员链上成功且满足确认数，才进入下一步。
- 显式 `submitted` 必须有有效 32 字节交易 hash 的提交证据才允许继续，不保证链上成功；单独 orderId 或无效/缺失 hash 仍暂停未知结果，不允许借订单号推进。不要误认为流水线是原子交易。
- 返回 `PENDING_CONFIRMATION`、未知结果、失败或到期会暂停；后台只返回 order ID 而没有 txHash 时，confirmed 模式暂停，不能猜测已上链。
- 同钱包的并发流水线动作沿用钱包的串行执行锁；不同钱包不共享该锁。目前没有跨流水线的优先级/抢占策略。

## 监听前额度准备与触发快路径

- 当前步骤的 OKX swap 在监听前读取一次额度；不足且明确 --auto-approve 才精确授权本次数量。未允许自动授权且额度不足时停止，不开启监听。
- 授权只等待首次绑定的成功回执，不额外等待配置中的两确认深度、不重读额度或规范块。单次出块不等于最终性，重组或外部操作可能使准备额度失效。
- 准备不调用 swap，不冻结可过期的报价/calldata；就绪记录绑定钱包发送方、完整交易参数和私有配置指纹。
- 触发后只走实际兑换准备/模拟/提交及必要资金校验，不查询额度、不调用 approve 接口、不等待授权。准备后额度被其他操作消耗或配置变化时停止/暂停，不临场补授权或重发。
- 状态 RUNNING 不代表监听已就绪。查看 results.phase：PREPARING → STARTING_LISTENER → WAITING → EXECUTING；WAITING 才表示准备和监听建立/回补已完成。
- 每一步在自己的监听前准备，不提前执行未来步骤或合并跨步骤额度。准备失败/未知不启动监听；已准入的准备请求仍 drain。准备和兑换使用不同确定性 operation ID（a-/p-），准备中断不会盲目重放。
- 准备状态未知或尚无监听截止就中断的任务，不自动开启新的监听窗口；需只读核验并明确安排新的任务。取消/未触发不自动撤销已成功的授权，授权 Gas 也不能退回。
- 上述快路径仅用于实例 Task；普通独立 market-order swap 保留原有按需授权、确认深度和额度复核策略。成交后的 Task 完成确认仍按 completion/confirmations 执行，与触发前的授权等待不同。

## 执行计划与状态读取

arm/resume 预编译固定动作和准备命令，绑定钱包对象；触发后不解析 CLI argv，恢复需重新编译且不续期。计划不写入状态文件，额度仍按每个步骤单独准备，不是全流水线提前合并授权。

新 OKX Task 把设置/额度/锁/发送方检查放在准备阶段，触发后仅允许 OKX `/swap` + 币安 Preview + Execute 三次核心 HTTP；嵌套 SDK 也不得补查或读缓存。后台拒绝、App 确认或结果未知时暂停，不刷新/重发。原任务截止/取消、本地绑定、模式原到期时间及模拟/防重仍检查。WSS 事件规范性和回执另算，详见 [THREE-CALL.md](THREE-CALL.md)。

准备阶段与其他动作仍使用最多 3 秒的命令级只读去重，0 TTL 禁用缓存；会话变化、缓存到期及身份/取消检查不被忽略。该开关不会重新启用新三调用模式的状态查询。不在等待时高频后台轮询。OKX HTTP pool 跨准备与触发复用，路由现取、模拟现做。

结果提供 stateReadStats、walletExecuteMs、hotPathTiming；字段范围与限制见 [HOT-PATH.md](HOT-PATH.md)。它们不是完整事件到上链时延。触发标记与动作 InFlight 合并落盘，但任何执行都必须在该持久 admission 之后。

## 触发条件

```json
{"kind":"immediate"}
{"kind":"time","unix_ms":1793869200000}
{"kind":"block","number":12345678}
{"kind":"event","address":"0x2222222222222222222222222222222222222222","topics":["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"from_block":12345000}
```

区块/事件使用共享 WS 上的 `eth_subscribe newHeads/logs` 推送；相同订阅引用计数复用，不同订阅共用同一 socket。订阅建立后读取一次初始 head，日志执行一次有界回补，避免建立订阅期间漏掉事件；不是循环查询 head。未做生产延迟基准，不能承诺狙击级最低延迟。事件为已出块日志，按地址及 1..4 个精确 topics 匹配；不是 pending 交易监听。总回补最多 500 块，每次 eth_getLogs 最多 10 块以兼容 Alchemy 免费档；分页间检查取消/期限，实时通知保留在有界订阅缓冲中，不重试失败页或另开连接。拒绝页范围外日志。日志和回执校验规范链块哈希，回执另核对钱包发送方。from_block 仍是显式回补起点，不会因为额度准备而自动重设；可能回补准备期间的匹配事件。若只希望手动发起就绪后的新事件，等 WAITING 再转出并明确起始区块。触发后仍存在正常链重组风险，不提供绝对不可回滚保证。

## 单连接与额度

- named daemon 的新任务与普通 OKX 自动授权共用一个 RPC 管理器；只使用 WSS，没有 HTTP 回退。
- 实例同时只接受一个 endpoint；冲突明确报错，不另开连接。不支持同时监听不同链端点。
- acquire 不联网；第一次请求才建立连接，最后一个租约释放后关闭。
- 同一任务触发/自动授权/回执期间复用连接；相同短时请求合并，串行 RPC 请求限速为最多约 4 次/秒，订阅通知不经过此限流等待。这不是 Alchemy CU 配额保证。
- 最后一个订阅者离开时取消该服务端订阅；通知缓冲溢出、畸形消息、订阅失败会暂停，不静默丢弃后继续。
- OKX 私有配置的 HTTPS rpc_url 在 named daemon 中映射为同地址 WSS，必须与流水线 endpoint 一致且服务端支持；否则拒绝，不切回 HTTP。
- 断线/协议错误会暂停任务；当前没有自动重连回补，避免静默错过触发。RPC 拒绝显示失败方法和数字错误码，已知的 10 块限制使用固定提示，不透传可能带密钥的服务端原文。
- 新任务只在 named daemon 的实例调度器执行，不再存在独立单钱包监听器。普通目录形式的单钱包业务访问不提供 pipeline 控制入口。

## 持久化与取消

状态保存到 registry 下的 `pipeline-<id>.json`，0600、写临时文件 → fsync → rename → 目录 fsync。动作在提交前持久标记 InFlight。

崩溃后 InFlight 转为 Uncertain，运行中的流水线转为 INTERRUPTED，不自动重发。

- `pipeline reconcile ID`：只读核验当前未完成 task；从执行结果或本钱包确定性 operation ID 的日志恢复 txHash，核对链、发送方、规范块和确认数。每个已验证结果立即持久化。不会发交易或把“未查到回执”当成“没提交”。
- `pipeline resume ID --confirm`：只执行 Pending 成员；Submitted/Confirmed 不重发，Uncertain/InFlight/Failed 未解决时拒绝。已开始授权准备却缺 ready proof 时也在控制层立即拒绝，不会先启动再补授权。已触发步骤不再次等待同一触发，未开始的后续步骤仍遵守自己的触发条件。
- 每个已开始步骤的原始截止时间持久化，resume 不续期；过期步骤即使还有 Pending 钱包也拒绝继续。新步骤第一次开始才获得自己的时长预算。
- arm 绑定钱包目录、会话与 identity 的文件指纹；每次后端请求前复核，resume 也核对。重新登录或换 identity 后不能沿用旧授权执行。
- CANCELLED 不可恢复；旧版本没有持久截止时间/身份绑定的中断状态不可 resume。
- 没有 txHash、只有 order ID，或回执不足以证明结果时，保持未解决，不接受人为 success 标志或盲目重发。本版尚无 order ID → txHash 自动查找。

断线后不会自行重连和推进；需主动 reconcile/resume。未知且无 hash 的动作不能因为主动 resume 就重发。离线回复丢失/超时/真实 SIGKILL 重启验收见 [SINGLE-WALLET-FAULTS.md](SINGLE-WALLET-FAULTS.md)，不等于真实链上验收。

取消会阻止后续请求/步骤，但不能撤回已提交的交易。已接纳的请求会 drain，不因为另一钱包失败就取消网络请求。每次币安后台请求前检查取消/截止时间，已经发送的请求不因到期而重试。

## 仍待完成

1. 可验证的断线恢复和回补；当前断线暂停而不是自动重连。
2. order ID 自动查找 txHash、跨流水线优先级和更多故障注入。
3. 生产链端到端延迟、真实 Alchemy 额度与部署验收；当前本地记账基准见 [PERFORMANCE.md](PERFORMANCE.md)，不能代替实盘验证。

旧任务/typed executor 清理已完成，过时文档已改为迁移提示；普通登录、查询、签名、多钱包隔离和目录形式兼容测试保留。
