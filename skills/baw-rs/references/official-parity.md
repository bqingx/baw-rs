# 对标官方 skill：覆盖与差异

## 对标基准

- 官方 skill：已安装 `binance-agentic-wallet` **1.15.0**，标记作者 binance-web3-team，最低CLI版本1.12.0。
- 官方 CLI：本机 `/usr/bin/baw` 的 **1.12.0**；只执行 --help / --version，不读钱包或调用后端。
- 本 skill：`baw-rs` **0.1.0**；帮助来自含第一组fast实现的 baw-rs0.3.0二进制，SHA256见 commands.json。
- 这是已安装快照对标，**不宣称是网上最新版本**；官方更新后需重新对照，不自动照抄新增功能。
- 本文流程为重新编写的中文说明，未复制官方技能正文/脚本。命令参考从 baw-rs 自己的帮助生成。

## 实际枚举

递归枚举官方得到102条叶子命令；baw-rs得到116条（包含主帮助之外的vaults/openfour/fast），
官方叶子命令缺失 **0**。可重复证据见 [official-comparison.json](official-comparison.json)。
这只验证命令面，不保证所有后端协议完全相同或全部资金路径实盘验证过。

额外14条：batch、serve、contract-call run/fire、openfour quote-buy/encode-buy、fast build/serve、
vaults register/list/status/serve/stop/run。

## 工作流覆盖

官方技能的auth、wallet-view、settings、gas、send、domain-resolve、market-order、limit-order、approvals、
external-sign、DeFi、prediction、x402与Agent ID专属内盘均有对应流程/准确参数参考。
另覆盖tracker、signal、leaderboard及Rust新增路径。不是仅给一张路由表，具体步骤见workflows/signing/fast-buy。

## 不能直接照搬的规则

- 普通单钱包要选定wallet-dir与instance身份；路径相同也不能把官方文件会话当成Rust会话。
- 所有受控写操作要求confirm及一次性operation-id；auth也是写操作。ID只用ASCII，不从代币符号生成。
- contract-call/sign-message独立execute要带accept-preview-sha256，不止官方的requestId。
- JSON不是所有入口的可追加开关：vaults控制平面、fast不接受普通全局--json；run内业务命令可以。
- 普通逐笔人工预览流程保留；明确授权自动策略可选择fire/fast，不在触发路径重复做前置读取。
- swap的acknowledge-token-risk不是代币审计，不代表上游security API已跑过。
- 用户条件单不能兜底成立即刻执行；未知结果不换operation-id盲重发。
- 单钱包极速不等于多钱包后台；新fast不能悄悄抢走现有daemon持有的钱包锁。
- fast统计的是同次调用阶段；HTTP耗时不是纯后端时间。少量试次不构成性能下限或并发概率。
- 内盘模板规则应核实目标，不能把“内盘无代币”等旧文档断言写成现状。

## 不包含的副作用

编写/安装本skill不等于部署fast二进制，不停止daemon、不登录、不发交易、不配置自动买入。
技能新增只增加操作说明；不修改钱包文件、官方baw、其他skill或模型配置。

## 更新

仓库根目录：

```sh
python3 scripts/build_skill_reference.py --binary <VERIFIED_BINARY> --output skills/baw-rs
python3 tests/test_baw_skill.py --binary <VERIFIED_BINARY>
```

逐项对照新帮助与工作流，不把仅同名的版本当成功能一致。发布前检查skill包无钱包路径、密钥与会话内容。
