# 日常流程细节

以下都用命名钱包形式 `"$B" --wallet "$NAME" ...`；参数细节以对应 `cli-*.md` 为准。

## 登录

```sh
"$B" --wallet "$NAME" --confirm --operation-id "login-$NAME-1" auth signin --json
# 把 urlForWeb / pairingCode 给用户（可生成二维码），等用户在 App 确认
"$B" --wallet "$NAME" --confirm --operation-id "verify-$NAME-1" auth verify --qrCodeId "$QR" --json
"$B" --wallet "$NAME" wallet status --json
```

- 登录也会改本地状态，所以同样要 `--confirm` + 各自唯一的 operation-id。
- verify 用原样 qrCodeId；二维码超时就重新 signin，不反复用旧码。由助手主动 verify，不需要用户回来报告。
- 有效期以接口返回为准；不为保活偷偷交易。`auth signout` 只退出指定钱包。

## 查询

- `wallet balance [--binanceChainId C] [--tokenAddress T] [--hideLowValue ...]`：按返回的链分组展示，代币核对 decimals。
- `wallet tx-history [--binanceChainId C] [--tx HASH]`：按交易哈希核对。
- `wallet settings` / `left-quota` / `tx-lock`：诊断用，只读，不擅自改设置或清交易锁。
- `wallet gas-price --binanceChainId C`：查 Gas。

## 转账

1. 展示：链、代币完整地址（原生币标明）、收款完整地址、数量、Gas；确认余额覆盖 数量 + Gas，且在日限额内。
2. 域名（`.eth` / `.bnb` 等）先 `wallet resolve-domain --name X --binanceChainId C`，展示解析地址后再确认。
3. `"$B" --wallet "$NAME" --confirm --operation-id OP wallet send --binanceChainId C --recipient ADDR --amount X [--tokenAddress T]`
4. 地址簿/限额/App 确认由后端要求时照实说明，不绕开。
5. `wallet speed-up` / `wallet cancel` 是新的链上操作，单独确认；cancel 不保证抢先成功。

## OKX 兑换

```sh
# 报价（只读）
"$B" --wallet "$NAME" market-order quote --okx-config "$OKX" --binanceChainId 56 \
  --fromToken "$FROM" --toToken "$TO" --fromTokenQty 10 --fromTokenDecimals 18 --slippage 10 --json

# 执行（用户确认后）
"$B" --wallet "$NAME" --confirm --operation-id "$OP" market-order swap --okx-config "$OKX" \
  --binanceChainId 56 --fromToken "$FROM" --toToken "$TO" --fromTokenQty 10 --fromTokenDecimals 18 \
  --slippage 10 --mev false --acknowledge-token-risk [--auto-approve] --json
```

- OKX 配置（0600，仓库外）里有凭据文件路径、`chain_index`、`expected_spender`、`allowed_router_addresses`、`max_gas_limit`、`max_price_impact_percent`；ERC-20 自动授权还要 `rpc_url`、`approval_wait_ms`、`approval_confirmations`。不要回显配置全文（含 RPC Key）。
- 价格冲击超过配置阈值会在本地被拒（不上链不扣币）；报价回显与参数不一致也会拒绝。
- swap 每次都重新取路由（不复用旧报价）、强制模拟：模拟净到账必须 ≥ OKX 当次最低值（及可选固定底线），钱包扣款不超过输入。
- `--auto-approve`：额度不足时只授权本次精确数量，等回执、复读额度、重新取路由再 swap；授权与兑换在日志里分阶段记录。
- 卖出全部持仓时注意代币精度与余额整数；给钱包留足原生币作 Gas。
- `market-order intent ...`（同参数）只在本地生成意图摘要，不读配置、不联网、不授权。

## 授权管理

- `approvals list` / `detail`：展示链、代币、spender、额度。
- `approvals approve --binanceChainId C --tokenContract T --spender S --amount X`：只给已核实的 spender 和需要的额度，不默认 `--unlimited`。
- `approvals revoke --binanceChainId C --tokenContract T --spender S`：撤销前确认。
- 都是写操作，需要 `--confirm --operation-id`。

## 多钱包批量（如免费铸造）

1. 并行只读：抓页面/ABI、各钱包地址、余额、资格/已领取状态、Gas。
2. 用户确认后，各钱包并行 `contract-call fire`（每个钱包唯一 operation-id，如 `mint-w1-<日期>`）。
3. 轮询回执与余额核对（短间隔，不长时间 sleep）；失败的按 [errors.md](errors.md) 对账，不盲目重发。
4. 有顺序依赖（如邀请人先铸）时按依赖分批。

## 写入后对账

见 [errors.md](errors.md)。流水线见 [pipelines.md](pipelines.md)。
