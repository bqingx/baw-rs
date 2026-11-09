# 合约调用与消息签名

签名在币安后端完成，本地没有私钥。每次写操作前都会核对开发者模式（未开或过期就失败，需用户在币安 App 打开）和该链的实际发送地址。

## 分步：preview → 人工确认 → execute（默认）

```sh
"$B" --wallet "$NAME" contract-call preview --binanceChainId 56 \
  --from "$ADDR" --to "$TARGET" --value 0 --inputData 0x... --json
```

向用户展示：`parsedTx`、模拟结果、全部 `risks`、余额/授权（allowance）/authority 变化、过期时间。
用户确认后：

```sh
"$B" --wallet "$NAME" --confirm --operation-id "$OP" \
  --accept-preview-sha256 "$PREVIEW_SHA256" \
  contract-call execute --requestId "$REQUEST_ID" --json
```

- `previewSha256` 和 `requestId` 都来自 preview 输出，原样传回。摘要是本地审核绑定，不是交易哈希。
- 预览最长 300 秒（受开发者模式到期等进一步收紧），只能执行一次：执行失败、换 operation-id 都不能再用同一预览。参数变了就重新 preview。
- 模拟失败或后端报错时不 execute。

## 一步：`contract-call fire`

```sh
"$B" --wallet "$NAME" --confirm --operation-id "$OP" contract-call fire \
  --binanceChainId 56 --from "$ADDR" --to "$TARGET" --value 0 --inputData 0x... --json
```

- 同进程 preview + execute，没有中途人工审核；仅用于用户已明确授权边界的自动动作（抢铸、事件触发、流水线 action）。
- 输出仍带 `risks`、`authorityChanges`、`allowanceChanges`，事后要检查并报告。
- `--policy FILE`：钉死 `chainId`、`to`、`valueWei`，可选 `selector`、`calldataSha256`、`maxGasLimit`，以及 `allowRisks`、`allowAuthorityChanges`；不符即中止。字段以仓库 `docs/SIGNING.md` 和源码为准，不要编造。
- `--skip-prechecks` 只省掉开发者模式/地址两次只读检查并信任 `--from`；后端仍要求开发者模式。OKX 兑换和流水线拒绝它。
- 多钱包：不同钱包各发一条 fire 可并行；同一钱包在 daemon 内排队。

## 消息签名

`sign-message preview` → 展示 → `sign-message execute`（同样需要 `--accept-preview-sha256`）。返回 pending 时用 `sign-message result --order-id ID` / `history` 查。
EIP-712 要核对 domain 的 `chainId`、`verifyingContract`、`primaryType` 与 message；签名可能等同授权或转账，不是"免费无风险"。
`result` 会返回签名本身：不要写进日志或公开报告。

## 限制

- 外部合约调用路径没有 MEV 保护和 Gas 价格档位；`gasLimit` 只是用量上限。
- `requireConfirmation:false` 只表示后端不需要 App 二次确认，不等于用户已授权。
- `BROADCASTED` 不是链上成功；以回执 `status`、发送方、事件和余额变化为准。
