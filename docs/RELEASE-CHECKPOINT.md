# Release CLI 本机检查点：可选固定最低到账

## 新产物与仍在运行的旧产物

- `target/release/baw-rs`，`baw-rs 0.3.0`，**9,402,320 bytes**（约 8.97 MiB）。
- 新 SHA256：`6ae071bebb8d2fb37d59be4ca79aa9cc09bc317ac68e223d911a48f55da725f8`。本轮仅离线验证/构建，不切后台、不新增授权/交易。
- 已按此前授权部署的是 a151a367… immutable artifact，后台 PID 135727，六钱包 CONNECTED；只读总览/摘要与 PushPlus APP 送达/去重已经用户确认。证据在仓库外 registry/overview-intent-acceptance.json。之前的 dec3e144…/b8a49e… 是历史阶段。本轮 6ae071be… 未部署，未改真实配置、身份/会话或 Task。
- 此前 bsc-usdt-ten-percent-1794038113229 已按独立授权完成：1 USDT→BNB、滑点10%、固定floor 0.00129 BNB，Task confirmed；实际扣款1 USDT、剩余授权0、本地事件接收→动作返回1044.427ms，三核心HTTP。私有 tasks/<ID>-{verification,native-verification}.json 保留；不是新可选floor功能的实盘验收，也不是未来交易许可。
- 前一 f7050cfa… 产物曾按用户授权切至 PID 75994，身份/会话和历史任务未变；该历史切换不是本次统一 calldata 策略的部署。
- 上一连接观测产物 `027b996b…` 曾按用户授权启动测试后台 PID 39475 并完成真实事件兑换；不是新事件时序字段的实盘结果。
- 之前真实 949.95 ms 样本对应的是 `71801ce9c4cd96637e975df3b6cd9df82f7e767e5b5d53f6b0fa671482b7a2b4`，不能冒充本次观测版的成绩。
- Linux x86-64 stripped PIE；优化/Thin LTO/strip，单构建 job。
- 未安装 PATH。原测试进程已退出、旧任务均完成或取消；保留身份/加密会话文件，其启动前后 SHA256 一致，启动新测试进程。历史 PID 108481/3801 不作为当前存活证据。
- main 工作区未提交；没有 commit/push、生产服务安装或会话迁移/删除。

## 本轮变化

固定最低到账改可选：省略时按触发/执行时 fresh OKX reported minimum、当前quote/slippage界限及Preview净到账约束，reported仍必须存在/合法/>0，不默认0/1wei，不冻结准备报价；显式Some仍在授权前及提交前严格检查。完整Option参数继续进入durable/prepared binding，None/Some/数值不能互换复用proof；现有显式Task和原期限不变。更新意图readiness与minimumOutputPolicy诊断。见 OPTIONAL-MINIMUM.md。本轮只离线构建，不部署/交易。

## 前轮状态总览与意图摘要

新增 wallets overview 本地白名单快照和显式 --refresh，独立临时 Client 最多 login/settings 两 GET，不持久化会话或改财务缓存。共享 inspection/execution 准入与钱包互斥防止手动刷新和 Arm/Resume/同钱包执行交错；活动/未知Task、忙/未加载钱包跳过。新 market-order intent 与 pipeline describe 纯本地描述，不读私密profile/policy，不load/arm、不授予期限/preview digest；精确units与税率/滑点/floor、真实MEV/Gas限制、非法fire payload/损坏journal边界均覆盖。见 INSPECTION.md。本轮不部署、不访问真实钱包/推送/交易。

## 前轮在线接入与通知

修复在线新增被长 registry.lock 阻止的问题：短事务锁与 lifecycle 移除屏障分离，新增仍检查目标目录租约；真实 CLI/IPC 离线测试验证 PID 不变热加载，删除、重绑定、重复目录、跨 registry 抢占仍拒绝。新 CLI/daemon 锁协议须配套更新。见 HOT-ADD.md。

新增可选 PushPlus 任务汇总：默认关闭，私有绝对路径配置、渠道显式 app/wechat；终态持久化后走独立有界队列/HTTP，不读财务热路径辅助状态。持久 at-most-once 尝试、不重试/重定向，不冒充手机送达；只发 typed 状态/槽位/计数，过滤 raw error/URL/calldata/凭据。队列溢出只原子计数，不同步写 stderr；mixed 状态不被 App 提示掩盖。wallets status 提供无密钥 enabled/丢弃计数。见 TASK-NOTIFICATIONS.md。全部仅离线，没有读取真实 token 或推送。

## 前轮单钱包故障验收

