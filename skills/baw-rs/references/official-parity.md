# 官方 CLI 对照与当前边界

## 基准

- 官方 `@binance/agentic-wallet` CLI 1.12.0；只读取帮助、版本和参数传递代码，不登录或发交易。
- 本 skill 0.2.0；命令快照来自实际 baw-rs Release 产物，版本和 SHA256 见 [commands.json](commands.json)。
- 官方命令快照有 102 条叶子；本构建 51 条。刻意缺失 72 条、Rust 额外 21 条，见 [official-comparison.json](official-comparison.json)。只比较命令面，不代表协议完全等价或实盘通过。
- `task` 是 `pipeline` alias，`vaults` 是 `wallets` alias，不重复计数，也不对应另一套引擎。

## 当前范围

保留登录、查询、转账、OKX 兑换、授权、合约/消息签名、多钱包控制、实例流水线及离线编码。
不追求官方全命令覆盖；DeFi、预测、tracker、signal、leaderboard、limit-order、x402 不属于当前实现。
旧钱包级任务和 typed resident 执行器已移除。

## 关键差异

- 所有钱包作用域命令明确选择钱包；`wallets` 管生命周期，`pipeline/task` 管实例任务，不带 --wallet。
- 业务写操作要求明确确认和唯一 operation ID；外部签名 execute 额外绑定接受的预览 SHA256。
- 交易默认且仅 OKX，无 Binance quote/swap 回退；必须指定配置、精度、滑点及兑换输出底线。
- 官方原生 market-order 有 mev/gasLevel；外部 contract-call 没有这些字段。不能把它们套到 OKX 构造交易路径，也不能仅凭 CLI 判断未公开服务端能力。
- pipeline/task 的确认只覆盖固定配置；恢复不重发、不续期、不沿用变化后的钱包身份。
- `BROADCASTED` 不是链上成功。未知提交不换 ID 盲目重发。
- `serve`/`--warm` 目录访问兼容仍保留，不包含旧任务执行器；不能抢占现役 daemon。

## 更新参考

```sh
python3 scripts/build_skill_reference.py --binary <VERIFIED_BINARY> --output skills/baw-rs
python3 tests/test_baw_skill.py --binary <VERIFIED_BINARY>
```

编写、构建或检查技能不等于授权部署、登录、重启服务或执行交易。源码和帮助验证不代替真实链/钱包验收。
