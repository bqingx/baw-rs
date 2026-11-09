# 手动状态总览与提交前意图摘要

本轮只改代码/离线测试/构建，不切现有六钱包后台，不读取真实钱包/OKX 配置或推送 token，不执行真实交易/推送。新 IPC/CLI 需配套更新后才能使用。

## 1. 状态总览

```sh
baw-rs --registry /absolute/registry wallets overview
baw-rs --registry /absolute/registry wallets overview --refresh
```

### 默认本地快照

通过 named daemon 汇总已注册/已加载钱包、私密会话文件是否存在、Task 状态/阶段/计数/原持久截止时间、需人工核验的本地 operation，以及通知 enabled/丢弃计数。

- 默认 **0 个后台 HTTP、0 个链查询**，不解密钱包会话，不初始化财务 Client，不补 allowance、quota、余额或回执查询。
- sessionFilePresent 仅表示本地存在安全文件，不能当作 CONNECTED。remote 标为 NOT_REFRESHED；没有新检查时不伪造登录有效期。
- Task/operation 输出白名单字段，不输出配置路径、RPC URL、calldata、任意原始错误/响应、身份、Cookie、session 或凭据指纹。
- 已被绑定 Task 确认的 operation 可从待核验列表排除；其他带 hash 的历史操作仍只表示需验证，不自动冒充已成交。无 hash/未知/损坏记录显式显示，retrySafe 始终 false。
- Task 文件或 journal 损坏、目录不安全、数量截断会标为不完整，不把“没读出来”当作“没有未解决操作”。每次最多扫描 256 个额外归档、128 个 journal 文件；内存 Task 列表也有界。

### 显式刷新

只有 --refresh 才使用独立临时 Client，每个合格钱包至多两个固定 GET：login/query → settings/query；不是现有 wallet settings 的三查询组合，不查 quota/address/allowance，不打开 RPC。

- 一次最多 16 个钱包、并发 4、每钱包总预算 6 秒；失败/超时不重试。不因一次失败阻止其他钱包本地信息展示。
- 若已有活动 Task、未解决/中断状态或 Task 正在改状态，则跳过远程刷新。未加载的钱包也跳过，不热加载、不创建/持有新钱包租约；需要时先经正常命名钱包命令完成加载。
- 明确的读写准入隔离：整个远程刷新占有 inspection 写 reservation，Arm/Resume 必须在任何预加载/授权/截止授予前取得共享执行 permit，并持有至任务 drain/终态保存。因此两者不能交错开始；刷新期间 Arm/Resume 明确报忙，原 Task/截止不改变，不自动重试。默认本地 overview 不占用该 reservation。
- 每个钱包只用 try_lock，不排队等忙钱包；成功后保留钱包互斥到该钱包刷新结束，防止普通钱包执行和读取交错。主动刷新的最长 6 秒可能使随后到达的普通钱包命令短暂等候；不是后台热路径固定延迟。已 armed 的事件路径不会被新刷新抢占。
- 临时 Client 不放入 resident 财务 Client/cache，也不保存 Cookie/session。若提供方在 GET 中换发 Cookie，结果明确 session_rotation_not_persisted，当前持久会话连接状态改为 UNKNOWN，仅保留 observedConnectionStatus；不能把一次性查询的 CONNECTED 当作原会话仍有效。不会自动续期/登录/修复，需单独按正常钱包流程核验。
- 开发者模式 expiresAt 按现有接口的 Unix 秒显示；login 到期字段保留 AsReported 原值，不猜秒/毫秒，不计算一个虚假的统一到期日。缺失字段为 null。

所有数据是 `per_item_not_atomic` 的手动快照，并有本次采集时间；不是同一链高度的全局一致视图，不增加后台轮询。普通 status/overview 都不是签名授权。

## 2. 独立兑换意图摘要

```sh
baw-rs market-order intent \
  --binanceChainId 56 \
  --fromToken 0x1111111111111111111111111111111111111111 \
  --toToken 0x2222222222222222222222222222222222222222 \
  --fromTokenQty 1.25 --fromTokenDecimals 18 \
  --slippage 1 --mev false
```

这两个 token 只是格式示例，不是交易建议。命令不需要钱包或 daemon，在钱包/会话/registry/私有 OKX profile 加载之前纯本地返回。即使提供 --okx-config，摘要也不打开它、不显示其路径。

