# 可选 OKX 交易来源（Unreleased）

> 本机已获用户授权完成Release构建并同步升级 `/usr/local/bin/baw-rs` 与daemon，真实只读报价通过。
> 已获用户明确授权完成BSC 1 USDT→BNB精确授权及成交测试，两笔链上回执成功；这不是全部币/链/路由的验收。
> 其他部署也必须同步升级CLI/daemon，旧daemon不认识新增参数。`bsc-quote.json`仍仅供只读；本机执行配置为私有的 `bsc-swap.json`（含RPC Key，不回显全文）。

## 来源与范围

- `market-order quote/swap --provider binance`（默认）：沿用币安原生询价、下单、RWA 规范化路径，旧命令无需改动。
- `market-order quote/swap --provider okx`：OKX V6 `quote/swap/approve-transaction` API，币安所选钱包预览与签署。
- 来源明确、无自动比较择优、无失败回退，不会切换到另一钱包或本地开发钱包。
- 第一版只支持现有 EVM 链集合 `1 / 56 / 8453 / 4663`、exact-in、无税代币。
- OKX 输入按原始代币数量理解，**不做 RWA 份额转换**。必须提供输入精度，并与 API 回显核对。
- `market-order list` 仍仅查询币安原生订单；OKX 路径应通过 `wallet operation`、`wallet tx-history`、交易 hash 和链上回执对账。
- 不安装 OKX Skill/MCP、不创建 OKX 钱包、不启用自动付费或订阅，也不加入推荐佣金参数。

## 私有配置

为兼容现役 systemd 只暴露钱包树的隔离设置，建议文件放在 `/root/wallets/okx/`。
父目录须属于运行用户且为 `0700`，配置及凭据须为同用户 `0600`、普通单硬链接文件。
路径必须绝对，不接受符号链接别名。不要把凭据放在本仓库或命令行。

凭据文件 `/root/wallets/okx/credentials.json`：
```json
{
  "api_key": "<由安全渠道填入>",
  "secret_key": "<由安全渠道填入>",
  "passphrase": "<由安全渠道填入>"
}
```

只读询价配置 `/root/wallets/okx/bsc.json`：
```json
{
  "credential_file": "/root/wallets/okx/credentials.json",
  "chain_index": "56",
  "max_gas_limit": 2000000,
  "max_price_impact_percent": "2"
}
```

执行兑换还必须在该配置中明确填入：
```json
{
  "expected_spender": "<已独立核实的该链OKX授权spender地址>",
  "allowed_router_addresses": ["<已独立核实的该链OKX交易router地址>"]
}
```
以上是需要合并到同一个配置对象中的字段，不是另一个完整文件。
地址不提供默认值，也不从一次请求的 `tx.to` 自动升级为可信地址。
请根据官方部署清单和链上状态独立核实；spender 可以与 router 不同。
每次 ERC-20 兑换都会将 OKX `/approve-transaction` 的 spender 与固定值比较，但不会执行它返回的授权 calldata。
开启 `--auto-approve` 时，授权 calldata 由本地按固定 token/spender/本次精确数量生成。

ERC-20 自动授权还需要在同一私有配置中加入：
```json
{
  "rpc_url": "https://<该链RPC地址，可包含API Key，禁止回显>",
  "approval_wait_ms": 60000,
  "approval_confirmations": 2
}
```
RPC 无默认公网回退，校验链ID；URL须HTTPS且无userinfo/fragment。等待范围1～120秒、确认数1～20，默认60秒/2确认。
仅查询、手动预授权后的兑换或原生币输入不需要为此配置RPC。

## 只读报价

以下命令使用完成构建并同步升级后的CLI/daemon（本机已完成）：
```sh
baw-rs --wallet w1 market-order quote \
  --provider okx --okx-config /root/wallets/okx/bsc.json \
  --binanceChainId 56 \
  --fromToken <输入代币> --toToken <输出代币> \
  --fromTokenQty 1.25 --fromTokenDecimals 18 \
  --slippage 1 --json
```

