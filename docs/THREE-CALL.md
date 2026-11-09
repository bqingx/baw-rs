# Task OKX 三次核心 HTTP 调用

最新源码/Release 行为。SHA256 `71801ce9…` 已切换测试后台 PID 3801，完成一次明确授权的真实事件兑换：监听前授权、命中后仅三次核心 HTTP、Task confirmed。

## 范围与准确计数

适用于带新就绪记录的 **Task OKX swap**。成功的提交路径只有：

1. OKX `/swap` 一次：现取报价与 calldata。
2. 币安 `contract-call/preview` 一次：现做模拟。
3. 币安 `contract-call/execute` 一次：提交接受的 requestId。

这里统计 HTTP，不是宣称整个 Task 只有三次网络操作。监听建立/回补、事件规范块校验和提交后的回执确认仍通过实例共享 WSS。普通独立 swap、普通合约调用及不带新凭据的历史只读恢复，不自动变成这个模式。

## 监听前

准备阶段继续检查当前设置、日额度、交易锁和实际钱包发送方，并按显式许可处理精确授权。只有首次绑定成功回执后才准备就绪，不额外等两确认或复读额度。

新增私有 `walletPolicy` 记录保存：链、固定发送方、已检查的 Developer Mode 到期时间、当前客户端身份的单向指纹。它与完整兑换参数和私有配置指纹一同绑定，且只能在原 Task 截止/取消及钱包绑定 gate 下使用。

这是本地就绪记录，不是发给币安的权限 Token，也不表示后台将来一定接受。旧就绪记录缺字段时拒绝使用，不静默升级、重查状态或重新授权。

## 触发后

- 从已绑定记录取得 sender，不查询钱包/网络列表。
- 不查询 settings、日额度、交易锁或 allowance；不调用 approve API。
- Preview/Execute 的内部钱包策略检查也使用同一记录，校验链、sender、客户端身份、原任务 gate 和记录中的模式到期时间。
- 客户端在此作用域只允许两个精确的 POST 路径：contract-call preview/execute。任何辅助请求立即失败，即使缓存已有值也不能偷偷返回；这不是全局 skip-prechecks。
- SDK 内部仍保留预览内容/风险/资产变化检查、有效期、一次性消费、SHA256 绑定、提交前持久防重。路由与模拟必须现取。
- 固定最低到账可选，省略为 `None`（不默认 0/1 wei）；fresh OKX route 的 `minReceiveAmount` 仍必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output。显式 `Some` 还须额外满足固定底线；独立 Preview 的钱包净到账必须 >= reported minimum。
- 不采用准备期旧 quote floor，不自动增加滑点或税率，不本地解码 calldata，不把提供方元数据/模拟称为链上独立保底。`None` / `Some` 连同完整 args 绑定原记录，已有显式任务、授权和截止不变；不自动升级旧记录或重放。
- 新记录的回执/对账使用原 sender，不再为确认成交补查钱包列表。模式到期不妨碍对既有交易做只读确认，但不允许继续发新请求。

## 接受的取舍

等待期间，后台设置、额度、交易锁或会话有效性可能变化；不做后台高频刷新，也不会在触发时提前发现所有远端变化。后台拒绝、要求 App 确认、结果未知或额度失效时暂停，不补查、不补授权、不降级或重发。不能保证服务端总能拒绝每一种准备状态变化，也不能把准备通过视为保证可提交。

本地身份/会话文件或配置变化、取消、原截止、固定模式到期仍会阻止提交。Developer Mode 后台续期不会自动延长已有就绪记录，需明确重新安排。

## 指标

成功的新模式预期 `stateReadStats.binanceRequests=2`、`policyReads=0`、`cacheHits=0`，再加 OKX swap 一次；`preparedWalletPolicyUsed=true` 仅表示使用此模式，不代表成交成功。

这里 `maxAgeMs=0` 表示读集仅作计数，根本不允许状态读取；不是“禁用缓存后强制刷新”。BAW_RS_CHECK_TTL_MS 不会重新启用热路径辅助查询。其他路径的有界读集仍按原规则工作。

`walletExecuteMs` 和 `hotPathTiming` 的测量边界见 [HOT-PATH.md](HOT-PATH.md)。本版一次真实样本：路由 97.77 ms、Preview 404.14 ms、Execute 434.92 ms，walletExecuteMs=949.95 ms。相比前一版同边界的 1419.88 ms，单次观测少约 470 ms（33%），不是稳定提速保证，更不是完整事件到上链耗时。币安请求 2、状态读取/缓存命中 0；链上实际扣 1 USDT，授权/兑换回执成功，用后额度 0。

## 回归保证

- 直接 SDK 与完整 controller 均验证：一个路由、一个 Preview、一个 Execute。
- 拒绝任何触发期辅助请求，包括已有缓存的调用。
- 准备记录缺失/过旧格式、字段篡改、错误客户端/链/sender、模式到期、任务过期/取消/身份变化，在核心请求前停止。
- 后台 Preview 拒绝、Execute 结果未知、等待 App 确认，都暂停且不刷新/重试。
- confirmed 模式在旧地址缓存过期后仍只用记录 sender 确认回执；无隐形的提交后钱包 API 查询。
- 正常独立调用、原有授权策略、按需单 WSS、日志 10 块分页和恢复防重回归保留。
