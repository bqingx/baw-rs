# baw-rs

Rust 原生的多钱包执行客户端：**OKX 构造交易，币安钱包签名/广播，实例级 Task 与流水线调度**。

> 非官方项目，与 Binance 无隶属关系。涉及真实资金；单钱包事件 Task 的旧授权时序及监听前精确授权时序均已各成功验收一次，不代表全部验收。最新“三次核心 HTTP”版也已完成一次明确授权的事件兑换，钱包执行阶段约 950 ms、币安状态读取 0；这不等于完整事件到上链耗时或稳定延迟保证。

## 核心能力

- 多个具名钱包独立登录、独立会话/client/执行队列，不同钱包互不使用全局交易锁。
- 交易仅走 OKX V6 聚合器；输入精度、显式滑点、固定 router/spender、fresh route 最低值及模拟资产变化检查；额外绝对输出底线可选，不从准备期报价冻结。
- 显式精确自动授权：Task 在监听前查额度、必要时授权，只等首个成功回执，不额外等两确认/复读额度；触发后不查额度或补授权。独立 swap 保留原确认/复核流程；未知结果停止。
- 合约 `preview → execute` 分步审核，或明确授权的 `fire` 一次完成必要预览与执行。
- 一个 task 包含多个钱包的动作，支持时间、区块、已出块日志触发；步骤内并行、流水线按完成条件推进。
- Alchemy 接入全实例一个按需 WSS actor：订阅、读取、回执和自动授权共用连接；无 HTTP 回退。
- 持久防重、预览绑定、会话/身份绑定、截止时间与只读对账；显式恢复不重发、不续期。
- [可选固定最低到账](docs/OPTIONAL-MINIMUM.md)：缺省按触发时 fresh route/滑点及模拟净额约束，不填 0/1 wei；显式固定值仍严格绑定，已有任务不改。本轮仅离线，未部署。
- [状态总览与意图摘要](docs/INSPECTION.md)：wallets overview 默认本地，--refresh 显式查询且与 Task 准入隔离；market-order intent/pipeline describe 纯本地，不授予授权或期限。本轮仅离线，未部署。
- [钱包在线接入修复](docs/HOT-ADD.md)：拆分生命周期/注册事务锁，真实 CLI 离线验证新增不重启；删除、重绑定与跨实例抢占仍拒绝。新锁协议须新版 CLI/daemon 配套。
- [可选任务通知](docs/TASK-NOTIFICATIONS.md)：终态保存后独立发送，未知/App/失败/过期分开，持久去重，通知失败不影响交易。默认关闭，需显式私有 PushPlus 配置及渠道；本轮未部署或真实推送。
- [单钱包离线故障验收](docs/SINGLE-WALLET-FAULTS.md)：回复丢失/超时、授权未知和真实 SIGKILL 重启/对账防重；修复 orderId 无 hash 被认定 Task 完成。仅模拟环境，未部署/新增真实交易。
- [兼容壳清理](docs/COMPAT-CLEANUP.md)：已删无效字段/命令及失效 provider，保留加密会话、终态历史核验与真实可用访问模式。最新清理仅离线验证，未切后台。
- [统一 OKX calldata 策略](docs/CALLDATA-POLICY.md)：按用户接受的提供方信任模型删除本地 ABI/selector 解码，保留 envelope、固定路由/金额/模拟/防重，始终不声称本地证明链上接收方/最低值。最新改动仅离线验证，未切后台。
- [通用带税币支持](docs/TAXED-TOKENS.md)：输入/输出税元数据上限分别显式设置，默认 0；不按币名适配，不扩大授权或滑点，以钱包模拟净额验证。最新功能仅离线验收，不保证动态税等任意合约。
- 新增 [事件到执行时序](docs/EVENT-TIMING.md)：区分 WSS 缓冲、规范性读取、准入落盘、调度、钱包排队及动作请求；仅离线验收，不伪造历史/恢复起点，不增加请求或落盘。
- Task 类型化执行计划、每钱包 OKX HTTP pool 复用及分段耗时/请求计数。新 OKX Task 触发后只做路由/Preview/Execute 三次核心 HTTP，状态检查前移且禁止 SDK 补查；本地 gate/模拟/防重及 WSS 事件/回执校验保留。准备及其他动作保留最多 3 秒读集。见 [三次调用约束](docs/THREE-CALL.md) 与 [热路径优化](docs/HOT-PATH.md)。