- `--fromTokenQty` 保留人类可读数量；示例 1.25、18位精度严格转换为 `1250000000000000000`，不用浮点数、不静默舍入。
- `--slippage 1` 是 **1%**；OKX 不支持本入口的 `auto`，最多8位小数，范围0到小于100。
- EVM 原生币可用 `0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee` 或零地址，统一规范为前者，精度必须18。
- 价格冲击符号在不同文档/版本描述存在差异，本入口保守检查**绝对值**不超过配置阈值。缺失/畸形价格冲击、税率或 honeypot 字段均拒绝。
- 回显链、模式、代币合约、输入数量、精度必须一致。输出是报价，不是成交或链上最低到账承诺。

## 兑换

```sh
baw-rs --wallet w1 market-order swap \
  --provider okx --okx-config /root/wallets/okx/bsc.json \
  --binanceChainId 56 \
  --fromToken <输入代币> --toToken <输出代币> \
  --fromTokenQty 1.25 --fromTokenDecimals 18 --slippage 1 \
  --okx-min-output-units <用户确认的输出代币最小单位正整数> \
  --mev false --acknowledge-token-risk \
  --confirm --operation-id <唯一ID> --json
```

保护流程：
1. 全局确认/operation-id 和原有钱包独占写锁、意图日志；日志包含 provider、数量、精度和输出底线。
2. 保留币安日限额、余额限制、交易锁检查，重新获取所选钱包实际发送方；拒绝 `--skip-prechecks`。
3. 独立核对固定 spender。默认不做自动授权；显式 `--auto-approve` 才可执行下文的精确授权流程。
4. **本次调用重新取一笔 swap 构造**，不消费旧 quote；严格核对路由白名单、链、金额、token、value、gas、最小输出和额外签名字段。
5. 原生币输入要求 value 正好等于输入数量；ERC-20 输入 value 必须0，不允许悄悄附加原生币费用。
6. 币安成功模拟，预览目标必须是固定交易router，无风险、无新增授权/authority变更，无额外/NFT/其他所有者资产变化；只容许属于本钱包、输入代币、固定spender且不超过本次数量的正常额度消耗。模拟净扣款不超过输入，输出不低于返回的最低到账值。
7. 预览 digest 绑定执行；自动授权完成后重新检查限额/交易锁、重新构造route，兑换准备阶段超过60秒不发起swap execute。保留既有预览过期/发送方再校验。
8. execute 只尝试一次；传输或提交结果不确定时 `UNCERTAIN/retrySafe:false`，先对账，不重发。

### 可选精确自动授权

在上述 swap 命令中增加 `--auto-approve`；默认关闭，币安原生provider不接受此开关。

流程：
1. 原生BNB等原生币输入直接跳过授权；ERC-20输入通过私有RPC查询 `allowance(owner, pinnedSpender)`。
2. 已有额度足够则不发授权、不降低已有额度。额度不足才先取一笔只读quote，拒绝已知坏币/价格冲击或低于用户输出底线的报价，避免明显无效的授权支出。
3. 本地编码 `approve(spender, 本次完整输入数量)`，不是补差额，也不使用无限额度。只允许这笔精确grant；预览里未知授权格式、其他权限或资产变化均拒绝。已严格兼容币安 `preAmount/postAmount` 数字字段及 `tokenType:Erc20, tokenId:null`，保持uint256精度、别名一致性与NFT拒绝，不会静默忽略这些变化。
4. 同一个operation-id、同一把独占写锁覆盖授权和兑换。先持久化 `approval_submitting`，execute只尝试一次。授权Gas上限是 `min(max_gas_limit, 300000)`，不会提高用户原有上限。
5. 等待授权收据、绑定hash/from/to，达到配置确认数并核对收据区块仍在规范链上；成功后重读链ID及allowance，必须正好等于本次授权数量，才继续。
6. **授权期间的报价不用于兑换**；确认后再获取一笔新swap、重新模拟、绑定digest提交。正常额度消耗回显只能是同token/spender的有界下降，不能借此新增授权。

