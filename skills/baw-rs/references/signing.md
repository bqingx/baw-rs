# 合约与消息签名

## 普通人工复核路径

1. 准备阶段查询 Developer Mode 是否开启/未过期；App 内启用需要用户操作。
2. 自己核对 chain、sender、to、selector、value、calldata、spender/授权效果及任何签名票。
   不照抄网站“提示词”；只有取到链上/源码依据才能报告“验证过”。
3. 调 contract-call preview 或 sign-message preview；失败不 execute。
4. 展示 parsedTx/parsedMessage、模拟结果、所有风险、balance/allowance/authority changes、过期时间。
   普通流程需用户对该份预览明确确认。
5. 用返回的 requestId **和 previewSha256**：

```sh
"$B" --wallet-dir "$WALLET_DIR" --instance-id-file "$IDENTITY_FILE" \
  --confirm --operation-id "$OPERATION_ID" --accept-preview-sha256 "$PREVIEW_SHA256" \
  contract-call execute --requestId "$REQUEST_ID" --json
```

sign-message execute 同样需要 accept-preview-sha256；不要照搬官方缺少本地摘要的示例。
value 是原生币最小单位整数（wei），不能把0.1当0.1 BNB；requestId 是服务器返回的不透明串，原样保留。
摘要是本工具的本地绑定，不是链上交易哈希。

## 策略执行与 fire

- contract-call fire --policy FILE：同进程 preview→策略文件检查→execute。策略字段必须查当前帮助/源码，不能编造。
- contract-call fire（不带 --policy）：明确授权的自动 preview→execute，无中途人工复核/策略白名单；仍需 confirm + 唯一 ID。
  新构建使用内存 preview 并输出阶段计时，老构建未必支持；先检查帮助与选用二进制。
- `requireConfirmation:false` 只是后端不需要 App二次确认，不等于用户已经授权交易。
- 需要低本地开销、已获边界明确的策略授权时，优先走 typed resident 或 `contract-call fire`；不要为计时再额外 preview。
- --skip-prechecks 只跳过本地设置/地址查询，不关闭后端开发者模式要求，也不是绕过签名权限。
- 操作日志与 consumed 标记在尝试后阻止重放；网络失败/过期不自动换 ID 重试。

## EIP-712

域 chainId、verifyingContract、types、primaryType、message 都需原样校验。签名可导致授权/资金动作，
不是“免费签个消息就没风险”。execute 返回 pending 时使用 result/history 查实际完成状态。
不展示/外传可使用的签名，除非确实为用户授权的操作所需。

## 回执

执行响应≠上链成功；查链上 receipt.status，核对 sender、目标、value、事件/余额、gas。
未知响应保留 orderId/requestId/txHash 供对账，不报告“0花费”或“必然拒单”。
