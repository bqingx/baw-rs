# baw-rs 0.3.0

用 Rust 原生实现的币安 Agentic Wallet 命令行工具，协议对齐已安装的 `@binance/agentic-wallet`
1.12.0。命令面与官方 JavaScript CLI 对齐：官方 102 条叶子命令全部覆盖，另有 `batch`、
`serve`、`contract-call run` 三条本实现独有的辅助命令。它不调用、也不替换系统里已安装的
`baw`，两者互不影响。测试平台为 Linux。

仓库不提供预编译二进制——只有你自己构建出来的产物，哈希才是你可以核对的。`scripts/verify.sh`
可一键重跑全部门禁并重建 `verification/` 下的证据（fmt、clippy、test、release 构建、源码清单、
叶子命令清单）。

## 构建与选择钱包

```sh
cargo build --release
./target/release/baw-rs \
  --wallet-dir /absolute/wallet-directory --instance-id EXACT-ENCRYPTION-IDENTITY --json wallet status
```

两个选择器都必须显式给出，可写在子命令前，也可用环境变量 `BINANCE_BAW_DIR`、`BINANCE_INSTANCE_ID`。
没有默认钱包、没有基于 MAC 的加密身份、没有系统钥匙串读取、也不会回退到别的会话。instance 必须
和被选目录里加密时使用的那一个完全一致。已加密的会话与旧版明文 client UUID 都能读，读取路径不会
做迁移写入。只有显式 `auth signin` 才可能在空目录里创建新身份。

**优先用 `--instance-id-file` 而不是 `--instance-id`**：写在命令行上的口令会进入 `ps` 输出和
shell 历史。

```sh
umask 077
printf '%s' 'your passphrase' > ~/.baw-instance.key     # 0600，结尾不需要换行
./baw-rs --wallet-dir /abs/wallet --instance-id-file ~/.baw-instance.key wallet status
```

该文件必须是 0600、属主本人、单硬链接的普通文件（禁止符号链接，≤4 KiB）；结尾一个换行会被忽略。
两种选择器解析出同一个字符串、因而同一把密钥，所以已有钱包切换到文件方式不需要重新登录。同时给
两种来源会直接报错，不做静默优先。

### `--instance-id` 是凭据，不是机器身份

`session.json` 的 AES-256-GCM 密钥只由这个字符串推导：

```text
PBKDF2-HMAC-SHA256(password = instance, salt = "baw-agentic-wallet-v1:" + instance, 10 万轮)
```

没有钥匙串、没有硬件根、没有逐主机秘密。后果全部是 fail-closed：

- **instance 字符串丢失 → 会话永久无法解开。** 没有任何途径能重新推导出密钥。
- **instance 字符串 + `session.json` 同时泄露 → 登录 cookie 可被任何人解开。**
  请把它当密码对待：放在 0600 文件或环境变量里，不要留在 shell 历史或会进 `ps` 的命令行。
- 用**不同的** instance 去读已有目录会报 `invalid encrypted credential`，且文件字节保持不变 ——
  不修复、不迁移、不重写（由 `fails_closed_on_wrong_instance_without_mutation` 覆盖）。

## 命令清单

`verification/leaf-commands.txt` 是权威清单，由 `scripts/leaf_commands.py` 从构建产物枚举生成；
`docs/JS-CLI-PARITY.md` 保存官方每条命令的原始 `--help` 文本。按功能分模块：

| 功能族 | 模块 | 叶子命令 |
|---|---|---|
| auth / wallet / market-order / contract-call / sign-message | `auth`、`client`、`trade`、`signing` | 原有 24 条 |
| approvals（授权管理） | `approvals` | list、detail、revoke |
| limit-order（限价单） | `limit_order` | buy、sell、list、cancel |
| prediction（预测市场） | `prediction` | 15 条（category / market / position / order / trade） |
| defi | `defi` | 11 条（protocol / investment / position / deposit / redeem / lp / claim / preview） |
| x402-payment | `x402` | preview、sign |
| wallet 扩展 | `wallet_ops` | speed-up、cancel、resolve-domain |
| tracker（地址跟踪） | `tracker` | 15 条（token / tx / follow / group / address）+ `ws` 推送订阅 |
| signal（自定义信号） | `signal` | 15 条（list / strategy / backtest / explore / credits / wallet-group） |
| leaderboard（榜单） | `leaderboard` | 7 条（query / analyze / alpha-radar / presets） |
| 本地自检 | `selfcheck` | cli-check、skill-check |

