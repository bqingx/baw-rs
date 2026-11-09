# OKX 唯一交易来源（Unreleased）

> 当前工作区默认且仅支持 OKX，`--provider binance` 已移除。历史部署/实盘记录不能冒充最新构建的验收；已验收阶段与新阶段见下方说明。流水线与单连接限制见 [PIPELINES.md](PIPELINES.md)。

> 当前服务器已分别通过旧时序与“监听前精确授权”的 w1 真实事件兑换，各消耗 1 USDT 兑换 BNB，授权/兑换回执均成功，用后额度为 0。监听前准备及上一轮热路径版已切换测试后台，测得约 1.42 秒钱包执行阶段（不是完整事件到上链延迟）。
> 最新三次核心 HTTP 模式已切换测试后台并通过一次真实兑换：walletExecuteMs=949.95 ms、币安请求 2、状态读取 0；实际扣 1 USDT，用后额度 0。见 [THREE-CALL.md](THREE-CALL.md)。最新 Release 未安装到 PATH。执行配置在仓库外 root 私有目录，可能含 RPC Key，禁止回显全文。
> Alchemy 免费档回补每请求 10 块限制已修复。先前暂停任务已取消，后续成功任务已完成，均不会自动重建、续期或重发；本轮开发不产生新的授权/交易。

## 来源与范围

- `market-order quote/swap`（默认且仅支持 OKX）：OKX V6 `quote/swap/approve-transaction` API，币安所选钱包预览与签署。`--provider okx` 可省略。
- `--provider binance` 拒绝；旧命令需补充 OKX 配置、精度及显式滑点，不能假定保持旧交易语义。
- 来源明确、无自动比较择优、无失败回退，不会切换到另一钱包或本地开发钱包。
- 支持现有 EVM 链集合 `1 / 56 / 8453 / 4663`、exact-in；税率上限默认 0。通用带税币模式由 --max-from-tax-percent / --max-to-tax-percent 显式设置元数据上限，钱包净额仍严格验证，详见 [TAXED-TOKENS.md](TAXED-TOKENS.md)。
- OKX 输入按原始代币数量理解，**不做 RWA 份额转换**。必须提供输入精度，并与 API 回显核对。
- `market-order list` 已不支持，明确返回迁移错误；OKX 路径应通过 `wallet operation`、`wallet tx-history`、交易 hash 和链上回执对账。
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
RPC 无默认公网回退，校验链ID；配置URL须HTTPS且无userinfo/fragment。named daemon 内自动授权会将同地址映射为 WSS，复用实例 RPC 管理器；与活跃流水线端点不一致或不支持 WSS 时明确拒绝，不回退 HTTP。legacy 冷调用仍使用 HTTPS。独立 swap 的等待范围1～120秒、确认数1～20，默认60秒/2确认；Task 准备只等首个成功回执，不使用额外确认深度或额度复读。
独立只读报价、独立手动预授权兑换或原生币输入不需要为此配置 RPC。Task 的 ERC-20 兑换即使不启用 --auto-approve，也需要 RPC 在监听前检查已有额度；不足则停止，不擅自授权。

## 只读报价

以下命令名是示例；未安装到 PATH 时使用已核实产物的绝对路径，并确认测试后台版本匹配：
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

`--okx-min-output-units` 是可选的固定最低到账（输出代币最小单位正整数）。以下示例显式设置该可选底线；省略时为 `None`，不是默认 0 或 1 wei。

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
1. 全局确认/operation-id 和原有钱包独占写锁、意图日志；日志包含 provider、数量、精度和可选输出底线（`None` / `Some`）。
2. 保留币安日限额、余额限制、交易锁检查，重新获取所选钱包实际发送方；拒绝 `--skip-prechecks`。
3. 独立核对固定 spender。默认不做自动授权；显式 `--auto-approve` 才可执行下文的精确授权流程。
4. **本次调用重新取一笔 swap 构造**，不消费旧 quote；严格核对路由白名单、链、金额、token、value、gas、最小输出和额外签名字段。无论是否设置固定底线，fresh OKX route 的 `minReceiveAmount` 都必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output；显式 `Some` 还须额外满足该固定底线。
5. 原生币输入要求 value 正好等于输入数量；ERC-20 输入 value 必须0，不允许悄悄附加原生币费用。
6. 强制独立的币安模拟成功，预览目标必须是固定交易router，无风险、无新增授权/authority变更，无额外/NFT/其他所有者资产变化；只容许属于本钱包、输入代币、固定spender且不超过本次数量的正常额度消耗。模拟净扣款不超过输入，钱包净到账必须 >= 当次 route 报告的 `minReceiveAmount`（reported），不能只满足 quote floor 或可选固定底线。
7. 预览 digest 绑定执行；自动授权完成后重新检查限额/交易锁、重新构造route，兑换准备阶段超过60秒不发起swap execute。保留既有预览过期/发送方再校验。
8. execute 只尝试一次；传输或提交结果不确定时 `UNCERTAIN/retrySafe:false`，先对账，不重发。

