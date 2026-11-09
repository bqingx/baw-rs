# 错误、未知结果与对账

## 判定结果

同时看三样：退出码、信封 `success`、`data` 里的业务状态。退出 0 / HTTP 200 都不能单独证明成交。

| 现象 | 含义 | 下一步 |
|---|---|---|
| 本地参数/权限/锁错误（未联网就失败） | 什么都没发出 | 修正参数后可重试（新 operation-id） |
| 广播前后端明确拒绝（风控、余额、开发者模式、限额） | 未提交 | 说明原因；用户同意后重新报价/预览 |
| `BROADCASTED` / `SUBMITTED` + txHash | 已提交，未必成功 | 查回执 |
| 只有 orderId、没有 txHash | 状态未知 | 对账；不能当成已上链，也不能当成没提交 |
| `UNCERTAIN`、超时、连接断开、进程被杀 | 状态未知 | 对账，**禁止重发** |
| 回执 `status=0x1` | 执行成功 | 再核实际到账与 Gas |
| 回执 `status=0x0` | 上链失败，仍扣 Gas | 报告 Gas 损失，分析原因 |

## 对账步骤

```sh
"$B" --wallet "$NAME" wallet operation --operation-id "$OP" --json   # 本地日志，不联网、未登录也能查
"$B" --wallet "$NAME" wallet operation --list --json
"$B" --wallet "$NAME" wallet tx-history --json
"$B" --wallet "$NAME" wallet tx-lock --json
```

日志 `verdict`：
- `UNCERTAIN`：停在 `SUBMITTING`，可能已提交。用 `recovery.txHash` / tx-history / 浏览器核对。
- `RESPONSE_RECEIVED`：拿到过响应，`recovery` 里可能有 `txHash` / `orderId` / `status`；仍需核链上结果。
- `FAILED_OR_UNKNOWN`：后端拒绝或传输失败；确认无链上效果前不谈重试。
- `progress.stage`（兑换多阶段）：`approval_*` / `swap_*` 显示停在授权还是兑换阶段，对应 `approvalTxHash` / `swapTxHash`。
- `retrySafe` 恒为 false：日志只能证明"可能已提交"，不能证明"没提交"。

## 规则

1. 旧 operation-id 永远不复用；换新 ID 也**不能**保证不重复交易——先确认没有链上效果。
2. 错误原文去掉秘密后照实报告；没有依据不要猜错误码含义。
3. 会话过期、identity 不符、权限错误：停下报告，禁止删除/覆盖 session、换钱包兜底。
4. 流水线用 `pipeline reconcile` → `pipeline resume --confirm`，见 [pipelines.md](pipelines.md)。
5. 报盈亏：列买入本金、实际回款、买卖与授权 Gas，用链上整数计算；整钱包余额差只有排除同期其他转账后才能当盈亏。

对外只展示公开地址、状态、orderId/txHash；不输出会话标识、签名或凭据。
