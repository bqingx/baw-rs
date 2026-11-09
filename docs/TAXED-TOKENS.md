# 通用带税代币支持

按转账税行为校验，不按币名或合约地址写适配/白名单。最新源码与 Release 已完成离线验证；用户随后明确要求切换后台，f7050cfa… 已作为测试 daemon PID 75994 启用，w1 状态 CONNECTED，身份/会话及历史任务状态未变。未对任何真实带税币授权/交易，不能把既有 USDT→BNB 验收或进程切换当税币资金验收。

## 使用方式

Quote 和 Swap 都支持以下通用参数（Task 的 argv 同样支持）：

```text
--max-from-tax-percent 3   # 可接受 OKX 报告的输入/卖出税元数据不高于 3%
--max-to-tax-percent 3     # 可接受 OKX 报告的输出/买入税元数据不高于 3%
```

两者默认都是 0，因此既有意图不会被静默升级为接受带税币。只卖带税币给无税资产可仅设置输入上限；只买带税币可仅设置输出上限；两端都带税则分别设置。相同策略可用于任意满足约束的币种，不需逐币改代码。

参数是 **百分数**：`3` 表示 3%，不是 0.03%。支持至多 8 位小数，必须在 0..<100。OKX `taxRate` 则是比例：`0.02` 表示 2%。全部按整数缩放比较，不使用浮点；刚好等于上限允许，超过一单位也拒绝。缺失/畸形/未知税率及原生币报告非零转账税均拒绝。

这只是对提供方税率元数据的上限，不是写入代币合约的税率锁，也无法保证其真实/未来税率。`--acknowledge-token-risk` 仍必须显式提供；税率上限不替代 honeypot/风险检查。

## 钱包金额、最低到账与滑点

1. 输入数量仍为允许钱包扣款的上限，不为“池子要收到多少”自动扩大数量。授权最多本次原金额，不因税而自动 gross-up，也不授权无限额度。
2. `--okx-min-output-units` 是可选固定底线，默认 `None`，不默认 0/1 wei；显式 `Some` 必须为输出币最小单位正整数，并额外严格约束 **钱包税后净到账**。
3. 执行/触发使用 fresh OKX route 的 `minReceiveAmount`：必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output；显式 `Some` 时还必须 >= 固定底线。不采用准备期旧 quote floor，不对 OKX `toTokenAmount` 再减一次税，不自动扩大滑点、提高税率上限或降低最低值。
4. 对同一笔 calldata 强制做独立币安 Preview，必须模拟成功；合计钱包输入扣款 `0 < debit <= requested amount`，输出净入账必须 `>= 当次 route 的 reported minimum`，显式固定底线因此也必须满足。额外向钱包扣税、净到账不足、异常 owner/NFT/资产、风险或权限变化均停下，不提交。
5. 这是保守接受条件：如果提供方最低值实际是税前而模拟税后低于它，可能拒绝本来可执行的交易。不会靠静默改滑点或减掉所谓“2%”解决。

### 为什么不统一给报价乘 `(1-tax)`

OKX 官方 quote/swap 文档将 `toTokenAmount` 描述为预计输出，将 `minReceiveAmount` 描述为滑点限制下的最低输出，但没有明确保证它们是否包含每一层转账税。对已经含税的报价再扣税会重复计算；多跳、多次收费、动态/非比例税也不能靠一次乘法覆盖。

因此返回 `transferTax` 元数据及风险说明，注明税额包含关系未承诺，不伪造“预计税后输出”。`runtimeBalanceValidation` 单独记录本次成功模拟的实际钱包净变化，不把模拟叫作最终成交。

## calldata 最低值的真实含义

用户已接受统一信任固定 OKX 提供方，最新代码不再解码任何 selector/ABI（包括旧标准/FOT V2）。全部 calldata 交给同一强制净余额模拟流程，不因格式升级走本地解析错误。`--okx-allow-opaque-calldata` 已删除，全部路径仍强制模拟。

`onchainMinimumDecoded` 和 `onchainNetMinimumDecoded` 始终 false。不证明内嵌接收方、路径或链上净最低值；风险/权限、最大钱包扣款、净到账与税率元数据上限仍严格检查，税率默认仍 0。策略版本在原意图中绑定，不能复用旧准备。见 [CALLDATA-POLICY.md](CALLDATA-POLICY.md)。

## Task 与热路径

两个税率上限、可选固定底线的 `None` / `Some` 连同完整 args 进入交易意图及准备绑定，改变任一个都不能复用原就绪记录。已有显式任务、授权和截止不变，旧记录不自动升级或重放。需要授权时，准备报价若已超过税上限则在花授权 Gas 前拒绝；触发时现取 swap 的税元数据再次校验，不在等待期轮询税率。

触发后仍只有一次 OKX swap、一次 Preview、一次 Execute；税检查本地完成，不增加 HTTP/RPC、补授权、修复或重试。税率、余额或合约状态失配导致拒绝/未知时按原规则暂停。读取时与执行时可能发生状态变化，不续期或保证未来税率。

## 覆盖与限制

离线覆盖输入税、输出税、双端税、默认拒绝、精确上限/超过上限、单位/格式/native 矛盾、税后净到账不足、额外钱包扣款、准备前拒绝、触发期税率变化、绑定税率上限、三请求计数、旧标准/FOT/new selector 的统一不透明策略与原策略隔离。

不承诺支持所有声称带税的合约。黑名单、反机器人、任意额外扣款、重基/反射、嵌套或第三方模拟行、Permit/新增权限、不受支持的路由、未知或不可解释的净额仍拒绝。动态税在模拟与执行间变化也可能使交易失败。钱包实际最小到账硬保证必须依赖可信链上路径的净额检查，不能只依赖 metadata/模拟。

## 核验来源

- [OKX quote](https://web3.okx.com/onchainos/dev-docs/trade/dex-get-quote)
- [OKX swap](https://web3.okx.com/onchainos/dev-docs/trade/dex-swap)
- [OKX approval](https://web3.okx.com/onchainos/dev-docs/trade/dex-approve-transaction)
- [Uniswap V2 Router02 源码](https://github.com/Uniswap/v2-periphery/blob/master/contracts/UniswapV2Router02.sol)：supporting-fee-on-transfer token/token 函数检查接收方余额差，不等于证明任意同 selector 部署的实际代码。
