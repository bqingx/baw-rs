# 单用户部署：删除无效兼容壳

本轮按用户要求直接清理源码，不维护已废弃入口；只离线测试/构建，不切后台、不修改真实钱包/会话、不开交易任务。

## 已删除

- `--okx-allow-opaque-calldata`：不再作为无效开关接受。所有 OKX calldata 统一 opaque＋强制模拟。
- route builder 的旧 JSON 字段 `allowed_selectors`、`require_runtime_preview`、`auto_approve`、`prepare_at_load`：删除字段/默认值/对应生产代码输出；严格配置遇到它们会拒绝，不再 silently ignore。
- 隐藏命令 `wallets register`、`wallets run`：命令树和 dispatch 分支删除，而不是保留迁移错误壳。
- `market-order list`、其 ListArgs 和失效分支；历史查询使用 `wallet tx-history`/交易 hash。
- 程序内部 `Provider::Binance`、无调用意义的 provider 辅助函数和只返回拒绝错误的 trade::run。实际交易只剩 OKX；`--provider okx` 仍是当前可用选项。
- 旧 Rust 全局 `set_skip_prechecks`/`apply_skip_prechecks` 和额外全局状态。保留当前请求 task-local 策略和服务启动默认值；不削弱 OKX/Task 禁止 skip-prechecks 的约束。

## 功能合并而非破坏数据

已有钱包统一通过当前 `wallets add` 接入：

```sh
baw-rs wallets add alice \
  --wallet-dir /absolute/existing-wallet \
  --instance-id-file /absolute/existing-identity
```

仅当两个路径都显式给出且都已存在时，走现有目录/身份的严格检查并登记，不生成身份、不覆写 session、不解密或联网。其他路径创建仍按原 add 规则；部分已有/冲突不自动补全或覆盖。默认 remove 保留数据，之后可用同样的显式 add 接回；删除仍需 --delete-data --confirm。

日常业务执行使用 `baw-rs --wallet alice ...`，不再提供重复的隐藏 run 包装。

## 明确保留

- 普通单钱包目录冷/暖访问、`serve`/`--warm`：仍是实际可用的访问模式，共用同一 resident 实现，不是被删除的旧任务引擎。
- 官方加密会话及 UUID 读取方式：不为了清代码强迫现有身份/会话迁移或重新登录。
- 历史已提交交易的 sender/回执只读核验、旧截止/绑定不足时拒绝续跑。
- `task/pipeline`、`vaults/wallets`：用户使用的当前别名，没有第二套引擎。

终态 COMPLETED/CANCELLED 的 Task 归档仍可查看：读取检查 ID、链、触发约束、拓扑和快照，不把归档 argv 当成当前可执行 CLI。新 load 和非终态继续执行仍严格解析当前命令；不存在借归档恢复已删命令的执行路径。取消/完成任务也不能 arm/resume。

移除无效参数改变了意图字段集合，当前 `calldataPolicy=pinned_okx_opaque_v3` 同时绑定新参数 schema。旧准备记录不能当作新记录继续监听/执行，不能静默升级授权或期限。

## 验证

新回归覆盖删除项解析/严格配置拒绝、采用已有钱包前后身份/session bytes 不变、终态归档可读/新定义拒绝旧语法，以及原多钱包隔离、相同钱包 FIFO、热加载、登录、取消/防重、3 核心 HTTP。测试 fixture 和 smoke/verify 脚本已改用当前 add/--wallet 入口；没有通过删除相关功能性验证来掩盖行为变化。