原有命令面（未变）：

```text
auth signin
auth verify --qrCodeId QR_ID
auth signout
wallet status
wallet address
wallet chains
wallet settings
wallet gas-price --binanceChainId CHAIN
wallet tx-lock --binanceChainId CHAIN
wallet agent-id
wallet balance [--raw] [--symbol SYMBOL] [--tokenAddress ADDRESS]
               [--binanceChainId CHAIN] [--hideLowValue true|false]
wallet tx-history [--type all|pending|confirmed] [--binanceChainId CHAIN]
                  [--size 20] [--nextCursor CURSOR] [--tx HASH]
                  [--startTime MILLISECONDS] [--endTime MILLISECONDS]
wallet left-quota
wallet send --binanceChainId CHAIN --tokenAddress TOKEN --recipient ADDRESS
            (--amount DECIMAL | --max) [--gasLevel LOW|MEDIUM|HIGH]
market-order quote --binanceChainId CHAIN --fromToken ADDRESS --toToken ADDRESS
                   --fromTokenQty DECIMAL [--slippage auto|PERCENT]
market-order swap --binanceChainId CHAIN --fromToken ADDRESS --toToken ADDRESS
                  --fromTokenQty DECIMAL [--slippage auto|PERCENT]
                  [--gasLevel LOW|MEDIUM|HIGH] [--mev true|false] [--acknowledge-token-risk]
market-order list [--orderId ID] [--binanceChainId CHAIN] [--status PENDING|FINISHED|FAILED]
                  [--fromToken ADDRESS] [--toToken ADDRESS] [--page 1] [--pageSize 20]
                  [--startTime MILLISECONDS] [--endTime MILLISECONDS]
contract-call preview --binanceChainId CHAIN --from ADDRESS
                      [--to ADDRESS] [--value WEI] [--inputData HEX] [--gasLimit UNITS]
                      [--unsignedTx SOLANA_BASE64]
contract-call execute --requestId PREVIEW_ID
contract-call run --binanceChainId CHAIN --from ADDRESS --to ADDRESS
                  --policy /path/to/policy.json
                  [--value WEI] [--inputData HEX] [--gasLimit UNITS]
sign-message preview --binanceChainId CHAIN --signType EIP712 --message JSON_RPC_STRING
sign-message execute --requestId PREVIEW_ID
sign-message result --order-id ORDER_ID
sign-message history [--binanceChainId CHAIN] [--limit 20] [--nextToken CURSOR]
                     [--startTime MILLISECONDS] [--endTime MILLISECONDS] [--sortType ASC|DESC]
batch
```

每条命令的精确语法以它自己的 `--help` 为准。EVM 合约预览必须给 `to`；Solana 必须给 `unsignedTx`
且拒绝仅 EVM 可用的字段。EIP712 消息会保留原始 `eth_signTypedData_v4` JSON-RPC 包装字符串，并且
必须让其签名者与域链同所选钱包一致。

## 常驻加速模式

`serve` 在单个进程内保持一份已解密的会话、一套连接池和一个 60 秒的开发者模式/钱包地址探测缓存，
监听 `<wallet-dir>/.baw-rs.sock`（目录 0700、socket 0600、校验对端 UID）。给任意命令加 `--warm`
即可走服务端；没有服务在跑时会静默回退到普通冷执行。

```sh
./baw-rs --wallet-dir /abs/wallet --instance-id ID --json serve &
./baw-rs --wallet-dir /abs/wallet --instance-id ID --warm --json wallet status
```

在本机、已登录真实钱包、0.3.0 产物上实测 `wallet status`：冷启动 **0.10 s**，走 `serve` 热调用
**0.01–0.03 s**。热模式省掉的是进程启动、会话解密和连接建立；剩余开销仍以服务端为主。`serve` 与
`batch` 不允许走 socket。

## 策略约束的无人值守执行

`contract-call run` 在单进程内完成 preview 与 execute，无需人工复核摘要。它不靠
`--accept-preview-sha256` 授权，而是靠一份 JSON 策略文件；仍然强制要求 `--confirm` 与唯一的
`--operation-id`。策略不匹配会在**发出任何 preview 请求之前**被拒绝。

```json
{
  "version": 1,
  "chainId": "56",
  "to": "0xContractAddress",
  "valueWei": "0",
  "selector": "0x1234abcd",
  "calldataSha256": "完整 0x calldata 的 64 位小写十六进制（可选）",
  "maxGasLimit": 300000,
  "allowRisks": [],
  "allowAuthorityChanges": false
}
```

