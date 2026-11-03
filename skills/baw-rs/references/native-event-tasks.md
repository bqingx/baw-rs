# 原生任务：时长与日志/pending触发

源码：`src/native_events.rs`、`src/native_pending.rs`、`src/executor.rs`、`src/route_action.rs`。旧日志版已部署实跑；本次时长/pending新增功能尚未切换生产，最终门禁与审查结果以交付报告为准。版本号0.3.0不足以区分构建，先核对运行二进制。

## 控制入口

```sh
baw-rs --wallet NAME task load --config /绝对路径/task.json
baw-rs --wallet NAME task arm TASK_ID --confirm
baw-rs --wallet NAME task status --json
baw-rs --wallet NAME task disarm TASK_ID
```

load只装载；arm启用后由既有多钱包后台监听、匹配、构造并执行，临时客户端退出不影响任务，不另起每钱包脚本。CONNECTING不是READY，准备与补读也可能先触发。配置父目录0700、文件0600、普通文件单硬链接。

## 必填相对有效期

新配置必须有`valid_for_ms`正整数毫秒，例如1800000代表30分钟。没有固定5分钟上限；遗漏/零/负数/浮点/字符串/溢出拒绝。用户输入`deadline_ms`单独或混用均拒绝。

首次有效arm尝试准备前开始计时，load等待不计时。内部一次计算绝对及单调截止点，准备、监听、动作构造和提交前检查共同受限。`resolved_deadline_ms`仅状态展示，不能输入续期。重复arm/disarm后arm/断线/恢复不得刷新期限。已启用任务重启不自动恢复；未启用LOADED才可首次arm。

等待到期或撤销释放钱包；ACCEPTED/EXECUTING不能用disarm承诺取消。execute POST前到期必须零execute；提交后到期不撤回交易、不抹掉后续结果。UNKNOWN不重发。长时等待不延长币安会话或开发者模式，需执行前复核。

新版迁移实现在同文件 `legacy_records` 保存完整原记录、current变为不可arm的历史墓碑（内部valid_for_ms=0，resolved_deadline_ms保留旧值）；旧ACCEPTED/EXECUTING保守UNKNOWN，旧等待/LOADED撤销，终态保留。保留全部history与旧ID，允许新唯一ID任务；迁移超256KiB不丢证据而明确拒绝，先成功持久化再发布内存。最终可用性仍以全量门禁与独立复审为准。

更改输入结构前先取旧版状态结构制作私有合成夹具，区分“拒绝用户旧配置”和“读取历史磁盘状态”两条路径。对旧SUCCESS、等待中、ACCEPTED、EXECUTING分别验证Status/Disarm/装载新唯一ID及再次恢复；不能让不可解析的旧current永久挡住所有控制入口。迁移应原子保存已用ID、operation_id、原deadline、history和原始证据，旧任务不可获得新时长或重新arm。写入失败不得宣称迁移完成、丢掉未知结果或清防重历史。不要修改真实钱包状态来演示；只有完整测试和独立复审通过后才报告迁移已可用。

## 三种触发来源

`source`省略默认`{"kind":"logs"}`。日志模式仍要求显式filter/start_block/confirmation_depth/max_backfill_blocks/chunk_blocks。topics仅固定值/null；不是任意data比较或价格复合脚本。start_block含该块，过早会触发历史事件。ERC20 Transfer由代币合约发出，钱包只是from/to筛选值，转出/收到必须明确。confirmation_depth=0也是已出块日志，不是mempool。removed:true忽略；断线按原有界补块，超界失败。

pending示例（占位地址，不能直接交易）：
```json
{"kind":"pending","subscription":"hashes","from":"0x1111111111111111111111111111111111111111","to":"0x2222222222222222222222222222222222222222","selector":"0x12345678","args":[]}
```

固定from/to/4字节selector，args必须显式数组，可为空。参数对象为`{"index":0,"abi_type":"bool","equals":"0x0000000000000000000000000000000000000000000000000000000000000001"}`，index从方法选择器后第0个32字节槽位起，类型uint256/address/bytes32/bool，仅精确完整ABI字匹配，不猜动态偏移。

pending省略日志filter和block/confirmation/backfill字段，显式null/0仅表示无日志语义。hashes订阅`newPendingTransactions`再HTTP查询真实详情；full订阅`["newPendingTransactions",true]`需供应商扩展支持，不自动切模式。检查WS/HTTP链ID，hash详情必须同hash，null或已出块跳过，字段畸形失败。只看到节点自己的mempool，HTTP/WS节点池不同可能查不到。

有界队列、滑动近期去重（日志与 pending 共用同一机制），不为每 hash 无限并发取详情；队列溢出失败，不静默丢。去重满窗口只淘汰最早记录，**不得因为累计条数把长任务判失败**——否则长有效期被隐形寿命抵消。pending 只按配置的参数位比完整32字节，选择器命中但带动态尾部/缺槽位只算不匹配，不让单条候选畸形终止整个任务。pending断线有不可补齐的缺口，明确失败，无重连/日志回退/重放。目标交易取消、替换或失败不撤回自己已提交的抢单。预览可能因当前链上未开放失败，不能绕过或承诺首块/排序。

### 多钱包共享一个任务

配置里写 `"wallets": ["w1","w2","w3"]`，同一个 task 就占用这几个钱包：

```json
{"task_id":"free-mint-1","valid_for_ms":1800000,"chain_id":"56",
 "rpc_url":"...","ws_url":"...","wallets":["w1","w2","w3"],
 "source":{"kind":"block_time","timestamp_gte":1793698200},
 "action":{"kind":"fixed","call":{"from":"","to":"0x…","value":"0","input_data":"0x…","gas_limit":200000}},
 "policy":{...},"queue_capacity":8}
```