完成单钱包 loopback 故障测试：Execute 回复丢失/生产客户端超时、授权回复丢失、App 确认/拒绝，以及真正 SIGKILL 独立子进程→新 OS 进程读盘→WSS 只读对账/禁止重放。发现并修复 orderId-only 错当 submitted 完成（现在必须有效 hash，独立 swap 为 UNCERTAIN），以及未知准备在 resume 控制层未提前拒绝的问题。原 deadline/模拟身份/session 不变。见 SINGLE-WALLET-FAULTS.md；不是实盘故障或延迟保证。本轮不部署、不触碰真实钱包。

## 前轮清理

删除无效 opaque 参数、四个旧 route 配置字段、隐藏 register/run、market-order list/ListArgs、Binance 程序枚举与旧拒绝 stub、重复全局 precheck setter。已有目录显式接入合并至 wallets add，不改写身份/session；保留当前冷/暖访问及终态只读归档，不恢复旧语法执行。新 schema 为 pinned_okx_opaque_v3，旧准备不升级。详见 COMPAT-CLEANUP.md。本轮未部署、未碰真实会话或交易。

## 保留的统一 calldata 策略

用户接受统一信任固定 OKX 提供方 calldata：删除本地 V2/FOT/selector/ABI 解码，只保留格式及 envelope 检查，并强制模拟/原净额、风险、权限与防重约束。不再要求旧 opaque 开关，不证明内嵌接收方/路径/期限/链上最低值；旧参数已删，输出两个解码标记始终 false。calldataPolicy=pinned_okx_opaque_v3 绑定意图/准备，拒绝旧策略就绪记录静默升级。见 CALLDATA-POLICY.md。

## 保留的通用带税支持

通用带税代币策略：输入/卖出与输出/买入税元数据上限独立、默认 0，用精确整数比较；不按币名/合约做适配，不 gross-up 输入或授权、不自动扩大滑点或重复给报价扣税。复用原 Preview 校验最大钱包扣款与原净最低到账，税率/净额失配即停；caps 绑定原意图，仍为三次核心 HTTP。所有 calldata 都不透明，不把模拟或 API 元数据冒充链上最低值保证。18 项新测试通过，已按后续明确授权切换测试后台，但税币资金路径尚未实盘，见 TAXED-TOKENS.md。

## 保留的事件时序

新增本地单调时序：保留 WSS actor 取到完整文本消息的 Instant，区分消费/缓冲、规范块 RPC 锁/限流/交换、准入 save、worker、钱包锁/本地准备、dispatch、核心 HTTP 与结果保存。回补/恢复不伪造 live 起点，记录缺失为 null；不增加请求、连接或计时专用 save。九项新测试与扩展 controller/回补/共享 WSS 覆盖通过。见 [EVENT-TIMING.md](EVENT-TIMING.md)。

## 保留的连接观测

在现有两个池上加入 connector 层观测，不增加请求/连接或预热。`hotPathTiming.httpTransport` 分别记录三个核心调用的不透明池/socket 编号、协议、区间连接活动与建连合计。DNS/TCP/TLS 不单独拆分，不能计算服务端时间；不暴露地址/凭据，不使用 verbose。清除旧计时并分类实际 POST，避免 Execute 校验失败时误报辅助请求。见 [HTTP-CONNECTIONS.md](HTTP-CONNECTIONS.md)。

## 保留的三次核心调用模式

新 OKX Task 的设置、额度、锁和实际发送方检查在监听前完成。绑定的 `walletPolicy` 与完整参数/配置指纹一同记录；原任务截止、取消、本地身份/配置及模式原到期时间始终检查。

触发后的成功提交路径为：OKX swap → 币安 Preview → 币安 Execute，三次核心 HTTP。SDK 内部使用同一记录，不读辅助状态；客户端硬拒绝所有其他后台请求，也不偷读已存在的缓存。后台拒绝、App 确认或结果未知时暂停，不刷新、补授权、降级或重试。不是全局 skip-prechecks。

路由与模拟现取、严格预览/金额/风险检查、预览绑定、一次性消费和提交前持久防重保留。新记录的回执和对账使用原 sender，不再补查钱包列表；模式到期仍可只读确认既有交易。历史缺少新记录时不静默升级。

**三次指 HTTP 提交路径，不包括共享 WSS 的监听/回补、事件规范块校验及成交回执确认。** 其他动作/准备仍保留原有最多 3 秒读集，OKX pool、类型化计划与持久 admission 优化不变。完整风险边界见 [THREE-CALL.md](THREE-CALL.md)。

## 保留的监听前准备