检查顺序：链 ID、目标地址（大小写不敏感）、精确 value（wei）、4 字节 selector、可选的全 calldata
SHA-256、gas 上限，然后是返回的 preview 中风险码、`authorityChanges` 与 `allowanceChanges`。
任何未被显式允许的项都会终止执行。不做重试：结果不明的 execute 只上报，绝不重发。仅支持 EVM。
累计支出上限（`maxTotalWei`）已存在，但目前只接受零值调用。

## 写操作授权与恢复

**所有 auth 命令、swap、转账与签名 execute 都必须同时给 `--confirm` 和一次性的
`--operation-id ID`。** 这两个全局参数可写在子命令前后。ID 由 1–64 个 ASCII 字母、数字、`-`、`_`
组成。缺少授权会在读取会话或发出 HTTP 之前失败。新 ID 等于一次新授权，绝不是重试不确定操作的许可。

任何写操作提交前，都会在所选目录的 `.baw-rs-operations/` 下同步一条私有的意图记录。记录里是意图
哈希、动作、状态、时间戳和白名单化的成功响应恢复标识，不含凭据、原始签名消息或签名值。已用过的 ID
永久拒绝，包括出错或崩溃之后。每个钱包一把私有锁，避免并发写操作互相踩。**不要删 journal 来重试**：
先对账订单/交易/签名历史。`RESPONSE_RECEIVED` 只表示拿到了接口响应，**不代表成交或交易确认**，必须
另查状态。`FAILED_OR_UNKNOWN` / `SUBMITTING` 需要对账。内层订单失败码一律报错，绝不报成功。swap/
转账的传输层不确定失败会返回 `UNCERTAIN`、`retrySafe:false`，绝不自动重发。

签名 execute 还额外要求 `--accept-preview-sha256 EXACT_DIGEST`（来自该次成功 preview 的输出）。
接受之前请完整阅读输出，包括 `parsedTx`/`parsedMessage`、`risks` 和**全部 `authorityChanges`**。
私有 preview 缓存把这份响应/输入绑定到请求、动作、客户端身份、所选钱包目录的 inode/设备、实际发送方
和链。有效期最长 5 分钟，并进一步受开发者模式过期与服务端过期约束。execute 会复查当前开发者模式与
钱包、在 POST 之前再查一次时间，并且在网络复查**之前**就消费掉已接受的 preview —— 即使这次尝试
失败也不会退回。换一个 operation ID 也无法复用它。只有在一次不确定的提交完成对账之后，才去获取
并复核新的 preview。

`auth signin` 返回二维码 URL、ID 与配对码；它从不打开浏览器。`verify` 在原始二维码有效期内每 2 秒
轮询一次，并受一条持久化的 5 分钟上限约束。只有在 `CONNECTED + CREATED` 之后才提交会话，且采用
加密原子替换与陈旧写入保护。`signout` 即使远端登出失败也会清掉本地会话、保留客户端身份，并明确
报告远端状态不确定。详见 [docs/AUTH.md](docs/AUTH.md) 与 [docs/SIGNING.md](docs/SIGNING.md)。

## 只读批处理

```sh
printf '%s\n' '["wallet","status"]' '["wallet","balance"]' \
  '["wallet","balance","--raw"]' '["wallet","tx-history","--type","pending"]' |
  ./baw-rs --wallet-dir /absolute/wallet-directory --instance-id EXACT-ENCRYPTION-IDENTITY batch
```

batch 只接受只读的 **wallet** 命令，每行一个 JSON argv 数组，复用同一份会话/客户端/连接池。
所有写命令与逐行全局参数覆盖都被拒绝；顶层的 `batch --confirm`、operation ID、preview 接受参数同样
被拒绝，因此不存在"一刀切授权"。错误按行返回，处理继续。单行上限 65,536 字节，超长行可恢复，严格
校验 UTF-8 与字符串数组，末行无换行也能处理。全部成功才返回 0；stdout 管道断开时安静退出。
`--timing` 只在 stderr 打印 `command_ms=...`，绝不打印参数或报文。

## 输出与有意的兼容差异

始终输出 JSON（接受 `--json`）。每条命令产出 `{"success":true,"data":...}` 或
`{"success":false,"error":{"message":"清洗后的错误"}}`。

