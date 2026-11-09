# OKX 兑换与钱包转账

## 交易

`market-order quote/swap` 默认且仅使用 OKX 聚合器，详见 [OKX-TRADE.md](OKX-TRADE.md)。币安原生报价/兑换实现已删除，不存在自动 provider 回退。`market-order list` 返回明确迁移错误。

- 需要私有 OKX 配置、输入精度、显式滑点；`--okx-min-output-units` 为可选固定底线，默认 `None`，不是 0/1 wei。显式 `Some` 仍额外严格约束固定最低到账。
- 执行/触发使用 fresh OKX route 的 `minReceiveAmount`：必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output；强制独立模拟的钱包净到账 >= reported minimum。不采用准备期旧 quote floor，不自动增加滑点或税率，不本地解码 calldata，也不称为链上独立保底。
- 自动授权显式选择、精确本次数量，等待回执、复核额度后重新报价。
- 币安钱包负责预览、签名和广播；公开外部合约接口没有 MEV/Gas 价格档位参数。不能把原生兑换参数或 OKX 元数据当作最终广播保护。
- 写操作要求确认和唯一 operation ID；`None` / `Some` 连同完整 args 绑定意图/准备，已有显式任务、授权和截止不变，不静默升级旧记录或重放；未知结果暂停，不自动重发。

## 转账

`wallet send` 保留币安钱包原生转账接口，与已删除的原生兑换不是同一路径。

- `--amount`（人类可读数量）与 `--max` 必须且只能选一个。
- 明确链、完整 tokenAddress、完整 recipient；域名必须先单独解析并核对地址。
- 转账接口仍提供 `--gasLevel LOW/MEDIUM/HIGH`，不要与合约调用仅支持 gasLimit 混淆。
- RWA 转账保留精确份额换算和余额/精度检查，不使用浮点数。
- 后端要求 App 确认时返回 PENDING_CONFIRMATION，不能视为已发送。
- 请求断开、HTTP 错误或无可信 hash 时不自动重试；查看本钱包 operation 日志和链上结果。

## 自动执行与恢复

多钱包动作统一由 [实例级流水线](PIPELINES.md) 调度；旧钱包级任务和 typed prepare/trigger 执行路径已删除。

交易/签名/授权是实际资金操作。本轮构建和离线测试不构成真实登录、自动授权、交易或部署的批准，也不代表生产可用性验证。