每步：读额度 → 不足且明确允许时精确授权 → 首个绑定成功回执 → 启动监听。不额外等两确认或复读额度/规范块；准备独立最多 120 秒，之后才开始监听/执行预算。后续步骤仅在自己的监听前准备。

arm 可能立即花授权 Gas，`results.phase=WAITING` 才是监听就绪。取消/未命中不撤销已成功授权，首回执存在重组风险。准备未知不重放，独立 swap 保留原授权/复核策略。

## 验证结果

- 全量串行 Rust：**405 通过，0 失败，1 手工基准忽略**。
- 本轮新增无fixed独立/准备/触发、缺失/零/越滑点/高于报价/净额不足拒绝、None/Some绑定模式切换、准备价到触发价变化不冻结/不偷填1wei与摘要readiness回归；此前新增 overview 默认零HTTP/两GET刷新/忙与活动/并发准入/未加载无副作用/Cookie变化/未知操作脱敏，纯本地交易/Task摘要与真实swap gate不变、RPC不连接、payload严格拒绝、非对象journal不panic；此前新增真实 CLI 在线 add/热加载与删除/抢占拒绝、通知配置/队列/隔离/去重/失败/无重定向/大任务/mixed 及 Task 三核心计数不变回归；此前新增单钱包故障矩阵/SIGKILL 与独立 swap hash 判据、准备恢复前置拒绝；此前新增删除项拒绝/严格字段/既有钱包不覆写/终态归档测试；此前替换旧 ABI 假设测试、扩展旧意图拒绝/统一路线/旧参数兼容回归，新增不透明格式边界用例；此前 18 项税率/净额/路由/CLI/准备绑定回归及此前 9 项时序/队列/取消等回归及此前 12 项连接观测与 19 项三调用回归保留：三调用精确计数；缓存也不得绕过的辅助读取禁令；上下文/身份/客户端/链/sender/原截止/取消/模式到期检查；旧/损坏记录；后台拒绝、App 确认、未知执行的 controller 暂停；过期缓存下的零额外后台回执查询等。
- Clippy `-D warnings`、fmt、diff whitespace 检查通过。
- 实际新 Release 的 7 项 Python skill 检查和 27 项隔离 smoke 检查通过，51 个规范可见 CLI 叶子。
- 日志 `/tmp/baw-optional-minimum-tests.log`、`/tmp/baw-optional-minimum-clippy.log`、`/tmp/baw-optional-minimum-release.log`；smoke `/tmp/baw-optional-minimum-smoke.json`。
- 资金路径验证均为合成会话与回环 HTTP/WS。smoke 明确 `transactions_submitted=0`、`real_wallets_opened=false`。

## 真实环境边界

用户后续已单独授权后台切换，部署证据位于仓库外 registry/tax-support-deployment.json；不新增资金窗口。此前连接观测版的明确授权 Task bsc-usdt-observe-1793943463427 已 COMPLETED/confirmed。walletExecuteMs=1013.15；路由 86.07、Preview 433.86、Execute 479.59 ms。三个核心请求均复用已观察 socket、没有新连接尝试；Preview/Execute 同 pool/socket。币安请求 2、policyReads/cacheHits 0。监听前精确授权，实际扣 1 USDT，授权/兑换回执成功，用后 allowance 0。私有证据 tasks/bsc-usdt-observe-1793943463427-verification.json。详见 HTTP-CONNECTIONS.md。

以下前一产物记录保留作历史对照，不冒充新的连接测量；本次成功不授权未来交易或续开窗口。

此前已分别验收旧的触发后授权，以及监听前精确授权/上一轮热路径两个真实事件测试；各兑换 1 USDT 到 BNB，授权/兑换回执均成功，用后额度 0。上一轮测得 `walletExecuteMs` 约 1.42 秒，仍包含五项后台状态读取，不是此三调用版的成绩，也不是完整事件到上链延迟。

用户随后明确授权的新窗口 `bsc-usdt-three-1793939959297` 已 COMPLETED/confirmed。授权/兑换回执均 status 0x1，实际 USDT 扣款 1e18，最终 allowance=0。钱包执行 949.95 ms（路由 97.77、Preview 404.14、Execute 434.92 ms），币安请求 2、状态读取/缓存命中 0。对比上一样本 1419.88 ms，少约 470 ms；只是同边界单次观测，不是稳定保证或事件到链上延迟。

只读链上核验证据在仓库外私有 `tasks/bsc-usdt-three-1793939959297-verification.json`。未迁移/删除会话，未 commit/push；成功任务不再触发，旧取消任务不重建、不续期。后续资金测试仍需明确授权。
