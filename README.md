# baw-rs

**Rust 原生实现的币安 Agentic Wallet 客户端：全命令覆盖、原生多钱包、默认拒绝（fail-closed）。**

面向需要在 Linux 上脚本化操作币安 Web3 钱包的人。官方 `baw` 由 Node 实现，本项目用 Rust 从零重写同一套
协议：命令面与官方 1.12.0 对齐，并补上了官方没有的多钱包后台与常驻服务。

> ⚠️ **非官方项目**，与 Binance 无隶属关系，协议通过逆向已安装的 npm 包还原。
> 涉及真实资金，使用前请读[安全模型](#安全模型)与[项目状态](#项目状态)。

## 目录

- [特性](#特性)
- [快速开始](#快速开始)
- [多钱包](#多钱包一个后台管全部)
- [安全模型](#安全模型)
- [与官方 CLI 的关系](#与官方-cli-的关系)
- [项目状态](#项目状态)
- [文档](#文档)
- [许可与免责](#许可与免责)

## 特性

| | |
|---|---|
| **命令面** | 45 条叶子命令：`auth`、`wallet`、`market-order`、`contract-call`、`sign-message`、`approvals`、`task`（事件任务）、`wallets`（钱包生命周期与控制面）、`cli-check`、`skill-check`、`serve`，另有独立入口 `openfour`（`quote-buy` / `encode-buy`） |
| **交易** | 写操作要求显式授权；普通外部签名支持「预览 → 人工确认 → 执行」。`contract-call fire` 是显式授权的自动预览→执行，没有中途人工审核 |
| **查询** | 余额、历史、额度、gas、授权（approvals）、Agent ID、会话到期时间 |
| **多钱包** | 一个后台进程管理多个具名钱包，各自独立登录，可并行；钱包名是全局参数 `--wallet <NAME>` |
| **常驻后台** | 复用会话和 HTTP Client：单钱包 `serve`（配 `--warm` 走本地 socket），多钱包 `wallets serve`，每笔不必重启进程。性能以同次调用计时与可复现测试为准，不宣称生产延迟下限 |

## 离线编码与低本地开销路径

离线 `openfour encode-buy` 不联网、不需要会话：精确 uint256、必填绝对最低数量。
只支持已知 BNB/WBNB options=0 模板；无 quote **不是百分比滑点保护**。
日常多钱包继续使用 `--wallet <NAME>`。测试仅使用合成数据与本地 mock，不包含任何真实下单。

通用 EVM 固定调用的低本地开销路径见 [typed resident 执行器](docs/resident-executor.md)：
同用户持久 socket 的 `prepare → trigger / disarm`，固定参数、钱包预约、提交前截止时间和持久防重。
这不是新增 CLI 子命令；代码接口不等于已部署或实盘验收。
基准命令与指标口径见该文，本文不提供未经实测的加速数字。

自动触发合约调用见 [原生任务](docs/native-event-tasks.md)：常驻订阅 WS 已出块日志，或显式选择
**待处理交易**，按发送方/目标/方法/静态参数匹配后执行任意已授权调用。
**必须填写有效时长 `valid_for_ms`，从启用起算，无固定五分钟上限**；不再让用户填写截止日期。
支持固定调用、受控字段绑定和触发后实时路由，无外部监听脚本依赖。旧日志版已实跑，
本次新增时长/pending能力尚未切换运行服务；测试结果、节点可见性与断线限制见该文。

## Agent Skill

面向 AI Agent 的中文版操作技能：[`skills/baw-rs/SKILL.md`](skills/baw-rs/SKILL.md)。
覆盖本仓库实际提供的 45 条叶子命令（已裁剪官方 baw 中的 DeFi、预测、跟踪、信号、榜单、限价与 x402 面），
并记录 Rust 新增的多钱包、原生事件任务与离线编码路径。
附实际 `--help` 参数快照、登录/交易/签名/对账流程和仅离线的校验工具，不携带任何登录态。

```sh
python3 tests/test_baw_skill.py --binary <VERIFIED_BINARY>
```

## 快速开始

需要 Rust 稳定版工具链（开发时用 1.99.1）。**仓库不发布预编译二进制**——只有你自己构建的产物，
哈希才可核对。

```sh
git clone https://github.com/bqingx/baw-rs.git
cd baw-rs && cargo build --release     # 产物：target/release/baw-rs
```

### 1. 准备钱包目录与加密口令

```sh
umask 077
mkdir -m 700 ~/.baw
printf '%s' 'your-passphrase' > ~/.baw-instance.key   # 0600
```

这个口令文件是登录态的加密依据：**丢了会话就永久打不开**。详见[安全模型](#安全模型)。

### 2. 登录

```sh
B=./target/release/baw-rs
W="--wallet-dir $HOME/.baw --instance-id-file $HOME/.baw-instance.key"

$B $W --confirm --operation-id login-1 --json auth signin
# → {"urlForWeb":"https://app.binance.com/uni-qr/xxxx","qrCodeId":"...","pairingCode":"abc123","expireAt":...}
```

把 `urlForWeb` 渲染成二维码（例如 `qrencode -o qr.png '<urlForWeb>'`）用手机确认，然后：

```sh
$B $W --confirm --operation-id login-2 --json auth verify --qrCodeId <上一步的 qrCodeId>
```

`verify` 每 2 秒轮询一次，最长 5 分钟。成功即完成落盘。

### 3. 日常使用

```sh
$B $W --json wallet status      # CONNECTED / UNCONNECTED
$B $W --json wallet address     # 多链地址
$B $W --json wallet balance     # 全部链资产
$B $W --json wallet settings    # 会话到期时间、日限额、开发者模式

# 只询价，不下单
$B $W --json market-order quote --binanceChainId 56 \
   --fromToken <代币地址> --toToken <代币地址> --fromTokenQty 1
```

写操作（swap / 转账 / 签名执行 / signin / signout）**必须**同时给 `--confirm` 和唯一的 `--operation-id`：

```sh
$B $W --confirm --operation-id send-1 --json wallet send \
   --binanceChainId 56 --tokenAddress <代币地址> --recipient <收款地址> --amount 0.01
```

## 多钱包：一个后台管全部

钱包名是**全局参数**：每条钱包作用域命令都写 `--wallet <NAME>`；管理子命令 `wallets`
（增删、列表、状态、常驻）**不需要**钱包名。一个进程管理多个命名钱包，**不**为每个钱包起子进程，
也**不**调用官方 CLI。

```sh
B=./target/release/baw-rs
R=$HOME/wallets/registry
export BAW_RS_REGISTRY=$R      # 也可每次传全局 --registry

# 生命周期：add 创建 0700 目录 + 0600 identity 并注册；已有目录用 register
$B wallets add alice
$B wallets add bob
$B wallets list
$B wallets serve               # 前台常驻，一个进程服务全部钱包

# 另开终端：每次必须写明钱包名，没有默认值、不回退
$B --wallet alice wallet status
$B --wallet bob wallet address
$B --wallet alice auth signin --confirm --operation-id alice-signin-001
$B --wallet alice task arm sell-1 --confirm      # 原生事件任务
$B wallets status
$B wallets remove bob                            # 默认保留数据；--delete-data --confirm 才删
$B wallets stop
```

设计要点：

- **不同钱包并行**：各自独立的客户端、连接池、会话与队列；一个钱包等网络不阻塞另一个。
- **同一钱包串行**：普通命令按钱包锁串行，写操作另有互斥锁与意图日志；typed 任务预约期间，
  同钱包普通命令（包括查询）拒绝 BUSY，typed 锁争用不排队。持久连接内响应逐请求串行。
- **绝不串号**：名字写错、未登录、目录被替换都直接报错；`--wallet` 模式里夹带
  `--wallet-dir` / `--instance-id` / `--warm` 会被拒绝。
- **注册表是启动快照**：运行期间不能修改，新增钱包需 `wallets stop` → `wallets add` → `wallets serve`。
- **registry 位置**：全局 `--registry`，否则 `$BAW_RS_REGISTRY`，否则 `$HOME/wallets/registry`。

兼容旧形式（语义不变，现役脚本可直接继续跑）：

- 传统单钱包：`--wallet-dir ... --instance-id-file ...`，`serve` 与 `--warm` 仍走
  `<wallet-dir>/.baw-rs.sock`。
- 旧命名形式：`vaults --registry R run NAME -- <命令>`；`vaults` 是 `wallets` 的隐藏别名，
  `register`、`run`、`task NAME` 仍解析。

完整语义见 [docs/MULTIWALLET.md](docs/MULTIWALLET.md)。

## 安全模型

原则：**默认拒绝，出错就停，能少给的权限绝不多给。**

- **口令即凭据**：登录态用 AES-256-GCM 加密，密钥只由固定口令经 PBKDF2-HMAC-SHA256（10 万轮）推导。
  不读主机名、网卡、机器码、系统钥匙串或 TPM —— 所以目录可以整体搬到另一台机器，配同一口令即可解密。
  代价是口令丢失即永久打不开，口令与 `session.json` 同时泄露即等于登录态泄露。推荐 `--instance-id-file`
  而不是命令行明文（后者会进 `ps` 与 shell 历史）。
- **写操作三重门**：`--confirm` + 一次性 `--operation-id` + 提交前的意图日志。用过的 ID 永久拒绝，
  崩溃后也不复用。`RESPONSE_RECEIVED` 只代表拿到接口响应，**不代表成交**。
- **签名钉住预览**：`contract-call execute` / `sign-message execute` 还需 `--accept-preview-sha256`，
  把执行绑定在你确认过的那份预览上，且只能消费一次。
- **默认校验发送方**：每次预览前都会核对 `--from` 与本钱包在该链上的真实地址，不一致直接报错不发请求。
  显式传 `--skip-prechecks` 可移除这道检查（连同开发者模式检查）换取约 0.95 秒，此时 `--from` 必填，
  写错地址由服务端报错 —— 这是本工具唯一一个「减少检查」的开关，默认关闭。
- **文件与网络**：目录 0700、凭据文件 0600、禁符号链接、校验属主；接口域名写死，禁止重定向、代理和
  任意 URL 覆盖。
- **不确定就不重发**：传输层失败返回 `UNCERTAIN` / `retrySafe:false`，必须人工对账后再决定。

## 与官方 CLI 的关系

- 保留命令面的命令名、参数名、默认值按官方 `--help` 逐条对齐；官方完整帮助快照见
  [docs/JS-CLI-PARITY.md](docs/JS-CLI-PARITY.md)（其中已裁剪的命令仅作官方语法参考，本仓库不再实现）。
- 两者互不干扰：官方用 `BINANCE_BAW_DIR` 与系统钥匙串，本项目只读自己指定目录下的文件。
- 有意的行为差异（例如余额默认做 RWA 规范化、`settings` 返回无损结构、交易结果区分 SUBMITTED 与 FILLED）
  记录在 [CHANGELOG.md](CHANGELOG.md) 与各 `docs/*.md`。
- `--help` 清单覆盖**不等于**功能等价，也不等于已实盘验证；真实状态见下节。

## 项目状态

- **版本**：0.3.0，主要测试平台 Linux
- **测试**：220 项通过、0 失败；另有 1 项本地基准默认忽略（离线：回环 HTTP fixture + 合成会话文件）
- **实盘验证**：完成过一次真实二维码登录，并通过 `status` / `address` / `balance` / `left-quota` /
  `settings` 只读查询（会话有效期实测 48 小时）
- **未验证**：转账、swap、approvals 撤销、contract-call / sign-message 执行等**任何真实资金动作**。
  这些路径目前只有离线测试覆盖；首次在真实账户上执行前请自行小额试跑
- **可复现**：`sh scripts/verify.sh` 重跑 fmt / clippy / test / release 并重写 `verification/` 下的
  日志与源码清单；多钱包专项证据在 `verification/multiwallet/`

## 文档

| 文件 | 内容 |
|---|---|
| [docs/AUTH.md](docs/AUTH.md) | 登录、验证、登出与本地存储 |
| [docs/SESSION-LIFECYCLE.md](docs/SESSION-LIFECYCLE.md) | 登录态从生成到失效的全过程 |
| [docs/TRADE.md](docs/TRADE.md) | swap 与风控检查 |
| [docs/SIGNING.md](docs/SIGNING.md) | 合约调用与消息签名的预览/执行绑定 |
| [docs/PORTFOLIO.md](docs/PORTFOLIO.md) | 余额、历史与 RWA 规范化 |
| [docs/MULTIWALLET.md](docs/MULTIWALLET.md) | 多钱包后台、IPC 与并发语义 |
| [docs/resident-executor.md](docs/resident-executor.md) | 通用 EVM typed resident 协议、预约/期限/防重、离线基准 |
| [docs/JS-CLI-PARITY.md](docs/JS-CLI-PARITY.md) | 官方 `baw` 帮助快照（含已裁剪命令的原始语法，仅作参考） |
| [CHANGELOG.md](CHANGELOG.md) | 版本变更与已知缺口 |

## 许可与免责

本项目以 **MIT 许可证**发布，全文见 [LICENSE](LICENSE)。转载、修改、二次分发都可以，保留版权与许可声明即可。

选择 MIT 而不是 AGPL 的理由：这个项目是钱包客户端，可能被改动后重新分发，任何许可证都挡不住有人拿它做坏事；
能降低的是别人想读源码核对时的门槛。宽松许可让审计和复用没有摩擦，剩下的风险由上面的免责声明和三重门来承担。

本项目为第三方实现，**不提供任何担保**。链上交易不可撤销，代币与合约可能包含恶意逻辑，钱包授权可能被
滥用。是否使用、用到什么程度，以及由此产生的任何损失，由使用者自行承担。