- 默认余额会规范化 RWA 份额、价格、精确数值、EVM checksum 并做过滤；`--raw` 保留后端原始 tokenList，
  跳过注册表加载/换算/过滤。原始数量**不一定**是 RWA 份额。规范化余额与额度金额是十进制字符串，
  不是 JS 浮点数。后端 JSON 里的十进制数字同样保留任意精度。详见 [docs/PORTFOLIO.md](docs/PORTFOLIO.md)。
- `settings` 有意保持原样的**无损组合** `{settings,tradingLimit,login}`，而不是官方那个改过名字、
  做过强转的显示对象。gas price、tx-lock、Agent ID 返回后端数据。普通查询里的 cookie 轮换只在内存
  生效，且需 HTTP/API 成功后才采纳；只有经过验证的 auth 提交才写盘。
- Quote 保留 quoteId、feeDetail、gasDetails 与后端元数据。签名 preview 保留官方 CLI 丢弃的
  `authorityChanges`。签名结果会有意返回用户请求的签名，且绝不写入日志或 journal。
- RWA 注册表在每条需要规范化的命令里现取；没有官方的全局/陈旧磁盘缓存。失败会向上传播。Solana 地址
  身份区分大小写。严格的输入校验会拒绝畸形金额、地址与含义不明的组合，而不是修剪或猜测。
- Swap 会做 settings/额度/tx-lock 检查并取一份新报价；这**不是** quote-ID 绑定的执行。非原生代币目标
  需要 `--acknowledge-token-risk`，它明确不是代币审计，也不是对后端策略的覆盖。默认滑点为 `auto`。
  不支持的 MEV 选择会直接失败，而不是静默丢掉保护。Ondo 需要 `--mev false` 并采用默认 MEDIUM gas
  选择（不承诺接口层的费率选择）。详见 [docs/TRADE.md](docs/TRADE.md)。
- 交易受理表示 SUBMITTED，不是 FILLED。App 内确认停留在 PENDING_CONFIRMATION。历史/状态与链上确认
  必须分别核对。后端受理仍然是权威；本地额度检查不能证明这笔交易落在美元上限之内。`gasLimit` 是
  单位数，**不是 gas 价格或累计 gas 成本保护**；该协议没有提供费率上限字段。
- 固定发布源为 `https://www.binance.com`；不支持 URL 覆盖、重定向、代理、子进程委托或写操作自动重试。
  请求有 10 秒连接 / 30 秒总超时和 4 MiB 响应上限。只有测试构建能注入回环 HTTP。协议版本元数据固定
  1.12.0；User-Agent 为 `baw-rs/0.3.0`。凭据文件要求目录 0700、普通文件 0600 且属主本人。错误的
  instance 一律 fail-closed，不修复、不迁移。

## 明确不做的部分

任意 URL 端点、浏览器/UI/钥匙串集成、终端二维码渲染、订单自动轮询、交易守护进程、费率上限都**不包含**。
任何不支持的路径都不会静默回退到官方可执行文件。

`tracker ws` 已实现：先走 HTTPS 的 pre-subscribe，再连 `wss://web3-stream.binance.com/w3w/stream`，
发送 `SUBSCRIBE`、每 30 秒 `PING`、`--duration` 到时 `UNSUBSCRIBE`；不设 duration 时按官方逻辑
以 5 秒起步、翻倍到 120 秒封顶重连。socket 本身不带凭据，授权来自 pre-subscribe 返回的 `subKeys`。

## 验证边界

**离线**：`scripts/verify.sh` 一键重跑 fmt / clippy / test / release 构建，并重写
`verification/` 下的日志、`source-sha256.json`（带 git 提交号与 `dirty_source` 标记）、
`release-identity.txt` 与 `leaf-commands.txt`。测试全部使用回环 HTTP fixture 与合成会话文件。

```sh
export PATH="$HOME/.cargo/bin:$PATH"
CARGO_TARGET_DIR=/path/to/target sh scripts/verify.sh
```

**实盘**（2026-10-30）：用真实二维码完成了一次生产登录（`auth signin` → App 确认 → `auth verify`
自动提交），随后只读验证通过：`wallet status`（CONNECTED）、`wallet address`（真实多链地址）、
`wallet balance`（真实余额）、`wallet left-quota`、`wallet settings`（含 `sessionExpireTime` 等
服务端时效字段）。会话有效期实测 48 小时。

**未验证**：转账、swap、限价单、DeFi 存取、x402 签名、contract-call 执行等任何真实资金动作。
这些路径只有离线测试覆盖，第一次在真实账户上执行前请自行评估风险。