展示 chain、输入/输出 token、输入十进制数量和显式 decimals 下的精确最小单位、税率上下限、滑点、可选的用户固定最低净到账 floor、精确/额度不足才授权策略、风险接受标记、MEV/Gas 实际接口限制。

示例省略可选 `--okx-min-output-units`，即 `None`，不补 0/1 wei，也不将其视为必填缺失。显式提供则为正整数 `Some`，仍是额外严格固定底线。执行/触发时使用 fresh OKX route 的 `minReceiveAmount`，必须存在且 >0、>= 当次 quote 按用户 slippage 计算的 floor、<= quoted output，且独立模拟钱包净到账 >= reported；显式 `Some` 还须满足固定底线。不采用准备期旧 quote floor，不自动增加滑点或税率，不本地解码 calldata，不称链上独立保底。纯本地摘要没有这些报价/模拟证据。

`None` / `Some` 连同完整 args 绑定执行意图/准备；摘要不改变已有显式任务、授权或截止，不自动升级或重放旧记录。

没有显式 decimals 就不猜数位、units=null；没有风险接受等必填字段可展示 missing，不借此降低真实 swap 校验。MEV=true/非 MEDIUM gas 会说明当前接口不能满足，不能把“已显示”理解成已支持。native decimals 需 18 的限制也明确指出。

**executionAuthorized=false、noDeadlineGranted=true。** 本地字段齐全不表示钱包、余额、税率、授权、profile、报价或路由已验证。没有 quote/Preview、preview digest、operation journal、授权或签名，不保存可重放模板，也没有可以充当 --accept-preview-sha256 的摘要 hash。

## 3. 固定 Task 定义摘要

```sh
baw-rs pipeline describe --config /absolute/private/task.json
```

定义文件须绝对路径、0600 且父目录 0700。只读取该定义，验证当前完整固定 Task 语法/拓扑/链、触发与完成条件；不存在的私有 OKX profile/policy 不被打开。无需 registry/daemon 已存在，不执行 pipeline load/arm。

- 各 step 展示有效时长、监听前准备预算、触发条件、确认条件和每钱包意图；不泄露 RPC URL。
- Swap 复用纯 intent_summary；contract-call fire 复用实际执行的纯 payload 校验（包括 calldata 格式、gas 边界），但只输出目标、声明 from、value、gas 与是否配置 policy，不输出 calldata/policy 路径。
- 这里只验证声明的发送方格式，不查真实钱包地址、不审计合约或验证外部 policy；执行时仍需原绑定/模拟/权限 gate。
- 明确 loaded=false、executionAuthorized=false、deadlineGranted=false。validForMs 是原定义时长，不是从 describe 开始计时；绝对截止只由原 arm/步骤准备流程授予，不能通过描述刷新。
- 描述不能替代 arm --confirm。后续 arm 仍可能花费精确授权 Gas，须用户另行明确授权。

## 离线验收

- 默认 overview 零 HTTP、独立刷新恰好两 GET，财务 Client 仍未初始化、身份/session bytes 不变。
- 活动/忙钱包跳过；在 login 请求被故意挂起时验证钱包互斥保留，Arm 被明确拒绝且原 Task 文件完全不变；刷新结束后原 arm 正常可用。
- 未加载钱包不新增 lease/常驻状态，缺失/损坏/hashless/未确认 journal 不泄露正文、不标 retrySafe。
- 提供方换 Cookie（含第二个请求失败）不误报当前原会话 CONNECTED；后台错误被静态分类，不泄露正文。
- 真实 CLI describe 不创建 registry/Task/授权文件、不打开不存在的 profile/policy；大整数精度保留，非法 calldata/gas 拒绝。
- 配置 RPC 的描述用例证明没有建立连接；摘要缺字段不削弱原 swap gate。

全量当前树 **393 Rust 通过 / 0 失败 / 1 手工基准忽略**，Clippy -D warnings、7 项技能检查和 27 项隔离 smoke 通过，51 个规范可见 CLI 叶子。Release 9,399,504 bytes，SHA256 `a151a3679d0b6daf6bf5c75af5241ef665d0f2e8994989c367f3cb48f21809fc`，未部署。

日志 `/tmp/baw-overview-intent-tests.log`、`/tmp/baw-overview-intent-clippy.log`、`/tmp/baw-overview-intent-release.log`。这些仅证明模拟/本地边界，不冒充真实平台可用性或实盘成交。