- `load` 时 daemon 给每个成员查出自己在该链的地址并写入 `call.from`（route 动作写 `user_wallet_address`），**其余字段原样保留**，所以授权范围不变。
- 成员共享**一条** WS 和一路 HTTP 轮询（按数据源指纹合并）；每个钱包仍有独立队列、独立防重、独立结果。
- `arm`/`status`/`disarm` 都按组执行；`status` 返回每个钱包的独立状态。
- 只列出的钱包会被操作；控制钱包必须在列表内，否则拒绝。
- **失败即整组回退**：装载或启用过程中任一成员失败，已成功的那几个会被撤销，避免“看起来整组已就绪，实际只有部分会发”。
- 单个钱包路径收到 `wallets` 非空配置会直接报错，不会静默忽略同伴。

### `block_time`：按链上区块时间触发

```json
{"kind":"block_time","timestamp_gte":1793698200}
```

用于“链上时间到点才允许”的动作，例如合约里 `require(block.timestamp >= startTime)` 的免费领取。**不用本机时钟**：本机早一秒会回滚，晚一秒可能被别人领走，只有区块自身的 `timestamp` 算数。

- 只接受 `timestamp_gte`（必须为正整数，Unix 秒）；出现 `filter`/`start_block`/`confirmation_depth`/`max_backfill_blocks`/`chunk_blocks` 任一字段即拒绝——确认深度与“尽快提交”直接冲突，静默忽略会造成误解。
- 两个读取路径：WS 订阅 `newHeads`（快路径），以及每 400ms 一次 `eth_getBlockByNumber("latest")`（补查）。两条路径统一走同一匹配与去重；WS 不支持或连接失败时**继续用 HTTP 轮询**，不因此判任务失败。
- 触发条件是链上 `timestamp >= timestamp_gte`；同一个块重复出现（WS+HTTP、重连重放）由块哈希去重，执行权仍由 operation 状态机保证只有一次。
- words 只允许 `constant` 和事件块 `read`（读该触发块的状态）；`input`/`topic`/`data` 需要 pending 或日志载荷，配置期即拒绝。
- arm 时已过阈值：立即用当前链头评估一次，命中即正常走预览→执行。
- 必须说清：看到达标区块时那个块已经封了，本能力提交的是**下一个块**。它去掉的是“等别人先领”和本机时钟漂移，**不保证抢到第一份**，也不保证进第一个达标块。要进首块只能在阈值前提前提交，代价是可能落到未达标块而回滚白付 Gas。

## 动作

fixed固定call，不是提前签好；words仅静态ABI、受限整数算术、上限与规范类型检查；route事件后实时OKX取路由，固定数量，不提前报价、不自动全仓。

日志words允许topic/data/constant/事件区块read。pending words只允许真实input静态参数/constant，拒绝topic/data/read；日志拒绝input。pending交易不是日志，不合成topics/data或事件区块。

arm检查会话/开发者模式/发送方/链及显式prerequisites；未配置余额/gas/allowance检查不能称全部查过。不透明路由默认拒绝，显式opt-in只表示接受未独立解码限制；不得擅自放宽allowAuthorityChanges。

相对时长必须穿透到每种动作，而不只是顶层解析：native route 的 action.config **禁止**出现嵌套 `deadline_unix_ms`（含 null 与混用），load 只做静态字段校验；事件发生时用首次 arm 解析的同一绝对与单调期限构造路由，不在触发时重算时长。独立 `RouteActionBuilder` 的绝对期限接口保持不变，只在 native 适配层注入。回归需覆盖：仅时长的 route 可装载、嵌套绝对期限被拒、load 后延迟首次 arm 不消耗时长、路由请求中到期零 execute、提交后到期仍保留结果。以上是源码契约，是否生效按运行二进制核对，不用测试通过代替部署确认。

## 状态、恢复与结果

LOADED→PREPARING→CONNECTING→READY→ACCEPTED→EXECUTING→SUCCESS/FAILED/UNKNOWN；等待撤销DISARMED。每钱包最多一个任务，预约阻挡该钱包普通命令，status/disarm独立控制，其他钱包不受预约影响。

SUCCESS仅后端受理，必须receipt/资产对账才称成交。明确未提交才FAILED，可能提交但结果不明为UNKNOWN、不重发。新状态机在提交前持久化EXECUTING，边界崩溃保守UNKNOWN；只有证明新ACCEPTED尚未越过持久化提交闸门，才能恢复为未提交失败。旧版若仅在内存记录提交事实，其ACCEPTED不能证明未发送，迁移要保守UNKNOWN。重启等待/准备任务DISARMED，不自动重新授权。旧worker排空、task_id归属检查防覆盖新任务。状态持久化失败要释放预约并明确错误，但**终态写盘失败只能把 `persistence_failed` 与错误合并进原 detail**：已提交（SUCCESS/UNKNOWN）的任务不得被改写成 `submission_attempted:false`，也不得丢 result/route/metrics 与 `retry: never; reconcile`，否则会被当成“没发、可重试”。写临时状态文件失败要在 write/sync/rename 三条路径上删除 `.native-event-*` 临时文件，且不得删掉原有好文件。

仅本地回环WS/HTTP合成测试可用于本次开发验收；不准为验证从任何钱包往监听地址造事件。测试通过、构建完成、部署成功、链上成交四件事分开报告。性能分段实测，不以事件到受理当端到端成交或生产保证。
