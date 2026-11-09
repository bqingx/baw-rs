# 实例级 Task / 流水线

当前源码能力，不代表选定二进制已经部署或经过实盘验收。先检查 `pipeline --help`。

```sh
"$B" pipeline load --config "$PRIVATE_CONFIG"
"$B" pipeline status "$PIPELINE_ID"
# 只有用户明确授权全部钱包、金额、目标、滑点及步骤后：
"$B" pipeline arm "$PIPELINE_ID" --confirm
"$B" pipeline cancel "$PIPELINE_ID"
# 故障停止后先只读对账，再确认继续：
"$B" pipeline reconcile "$PIPELINE_ID"
"$B" pipeline resume "$PIPELINE_ID" --confirm
```

- 以上入口不使用 `--wallet`。JSON 定义的 steps 是 task，每个 actions 元素有 wallet 和钱包相对 argv。
- 允许 task1(w1,w2) → task2(w2,w3) → task3(w3,w4)。每个步骤不同钱包并行，步骤间自动推进。
- pipeline describe --config /absolute/private/task.json 只读完整定义并展示各步触发/有效时长/确认条件/意图，不load、不arm、不连RPC、不读私密profile/policy、不创建期限或preview digest。摘要不代替arm --confirm；合约只展示声明from/to/value/gas，不验证实际钱包绑定或输出calldata。
- wallets overview 默认本地快照，可查看Task阶段/截止/未知操作，不能把session文件存在或journal有hash当成交。显式远程刷新与执行准入互斥，Arm/Resume报overview busy时原授权/截止未消耗，只能用户明确重试。
- `completion` 默认 confirmed：全部成员链上成功才推进；submitted 需显式选择且必须有有效 32 字节交易 hash，不代表成交；只有 orderId 仍暂停，不推进后继。
- arm 在监听前准备当前步骤的兑换额度，显式 --auto-approve 允许不足时立即精确授权并花 Gas，不再只是本地预加载。授权只等首个成功回执，不等额外两确认或复读额度；准备未知则不启动监听。
- actions 目前仅支持 contract-call fire 或 OKX market-order swap，不能自行携带 confirm/operation-id/钱包选择器/skip-prechecks。
- trigger 支持 immediate、time(unix_ms)、block(number)、event(address/topics/from_block)。后两者使用共享 WS 上的 newHeads/logs 订阅推送，带初始 head/有界日志回补（总计最多 500 块，每请求最多 10 块，兼容 Alchemy 免费档）；分页间检查取消/到期并保留实时通知，失败不自动重试或另开连接。不是 pending 监听，尚未实测生产最低触发延迟。
- 每步额度准备最多 120 秒；valid_for_ms 从准备完成、启动监听起算，两种截止均持久化且不续期。已有监听截止也约束准备请求。到期不发起新请求，已准入的交易仍可能上链。
- results.phase=WAITING 才表示监听就绪；RUNNING 可能是 PREPARING/STARTING_LISTENER。触发后不查额度、不授权、不等待授权；额度被消耗、配置变更或准备不匹配时停止，不现场修复或重试。
- 准备与兑换使用独立 a-/p- operation ID；准备中断、未知或缺就绪记录不盲目重放；已开始准备却无 ready proof，resume 控制层直接拒绝，不补授权。取消/未命中不自动撤销已成功授权，不能退回授权 Gas；下一步在自己的监听前准备。
- 失败、未知、PENDING_CONFIRMATION、缺 txHash 会暂停，不能改 ID 盲目重跑。重启后 INTERRUPTED/InFlight→Uncertain，不自动重发。reconcile 仅凭执行结果或本钱包日志中的 txHash 读取规范回执；不以“查不到”为未提交证明。resume 必须确认，仅执行 Pending，不重发已提交/确认动作，不续期；未知/失败未解决、钱包身份变化、已取消或旧状态无截止记录时拒绝。
- rpc_url 是私有 WSS 地址，实例只允许一个活跃 endpoint；无需求不联网，最后一个租约释放断开。OKX 自动授权共用此连接，配置端点必须一致。
- 固定动作在 arm/resume 编译为内存计划。新 OKX Task 监听前检查状态并保存 walletPolicy；触发后只做 OKX swap/币安 Preview/Execute 三次核心 HTTP，辅助读取即使命中缓存也拒绝。后台拒绝、App 确认或未知结果暂停、不刷新/重发。原截止/取消、本地身份/配置、模式原到期、模拟/防重仍保留，旧记录缺字段不自动升级。WSS 事件/回执另算。
- 准备及其他动作的状态读集仍最多 3 秒，0 TTL 禁用缓存，不会重新打开三调用模式的辅助查询；不后台高频刷新。OKX HTTP pool 跨准备/触发复用，路由/模拟现取，不能保证连接永不关闭。
- --okx-min-output-units 是可选额外固定floor：不传时触发新报价/route仍要求reported>0、满足当次滑点上下界，并验证模拟净到账>=reported；不冻结准备期报价、不默认0/1wei。显式floor和缺省仍在完整意图/proof中区分，已有Task的floor/授权/原截止不改。省略floor不表示能沿用过期或未知任务。
- 可选通知默认关闭，只有显式私有 PushPlus 配置/渠道才启用；终态成功保存后独立处理，不加入三核心调用。未知/App/失败/过期与 mixed 分开，submitted 完成不冒充链上成功。持久 at-most-once 尝试，不重试/启动补发；服务端受理不是送达，丢通知不能触发重新交易。wallets status 可看不含密钥的 enabled/队列丢弃计数。
- stateReadStats 是请求/缓存计数，hotPathTiming/walletExecuteMs 是有明确起止范围的本地耗时，不能当作完整事件到上链延迟。from_block 仍是显式历史回补锚点，准备阶段不会自动改为当前块。
- 旧钱包级任务、日志/pending 引擎、typed prepare/trigger/disarm/native IPC 已删除。task 只是 pipeline alias，不带 --wallet；单钱包目录业务访问仍兼容，但没有旧任务执行器。不要自动迁移/重放旧状态或擅自重启现役服务。
- MEV/private broadcast 与 Gas 价格控制尚未实现；contract-call 只有 gasLimit，不能把它当 gasPrice 或费用档位。

完整配置与限制以仓库 `docs/PIPELINES.md` 为准。任务文件、RPC Key、OKX 凭据、会话不得提交到仓库。取消不会撤回已经提交的交易。
