# 地址与单位

| 参数 | 单位 | 例 |
|---|---|---|
| `market-order --fromTokenQty` | 人类可读数量（配 `--fromTokenDecimals` 精确换算，不用浮点） | `1.25` + `18` → `1250000000000000000` |
| `market-order --slippage` | 百分数 | `10` = 10%，最多 8 位小数，<100 |
| `--max-from-tax-percent` / `--max-to-tax-percent` | 百分数；OKX `taxRate=0.02` 即 2% | 默认 `0` = 不接受带税币 |
| `--okx-min-output-units` | 输出代币最小单位整数 | 可选，不填 0/1 |
| `wallet send --amount` | 人类可读数量（以帮助为准） | `0.05` |
| `contract-call --value` | 原生币最小单位整数（wei） | 0.01 BNB = `10000000000000000` |
| `contract-call --gasLimit` | Gas 用量上限，不是 Gas 价格 | 仅用户指定时传 |
| `openfour quote-buy --amount`（独立程序） | 人类可读 BNB | `0.1` |
| `openfour encode-buy --amount-wei` / `--min-out-raw` | 最小单位整数 | |

## 地址

- EVM 原生币在 OKX 路径用 `0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee`（零地址也接受并规范化），精度必须 18。原生 BNB 不是 WBNB。
- 区分 chainId、代币合约、钱包地址；同名 ticker 不能跨链复用地址。地址只来自用户确认的输入或可信查询，展示时给完整地址。
- 支持的链以 `wallet chains` 为准；OKX 兑换目前限 EVM 链 `1 / 56 / 8453 / 4663`、exact-in。
- 计算用整数/Decimal，不转 float；四舍五入只用于展示，不改交易参数。
- 本技能不维护"安全币列表"；查币用已授权的只读来源，只陈述事实，不给投资建议。
