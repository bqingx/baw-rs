# 对标官方 skill：覆盖与差异

## 对标基准

- 官方 skill：已安装 `binance-agentic-wallet` **1.15.0**，标记作者 binance-web3-team，最低CLI版本1.12.0。
- 官方 CLI：本机 `/usr/bin/baw` 的 **1.12.0**；只执行 --help / --version，不读钱包或调用后端。
- 本 skill：`baw-rs` **0.1.0**；帮助来自裁剪后的 baw-rs 0.3.0 二进制，SHA256 见 commands.json。
- 这是已安装快照对标，**不宣称是网上最新版本**；官方更新后需重新对照，不自动照抄新增功能。
- 本文流程为重新编写的中文说明，未复制官方技能正文/脚本。命令参考从 baw-rs 自己的帮助生成。

## 实际枚举

递归枚举官方得到102条叶子命令；本构建得到 **45条**（含主帮助之外的 openfour 与隐藏别名 vaults），
官方叶子命令**刻意缺失 71条**：defi、prediction、tracker、signal、leaderboard、limit-order、x402 已从源码删除，
不是未实现而是不再提供。可重复证据见 [official-comparison.json](official-comparison.json)。
这只验证命令面，不保证所有后端协议完全相同或全部资金路径实盘验证过。

额外14条：serve、contract-call fire、openfour quote-buy/encode-buy、
wallets add/remove/list/status/serve/stop、--wallet NAME 业务命令及 --wallet NAME task arm/disarm/load/status。

## 工作流覆盖

本 skill 覆盖：auth、wallet-view、settings、gas、send、domain-resolve、market-order、approvals、
external-sign 与 Agent ID 专属内盘，均有对应流程/准确参数参考。
另覆盖 Rust 新增路径（--wallet 具名多钱包、wallets 生命周期、task 原生事件任务、typed resident）。
不是仅给一张路由表，具体步骤见 workflows/signing/resident-executor。

## 不能直接照搬的规则

- 普通单钱包要选定wallet-dir与instance身份；路径相同也不能把官方文件会话当成Rust会话。
- 所有受控写操作要求confirm及一次性operation-id；auth也是写操作。ID只用ASCII，不从代币符号生成。
- contract-call/sign-message独立execute要带accept-preview-sha256，不止官方的requestId。
- JSON不是所有入口的可追加开关：wallets控制平面、openfour、serve不接受普通全局--json；--wallet 后的业务命令可以。
- 普通逐笔人工预览流程保留；明确授权自动策略可选择contract-call fire或typed resident，不在触发路径重复做前置读取。
- swap的acknowledge-token-risk不是代币审计，不代表上游security API已跑过。
- 用户条件单不能兜底成立即刻执行（本构建没有限价单入口）；未知结果不换operation-id盲重发。
- 单钱包常驻不等于多钱包后台；`serve`不能悄悄抢走现有daemon持有的钱包锁。
- 同次调用统计的是本地阶段；HTTP耗时不是纯后端时间。少量试次不构成性能下限或并发概率。
- 内盘模板规则应核实目标，不能把“内盘无代币”等旧文档断言写成现状。

## 不包含的副作用

编写/安装本skill不等于部署二进制，不停止daemon、不登录、不发交易、不配置自动买入。
技能新增只增加操作说明；不修改钱包文件、官方baw、其他skill或模型配置。

## 更新

仓库根目录：

```sh
python3 scripts/build_skill_reference.py --binary <VERIFIED_BINARY> --output skills/baw-rs
python3 tests/test_baw_skill.py --binary <VERIFIED_BINARY>
```

逐项对照新帮助与工作流，不把仅同名的版本当成功能一致。发布前检查skill包无钱包路径、密钥与会话内容。