不追求官方 CLI 全命令覆盖。当前有 51 条可见叶子命令；`task` 是 `pipeline` 的别名，`vaults` 是 `wallets` 的兼容别名，不是独立实现。

## 构建

Linux，Rust stable。仓库不携带预编译产物或钱包凭据。

```sh
cargo build --locked --release --bin baw-rs --bin openfour
./target/release/baw-rs --version
sha256sum target/release/baw-rs
```

`openfour`（four.meme 曲线只读报价与离线编码）是同仓库的独立程序，不需要钱包，不经 baw-rs。
Release 使用优化、Thin LTO 和 strip；Debug 产物含调试信息，不能据其文件大小判断发行版大小或运行内存。
版本号相同不代表构建相同，请同时核对哈希和命令帮助。

## 钱包管理与登录

优先使用已有钱包目录和身份文件，不覆盖、迁移或删除现有会话。以下新建命令仅用于用户明确需要的新钱包。

```sh
B=./target/release/baw-rs
$B wallets add w1
$B wallets add w2
$B wallets list
$B wallets serve                # 前台常驻；在另一终端运行后续命令

$B --wallet w1 auth signin --confirm --operation-id signin-w1-001 --json
# 用户按返回的 urlForWeb / 配对码在币安 App 确认，再使用原 qrCodeId：
$B --wallet w1 auth verify --qrCodeId <QR_ID> --confirm --operation-id verify-w1-001 --json
$B --wallet w1 wallet status --json
$B --wallet w1 wallet address --json
```

每个钱包分别登录。registry 选择顺序为 `--registry`、`BAW_RS_REGISTRY`、`$HOME/wallets/registry`。
钱包名写错、未注册、未登录直接报错，不回退到其他钱包。新增钱包可在首次使用时由 daemon 重新读取 registry。

已有目录通过 `wallets add NAME --wallet-dir ... --instance-id-file ...` 显式接入，保留原身份/会话；旧 register/run 和无效参数已删除，见 [清理范围](docs/COMPAT-CLEANUP.md)。顶层单钱包入口（`--wallet-dir`/`--instance-id`/`--instance-id-file`、warm 开关、顶层 `serve`）已退役：业务命令一律走 `--wallet NAME` 加 `wallets serve`，本地日志用 `wallets journal NAME --list`。详见 [多钱包](docs/MULTIWALLET.md)、[登录](docs/AUTH.md)。

## OKX 报价与兑换

完整配置、授权和保护边界见 [OKX-TRADE.md](docs/OKX-TRADE.md)。配置与凭据须在私有目录，不能放入 Git。

```sh
$B --wallet w1 market-order quote \
  --okx-config /absolute/private/okx.json --binanceChainId 56 \
  --fromToken <INPUT_TOKEN> --toToken <OUTPUT_TOKEN> \
  --fromTokenQty 1 --fromTokenDecimals 18 --slippage 1 --json
```

兑换另需绝对最小输出、风险接受和明确写操作授权；需要时加 `--auto-approve`。默认且唯一 provider 是 OKX，`--provider binance` 拒绝，没有自动择优/失败回退。

**已接受的接口限制**：官方原生币安兑换有 MEV/Gas 档位选项，但 OKX calldata 使用的币安 `contract-call` 公开接口只暴露 `gasLimit`，不暴露 Gas 价格档位或保护广播参数。因此必须显式选择 `--mev false`；不支持的保护/费率请求明确拒绝，不能把 OKX 报价字段当最终广播保证。`gasLimit` 是执行用量上限，不是价格。

## 合约调用

需要在币安 App 启用有效的 Developer Mode。value 使用最小单位 wei，不是人类可读代币数量。

- 分步：`contract-call preview` 后审核内容，`execute --requestId ... --accept-preview-sha256 ...`。
- 一步：已明确授权时 `contract-call fire`，仍进行必要预览、发送方/风险/策略检查。
- 所有写操作要求 `--confirm`、唯一 `--operation-id`；不要重复使用或更换 ID 盲目重发同一意图。
- 授权管理保留 `approvals list/detail/approve/revoke`；无限授权需要额外显式确认，不是默认行为。

