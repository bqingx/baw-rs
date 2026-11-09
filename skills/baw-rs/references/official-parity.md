# 与官方 baw CLI 的差异

## 基准

- 对照官方 `@binance/agentic-wallet` CLI 1.12.0（只读其帮助与参数代码）。
- 本构建的叶子命令数、版本与二进制 SHA256 见 [commands.json](commands.json)；逐项缺失/额外见 [official-comparison.json](official-comparison.json)。只比较命令面，不代表协议完全等价。
- `task` 是 `pipeline` 的别名，`vaults` 是 `wallets` 的别名，不重复计数。

## 范围

保留：登录、查询、转账、OKX 兑换、授权管理、合约/消息签名、多钱包管理、实例流水线、four.meme 只读报价/离线编码。
刻意不做：DeFi、预测市场、tracker、signal、leaderboard、limit-order、x402。

## 关键差异

| 方面 | 官方 baw | baw-rs |
|---|---|---|
| 钱包选择 | 默认会话 | 每条命令显式 `--wallet NAME`，无默认、无回退 |
| 多钱包 | 一个实例一个钱包 | 一个 daemon 管多个钱包，各自独立登录、并行执行 |
| 兑换来源 | 币安原生 market-order | 仅 OKX 聚合器 → 币安 contract-call；无币安报价回退 |
| MEV / gasLevel | 原生兑换支持 | contract-call 路径不支持，传入非默认值即报错 |
| 写操作 | 直接执行 | 需 `--confirm` + 唯一 `--operation-id`，持久防重 |
| 外部签名 execute | requestId | 另需 `--accept-preview-sha256` 绑定已审核预览 |
| 自动化 | 无 | `contract-call fire`、实例级流水线（时间/区块/事件触发） |
| 未知结果 | — | 暂停、保留 orderId/txHash 对账，绝不自动重发 |

## 更新参考

```sh
python3 scripts/build_skill_reference.py --binary <VERIFIED_BINARY> --output skills/baw-rs
python3 tests/test_baw_skill.py --binary <VERIFIED_BINARY>
```

生成或检查技能不等于授权部署、登录、重启服务或交易。
