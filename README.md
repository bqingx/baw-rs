# baw-rs

**Rust 原生实现的币安 Agentic Wallet 客户端：全命令覆盖、原生多钱包、默认拒绝（fail-closed）。**

面向需要在 Linux 上脚本化操作币安 Web3 钱包的人。官方 `baw` 由 Node 实现，本项目用 Rust 从零重写同一套
协议：命令面与官方 1.12.0 对齐，并补上了官方没有的多钱包后台与常驻加速。

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
| **命令面** | 官方 1.12.0 的 102 条叶子命令语法全覆盖，另有 `batch`（只读批处理）、`serve`（常驻）、`contract-call run`（策略约束执行） |
| **交易** | swap、限价单、转账、合约调用与 EIP-712 签名，全部走「预览 → 确认 → 执行」三段式 |
| **查询** | 余额、历史、额度、gas、授权（approvals）、Agent ID、会话到期时间 |
| **DeFi** | 协议与机会发现、持仓、存取、LP、领取 |
| **预测市场** | 类别、市场、持仓、下单、赎回 |
| **其他** | 地址跟踪（含 WebSocket 实时推送）、自定义信号与回测、排行榜、x402 支付 |
| **多钱包** | 一个后台进程管理多个具名钱包，各自独立登录，可并行 |
| **常驻后台** | 后台进程持有全部会话；本机实测单次调用 0.10–0.17 s，与冷启动（0.09 s）接近——省掉了解密开销，但进程启动与本地 IPC 占主要时间 |

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

`vaults` 是独立入口（不需要 `--wallet-dir`）：一个进程管理多个命名钱包，**不**为每个钱包起子进程，
也**不**调用官方 CLI。

```sh
B=./target/release/baw-rs
R=/abs/accounts/registry

# 注册两个已存在的独立钱包目录（各自 0700，各自口令文件 0600）
$B vaults --registry $R register alice --wallet-dir /abs/accounts/alice --instance-id-file /abs/accounts/alice.key
$B vaults --registry $R register bob   --wallet-dir /abs/accounts/bob   --instance-id-file /abs/accounts/bob.key

$B vaults --registry $R list        # 注册表
$B vaults --registry $R serve       # 前台常驻，一个进程服务全部钱包

# 另开终端：每次必须写明钱包名，没有默认值、不回退
$B vaults --registry $R run alice -- wallet status
$B vaults --registry $R run bob   -- wallet address
$B vaults --registry $R status
$B vaults --registry $R stop
```

设计要点：

- **不同钱包并行**：各自独立的客户端、连接池、会话与队列；一个钱包等网络不阻塞另一个。
- **同一钱包串行**：命令按 FIFO 执行，写操作另有互斥锁与意图日志，避免并发覆盖。
- **绝不串号**：名字写错、未登录、目录被替换都直接报错；命令里夹带 `--wallet-dir` / `--instance-id`
  等选择器会被拒绝。
- **注册表是启动快照**：运行期间不能修改，新增钱包需 `stop` → `register` → `serve`。

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
- **文件与网络**：目录 0700、凭据文件 0600、禁符号链接、校验属主；接口域名写死，禁止重定向、代理和
  任意 URL 覆盖。
- **不确定就不重发**：传输层失败返回 `UNCERTAIN` / `retrySafe:false`，必须人工对账后再决定。

## 与官方 CLI 的关系

- 命令名、参数名、默认值按官方 `--help` 逐条对齐，对照表见 [docs/JS-CLI-PARITY.md](docs/JS-CLI-PARITY.md)。
- 两者互不干扰：官方用 `BINANCE_BAW_DIR` 与系统钥匙串，本项目只读自己指定目录下的文件。
- 有意的行为差异（例如余额默认做 RWA 规范化、`settings` 返回无损结构、交易结果区分 SUBMITTED 与 FILLED）
  记录在 [CHANGELOG.md](CHANGELOG.md) 与各 `docs/*.md`。
- `--help` 清单覆盖**不等于**功能等价，也不等于已实盘验证；真实状态见下节。

## 项目状态

- **版本**：0.3.0，主要测试平台 Linux
- **测试**：290 项通过、0 失败（离线：回环 HTTP fixture + 合成会话文件）
- **实盘验证**：完成过一次真实二维码登录，并通过 `status` / `address` / `balance` / `left-quota` /
  `settings` 只读查询（会话有效期实测 48 小时）
- **未验证**：转账、swap、限价单、DeFi 存取、x402 签名、contract-call 执行等**任何真实资金动作**。
  这些路径目前只有离线测试覆盖；首次在真实账户上执行前请自行小额试跑
- **可复现**：`sh scripts/verify.sh` 重跑 fmt / clippy / test / release 并重写 `verification/` 下的
  日志与源码清单；多钱包专项证据在 `verification/multiwallet/`

## 文档

| 文件 | 内容 |
|---|---|
| [docs/AUTH.md](docs/AUTH.md) | 登录、验证、登出与本地存储 |
| [docs/SESSION-LIFECYCLE.md](docs/SESSION-LIFECYCLE.md) | 登录态从生成到失效的全过程 |
| [docs/TRADE.md](docs/TRADE.md) | swap、限价单与风控检查 |
| [docs/SIGNING.md](docs/SIGNING.md) | 合约调用与消息签名的预览/执行绑定 |
| [docs/PORTFOLIO.md](docs/PORTFOLIO.md) | 余额、历史与 RWA 规范化 |
| [docs/MULTIWALLET.md](docs/MULTIWALLET.md) | 多钱包后台、IPC 与并发语义 |
| [docs/JS-CLI-PARITY.md](docs/JS-CLI-PARITY.md) | 与官方 CLI 的命令面逐条对照 |
| [CHANGELOG.md](CHANGELOG.md) | 版本变更与已知缺口 |

## 许可与免责

本仓库**尚未添加 `LICENSE` 文件**。公开分发前需补一个：作者倾向 MIT（宽松）或 AGPL-3.0（修改后须开源），
转载或二次分发请先确认。

本项目为第三方实现，**不提供任何担保**。链上交易不可撤销，代币与合约可能包含恶意逻辑，钱包授权可能被
滥用。是否使用、用到什么程度，以及由此产生的任何损失，由使用者自行承担。