参数见实际 `--help` 与 [SIGNING.md](docs/SIGNING.md)。

## Task 与流水线

任务属于实例，不属于钱包。示例：

```text
task1：w1、w2 并行
       ↓ 全部达到完成条件
task2：w2、w3 并行
       ↓ 全部达到完成条件
task3：w3、w4 并行
```

```sh
$B pipeline load --config /absolute/private/pipeline.json
$B pipeline status <PIPELINE_ID>
$B pipeline arm <PIPELINE_ID> --confirm
$B pipeline cancel <PIPELINE_ID>

# task 是相同入口，不带 --wallet：
$B task status <PIPELINE_ID>

# 故障停止后先对账，再显式继续：
$B pipeline reconcile <PIPELINE_ID>
$B pipeline resume <PIPELINE_ID> --confirm
```

- 默认所有成员链上成功后推进；显式 `submitted` 模式不保证链上成功，也不是跨钱包原子交易。
- 步骤须指定正数 `valid_for_ms`；已开始步骤的截止时间不因重启/恢复延长。
- arm 预加载会话并绑定钱包身份，也可能立即执行已明确允许的精确授权。results.phase=WAITING 才是监听就绪；会话、身份或配置变化后不能沿用旧准备记录。
- reconcile 只读取执行/本钱包日志中的 hash 与规范回执。resume 只执行 Pending；未知、失败、取消、过期或旧状态缺少绑定时拒绝。
- 没有 hash、只有 order ID 时继续暂停，不能猜测没提交。目前没有无人值守自动重连/推进。
- 一实例只接受一个活跃 RPC endpoint；不同端点/链的冲突明确报错。

配置参考 [PIPELINES.md](docs/PIPELINES.md) 与 [合成三步示例](examples/pipeline-contract.json)。示例是占位数据，不能直接用于实盘。

**旧接口迁移**：钱包级 `--wallet NAME task ...`、旧日志/pending 任务引擎，以及 typed `prepare/trigger/disarm/native` IPC 已删除，旧帧直接拒绝。不要用旧脚本绕回另一套执行路径。新系统目前支持已出块日志，不包含 pending 交易监听。

## 安全边界

- 会话 AES-256-GCM 加密；identity 是解密凭据，丢失后不能打开对应会话。不要同时泄露两者。
- 私有目录 0700、文件 0600、属主/符号链接/硬链接检查；API origin 固定，限制重定向和代理。
- 提交前持久记录意图；预览绑定执行。响应超时/未知不证明未提交，不自动重发。
- `BROADCASTED`、`SUBMITTED`、接口受理均不等于链上成功，需按 hash/规范回执确认。
- 断线、过载、日志缺口或不支持的请求明确停止。没有无限可靠/无限并发/不会重组的保证。
- 本项目不是资金安全审计、投资建议或生产延迟承诺。

## 验证与文档

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets -- --test-threads=1
python3 tests/test_baw_skill.py --binary target/release/baw-rs
```

旧功能随实现删除的专用测试不再计入；保留普通登录/签名/多钱包和新任务/恢复测试，并验证旧协议拒绝。

- [重构进度](docs/REFACTOR-PLAN.md)
- [本地基准与性能边界](docs/PERFORMANCE.md)：不包含网络、fsync 或链上耗时。
- [OKX 配置与交易](docs/OKX-TRADE.md)
- [余额与历史](docs/PORTFOLIO.md)
- [会话生命周期](docs/SESSION-LIFECYCLE.md)
- [Agent Skill](skills/baw-rs/SKILL.md)：实际帮助快照与中文版操作流程。

已有 w1 登录、报价、RPC 检查和一次旧授权时序的真实单钱包事件兑换；新监听前准备时序、多钱包/多步骤与端到端延迟仍待验收。任何新的链上写操作或任务窗口需明确授权。构建不自动安装二进制、切换现役服务或改变登录态；当前修复见 [Release 检查点](docs/RELEASE-CHECKPOINT.md)。

## 许可

MIT，见 [LICENSE](LICENSE)。非 Binance 官方产品，不保证资金安全、收益、服务端可用性或协议长期兼容。