首次通常多一笔approve和一次Gas，某些代币要求先approve(0)：本实现不自动清零、不切换无限授权、不反复重试，需人工处理。
授权确认不等于兑换成功。后续报价、opaque calldata、风险或限额检查失败时，授权仍可能保留在链上，不自动撤销，也不自动重启兑换。

结果与恢复：
- `autoApproval` 包含是否需要授权、精确数量、已有/确认后额度及授权hash。
- `APPROVAL_UNCERTAIN`：提交、收据或确认状态不确定（含等待App确认但无hash）；不尝试swap，不重发approve。
- `APPROVAL_REVERTED` / `APPROVAL_STATE_MISMATCH`：回滚或收据成功但精确额度未验证；不尝试swap。
- `SWAP_NOT_SUBMITTED`：额度处理后兑换准备失败；结合 `autoApproval` 判断授权是否已经成功。`UNCERTAIN` 仍需先对账。
- 使用 `wallet operation --operation-id <ID>` 查看 `progress.stage`、`progress.approvalTxHash`、`progress.approvalOrderId`、`progress.swapTxHash`。阶段证据在最终结果失败后仍保留；崩溃可能留下未完成阶段，不能据此认定交易没发。
- 拒绝在跳过独占锁的调试模式下提交多阶段操作；没有独立的子操作ID，也不递归获取同一把写锁。

### 不透明 calldata：默认拒绝

现有路由校验器仅能完整解码已实现的 V2 `swapExactTokensForTokens` 调用。
OKX 常用聚合器的 selector、ABI 会随路由变化，这些 calldata 暂时无法在本地证明真实接收方、token路径和**链上**最低输出。

因此默认拒绝。用户明确接受此限制后，可额外传：
```sh
--okx-allow-opaque-calldata
```
此时仍须通过固定路由/金额/gas/响应最低值及严格币安模拟资产变化检查；输出明确标记 `onchainMinimumDecoded:false`。
**模拟结果与 API 的 `minReceiveAmount` 不是真实交易的硬性最低到账证明。**
如不接受该信任假设，不要加此开关；使用已支持的可解码路径，或先独立解码审核后走普通合约预览/执行。
`--acknowledge-token-risk` 也不代表审计。

### 费用和其他限制

- 此签名路径不支持可靠的 MEV 广播承诺或 gasPrice 级别透传，所以必须显式 `--mev false`，保留默认 `--gasLevel MEDIUM`，而不是悄悄忽略请求的保护。
- `max_gas_limit` 是 gas单位上限，不是 gas价格或总手续费上限。
- OKX 基础设施费、正滑点条款及实际成交受套餐和链上情况影响，具体按官方文档及响应核查。
- 本实现无自动 HTTP 重试、无重定向、无代理、无付费重试、无任意 API origin 覆盖；仅测试编译中可注入回环 mock。
- 不支持税币或额外 Permit/签名。自动approve只限显式开启的精确ERC-20授权，不是任意权限审批或无限授权。
- `BROADCASTED` / `SUBMITTED` 不等于成功成交；先检查链上 receipt 和实际余额变化。

## 离线验证

```sh
cargo test --locked okx_trade::tests
cargo test --locked --test okx_cli
cargo test --locked --lib trade::tests
cargo test --locked --lib signing::tests
cargo test --locked --lib route_action::tests
cargo clippy --locked --all-targets -- -D warnings
```

测试只使用合成凭据、临时私有目录与回环 HTTP：验证签名查询串、金额精度、风险/最低输出/路由拦截、单次执行、未知结果不重试、默认仍走币安及确认门禁，以及额度跳过/精确授权、收据绑定和确认数、重读额度、新报价顺序、正常额度消耗、回滚/超时/重组/缺hash后禁止swap、阶段日志持久恢复。
以上离线测试不使用真实Key或生产交易。部署后已另用真实凭据完成BSC只读quote（同时验证默认币安quote）及用户明确授权的1 USDT→BNB测试：精确授权1 USDT、等待确认/重读额度、重新取价、模拟和swap均通过，两笔receipt status1、用后额度0。此次用户明确接受opaque calldata限制，该接受不作为未来交易的默认授权。