### Task：监听前准备，触发后不再授权

实例 Task 在每一步开始监听前查询额度，按显式 --auto-approve 的权限决定是否精确授权。只等首个绑定成功回执，不额外等两确认、不复读额度；普通独立 swap 仍遵循下文的原确认/额度核验流程。

Task 准备有独立最多 120 秒预算，就绪后才开始 valid_for_ms 的监听/执行预算。RUNNING 可能仍在准备，results.phase=WAITING 才代表监听建立/回补完成。准备和实际兑换有独立确定性操作日志，就绪记录绑定发送方、完整参数和私有配置指纹。

可选固定底线的 `None` / `Some` 连同完整 args 绑定原意图和就绪记录；省略与显式设置不可互换，不静默升级旧任务/准备或重放，不改变已有显式任务、授权范围及截止。准备期旧 quote floor 不用于执行/触发；触发时使用 fresh route 的 reported minimum 和当次 quote/slippage floor。

触发后的 swap 不查额度、不取 approve-transaction、不签 approve、不等待授权；只重新取实际路由、检查/模拟并提交兑换。外部操作消耗额度、配置变化或准备不确定时暂停，不临场补授权或重发。首回执存在重组风险，接受该快路径不意味着最终性保证。取消/未触发不撤回授权或退回 Gas。

### 可选精确自动授权（独立 swap）

在上述 swap 命令中增加 `--auto-approve`；默认关闭，币安原生 provider 已移除。

流程：
1. 原生BNB等原生币输入直接跳过授权；ERC-20输入通过私有RPC查询 `allowance(owner, pinnedSpender)`。
2. 已有额度足够则不发授权、不降低已有额度。额度不足才先取一笔只读quote，拒绝已知坏币/价格冲击，若设置固定输出底线也拒绝低于它的报价，避免明显无效的授权支出；这笔准备报价不提供后续执行的 floor。
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

### calldata：统一信任固定 OKX 提供方，不做本地 ABI 解码

按用户明确接受的策略，所有 selector/布局统一作为不透明 bytes 转交币安模拟，旧 V2/FOT 解码器已删除。不解析内嵌接收方、路径、数量、期限或最低值，也不因陌生/改变的 ABI 拒绝。保留非空十六进制与大小限制、固定 router/spender、金额/value/Gas、fresh route 最低值及可选固定底线，以及强制 Preview 的钱包净额/风险/权限校验。

`--okx-allow-opaque-calldata` 已删除，不再作为无效参数接受；正常操作确认不变。`onchainMinimumDecoded` 与 `onchainNetMinimumDecoded` 始终 false；API 元数据和本次模拟不是链上接收方/最低到账或未来执行的独立证明。策略版本绑定原交易/准备意图，旧准备不能静默升级。详见 [CALLDATA-POLICY.md](CALLDATA-POLICY.md)。`--acknowledge-token-risk` 仍不代表审计。

### 费用和其他限制

- 此签名路径不支持可靠的 MEV 广播承诺或 gasPrice 级别透传，所以必须显式 `--mev false`，保留默认 `--gasLevel MEDIUM`，而不是悄悄忽略请求的保护。
- `max_gas_limit` 是 gas单位上限，不是 gas价格或总手续费上限。
- OKX 基础设施费、正滑点条款及实际成交受套餐和链上情况影响，具体按官方文档及响应核查。
- 本实现无自动 HTTP 重试、无重定向、无代理、无付费重试、无任意 API origin 覆盖；仅测试编译中可注入回环 mock。
- 带税币不再一律拒绝，但需独立税率上限、可信路由/净额模拟和上述 fresh route 最低到账校验（显式固定底线为额外约束）；全部 calldata 净最低语义均不在本地证明，按用户接受的统一提供方信任策略执行，不保证任意税合约可执行。额外 Permit/签名仍不支持。自动approve只限显式开启的原输入金额，不因税扩大，不是任意权限审批或无限授权。
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

测试只使用合成凭据、临时私有目录与回环 HTTP：验证签名查询串、金额精度、风险/最低输出/路由拦截、单次执行、未知结果不重试、默认仅 OKX、拒绝币安 provider 及确认门禁，以及额度跳过/精确授权、收据绑定和确认数、重读额度、新报价顺序、正常额度消耗、回滚/超时/重组/缺hash后禁止swap、阶段日志持久恢复。
离线测试不使用真实 Key 或生产交易。此前明确授权的窗口已各验收一次旧时序和监听前精确授权时序的真实事件兑换；更早的回补失败任务已取消且未提交交易。最新三次核心 HTTP 模式随后也在新的明确授权窗口通过一次实盘验收；历史五分钟测试及 opaque calldata 批准不构成未来交易或无限期启用任务的默认授权。
