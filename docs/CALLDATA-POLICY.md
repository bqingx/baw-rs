# OKX calldata：统一信任提供方的不透明数据

用户已明确接受移除本地 ABI 解码，以免 OKX 路由/编码更新被少量硬编码解析器阻断。最新策略/意图 schema 为 `pinned_okx_opaque_v3`；本轮只代码/离线验证/构建，未切换后台或新增交易。

## 做什么、不做什么

- 从现有固定 HTTPS OKX 接口获取 calldata，保持原 bytes/大小写，不按 selector 选择解析器。
- 已删除 `DecodedV2`、`decode_v2` 和标准/FOT V2 分支。旧 `0x38ed1739`、`0x5c11d795`、新 selector 和短数据都经过同一流程；没有“先解析、失败后 fallback”。
- 不解码嵌入的路径、接收方、数量、deadline、最低输出或转账税语义，也不再强行推断入口 router 等于授权 spender。
- 保留最基本的数据格式限制：非空、0x 前缀、偶数长度十六进制、最大长度。不要求固定的四字节 selector/ABI 布局。只对格式异常拒绝，不对陌生/改变的 ABI 拒绝。
- 保留响应的链/from/token/数量/滑点等元数据一致性、固定 router allowlist、独立固定 spender、value/Gas 上限与正最低值等检查。

## 仍强制的执行约束

每次 swap 仍现取 route → 币安 Preview → Execute：模拟成功、风险/权限检查、最大钱包输入扣款、钱包净输出最低值、有限 allowance 消耗、预览 SHA256/客户端绑定、一次性消费、提交前持久防重、身份/会话/取消/截止 gate 全部保留。额外 Permit/签名不因此放行。

固定最低到账 `--okx-min-output-units` 可选，默认 `None`，不是 0/1 wei。执行/触发时 fresh OKX route 的 `minReceiveAmount` 必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output；显式 `Some` 仍额外严格要求 reported minimum >= 固定底线。独立 Preview 的钱包净到账必须 >= reported minimum，不能只满足 quote floor 或可选固定底线。不采用准备期旧 quote floor，不自动增加滑点或税率。

这些是元数据与独立模拟校验，不是本地 calldata 解码或链上独立保底。信任 OKX 编码不等于信任任意目标、扩大金额或盲目重试。后台拒绝、要求 App 确认、缺 hash 或结果不确定仍暂停。

输出统一为：

```json
{
  "onchainMinimumDecoded": false,
  "onchainNetMinimumDecoded": false,
  "calldataPolicy": "pinned_okx_opaque_v3",
  "routeValidation": {
    "recipient_verified": false,
    "calldata_min_out_verified": false,
    "requires_runtime_preview": true,
    "minimum_output_semantics": "unknown"
  }
}
```

提供方元数据/本次模拟不证明 calldata 在链上编码了指定接收方或最低值，模拟成功也不保证未来执行状态、税率或成交。内嵌 deadline 不再本地解析，原任务/preview 的本地截止仍限制新请求准入，不能撤回已经准入的链上交易。

## 兼容与原意图

- `--okx-allow-opaque-calldata` 已删除，传入会报参数错误。正常交易确认与独立税率上限仍需要。
- builder 配置的 `allowed_selectors`、`require_runtime_preview`、旧 `auto_approve`、`prepare_at_load` 已删除，严格配置不再接受这些字段。实际交易入口 --auto-approve 仍有效，不可混淆。
- 已删除旧全局 precheck 兼容 setter，保留当前请求级策略及服务默认值；OKX/Task 仍不能 skip-prechecks。
- 交易意图与准备记录绑定明确的 calldataPolicy 和完整 args（包括固定底线的 `None` / `Some`）；旧/不同策略的准备记录在监听/执行前拒绝，不静默升级、续期或重放。已有显式任务、授权范围与截止不改，历史已提交交易的只读回执核验不受影响。
- 输入/输出税率元数据上限默认仍 0，不因为移除 ABI 而自动接受任意税。税后净到账与原输入金额上限继续通过 Preview 检查，不 gross-up 授权/金额、不改滑点。

## 离线回归

覆盖全部 selector 统一、不透明 bytes 原样转交、旧 V2 ABI 改布局/路径/接收方/内嵌数量/期限不触发本地解析、短 calldata、格式/大小限制、固定 envelope 约束、强制模拟、税币净额不足/多扣款拒绝、旧准备策略在请求前拒绝及三次核心 HTTP 计数。旧 ABI 测试已改为该策略的回归，不用通过“删除失败测试”伪装兼容。
