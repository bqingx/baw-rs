# 原生多钱包：`vaults`

`vaults` 是新的本地控制面，不与原有 `wallet` 业务命令冲突。一个 Rust 进程管理多个命名钱包，直接调用原生客户端；**没有每钱包一个子进程，也不调用官方 CLI**。旧的 `--wallet-dir` + `--instance-id[-file]` 单钱包语法仍可使用。

## 注册与运行

以下 `/abs/...` 均为占位符。注册只保存已有独立钱包目录和已有 identity 文件的路径，不复制/迁移会话，不登录，不创建或重写 identity 文件。注册可以指向一个已有的空钱包目录，以便之后明确执行 signin。

```sh
# alice、bob 目录须已存在、属主本人且权限 0700。
# 对应的 identity 文件须已存在，权限 0600、单硬链接、非符号链接。
baw-rs vaults --registry /abs/private-registry register alice \
  --wallet-dir /abs/wallet-alice --instance-id-file /abs/alice.instance
baw-rs vaults --registry /abs/private-registry register bob \
  --wallet-dir /abs/wallet-bob --instance-id-file /abs/bob.instance
baw-rs vaults --registry /abs/private-registry list

# 前台运行；不会安装系统服务。另开终端发命令。
baw-rs vaults --registry /abs/private-registry serve
```

注册表目录不存在时，`register` 在已有父目录中以 0700 创建它；`registry.json`、锁文件均为 0600。注册内容为名称、规范化绝对路径、钱包目录的设备号/inode，**没有口令、cookie 或解密密钥**。最多 128 个钱包，名称区分大小写，允许 1–64 个 ASCII 字母、数字、`-`、`_`。

同一目录通过相对路径、`..`、符号链接别名重复注册，会按规范化后的设备号/inode 拒绝。启动和每次命令都复核目录身份，目录被替换时拒绝运行。不同注册表/旧单钱包服务也不能同时占有同一钱包的 resident 租约锁。

注册表是服务启动时的快照：服务运行期间拒绝 `register`，需要先 `stop`、等服务退出、注册，再 `serve`。没有自动切换、默认钱包、隐式选第一个钱包或热修改注册表。

```sh
# 本地 daemon 状态：PID、已注册名称；不会访问生产网络或逐个查询登录状态。
baw-rs vaults --registry /abs/private-registry status

# 每次都必须指定准确名称；-- 后面是原有业务命令。
baw-rs vaults --registry /abs/private-registry run alice -- wallet status
baw-rs vaults --registry /abs/private-registry run bob -- wallet address

# 分别执行，可并行；写操作仍必须明确授权。
baw-rs vaults --registry /abs/private-registry run alice -- \
  --confirm --operation-id alice-signin-001 auth signin
baw-rs vaults --registry /abs/private-registry run alice -- \
  --confirm --operation-id alice-verify-001 auth verify --qrCodeId RETURNED_QR_ID
baw-rs vaults --registry /abs/private-registry run alice -- \
  --confirm --operation-id alice-signout-001 auth signout

# 停止接收新连接，等待已接收任务完成，再清理 socket（文件系统阻塞可延长等待）。
baw-rs vaults --registry /abs/private-registry stop
```

`stop` 返回 `stopping` 表示已接受停止请求，不意味着所有在途操作已结束；前台 `serve` 最后输出 `stopped` 后才完成退出。强杀、断电、IPC 断线不证明资金操作失败，必须查各钱包 operation journal 和后端状态，不能自动重试。正常 `stop` 不主动取消已提交的写操作。

未知名称、未运行 daemon、选中钱包解密失败，都直接报错。**不会冷执行，也不会回退到另一个钱包**。`run` 内的 `--wallet-dir`、`--instance-id`、`--instance-id-file`、`--warm`、`--registry` 等选择器（包括 `--x=value`）被拒绝。命名路径不读取 `BINANCE_BAW_DIR` / `BINANCE_INSTANCE_ID` 来选择钱包。

## 并发、会话与缓存

- 每钱包独立客户端、连接池、cookie、持久会话快照、支付消费标记、async FIFO 锁。不同钱包的请求可同时等待网络；同一钱包的所有命令（不只写命令）按取得该锁的顺序执行。排队超过 30 秒的请求明确拒绝，不执行。
- `verify` 的网络请求和两秒轮询等待均是异步的，不占其他钱包的锁。预加载磁盘会话与 PBKDF2 在阻塞任务池处理；原有 auth 内部也有同步加解密，不承诺硬实时调度或固定延迟。
- 每条命令都重新检查注册路径和已加密会话；磁盘凭据改变会替换该钱包客户端。auth 从磁盘状态重新引导，结束时无论成功/失败均丢弃客户端中的临时 cookie，下一条命令从持久状态恢复。signin 的 pending cookie 只来自该钱包加密的 `auth-pending`；verify 不会把其他钱包的 QR/cookie 带入。
- 外部明确执行的冷命令修改/删除会话，会在下一次 daemon 命令预加载时被发现。已经在途的请求不能倒退撤回。冷进程不加入 daemon 的 FIFO 队列，仍由原有钱包写锁防止同时写入（冲突会失败）。
- 移除了原来按链 ID 索引的进程全局签名探测缓存；开发者模式和地址每次以当前客户端重新核实。文件 preview 缓存和 operation journal 仍在各钱包目录，沿用原有身份绑定。x402 的内存消费标记改为每钱包隔离，跨 auth 刷新保留；跨重启的付款选项防重放仍不能替代后端对账，持久 operation ID 日志仍生效。
- 不承诺公共行情命令可在未登录时执行；原有 dispatch 对多数命令仍要求所选钱包的会话。本次没有扩展这个边界。

## 本地 IPC 与流式命令

Linux Unix socket 为 `<registry>/daemon.sock`，父目录 0700、socket 0600，客户端与服务端都校验 peer UID。一个连接仅处理一条 JSON 行。请求上限 65,536 字节、最多 128 个 argv 项；响应上限 8 MiB；连接/读请求/写响应三秒超时；dispatch 阶段设置 360 秒异步超时，客户端读响应最长 400 秒；同时最多 64 个连接任务。过载可能拒绝连接，不提供恶意同 UID 调用者下的公平性保证。同 UID 进程本身可读凭据，不属于不同用户的安全隔离边界。

**上述时间不是服务端端到端完成或取消保证。** 360 秒计时不包含最多 30 秒的钱包锁排队，也不包含此前 `spawn_blocking` 的目录/identity/会话预加载与 PBKDF2；当前预加载没有超时。dispatch 中的同步文件系统/加解密操作也不能由异步计时器强制中断。磁盘或文件系统阻塞时，预加载、命令响应以及正常 `stop` 等待都可能超出上述时间；客户端 400 秒超时或断开不取消服务端工作，预加载恢复后仍可能提交命令，不能据此重试。已启动的 `spawn_blocking` 不能强制取消：本实现等待其完成并在此期间保持该钱包 FIFO 锁，不以丢弃 JoinHandle、提前释放锁来伪装有界取消。若以后加入预加载期限，必须在超期后禁止后续 dispatch，并确保阻塞工作真正结束之前不释放相关锁/租约、不允许存储竞争。

原始 IPC 示例（仅解释协议，一般使用 CLI）：

```json
{"op":"run","name":"alice","argv":["wallet","status"]}
```

未知字段、非法 JSON、非字符串 argv、选择器覆盖均 fail-closed，不在 daemon stdout 打印请求或凭据。`batch`、`serve`、`tracker ws` **禁止经 IPC 执行**，在读钱包文件/网络访问之前拒绝，避免无限流阻塞队列或向共享 stdout 输出事件。需要流式订阅时，另用原有显式单钱包 CLI；该输出属于独立调用者，不与常驻服务混合。命名 IPC 暂不提供流式复用。

旧单钱包 `serve` / `--warm` 已复用同一原生引擎及安全边界，socket 仍为 `<wallet-dir>/.baw-rs.sock`。旧 CLI 只有在 socket 路径根本不存在时保留冷执行兼容行为；已经尝试 IPC 后不再冷重试。旧 socket JSON 协议不是兼容 API，升级服务和 CLI 时应一起重启。

## 加密兼容与验证范围

`src/session.rs` 的加密算法和 KDF **没有修改**：PBKDF2-HMAC-SHA256，password=固定 instance 字符串，salt=`baw-agentic-wallet-v1:`+同一字符串，100,000 轮，AES-256-GCM。没有 MAC、主机名、硬件、系统钥匙串或注册名称参与密钥推导。目录 inode/device 只用于目录身份检查，**不是加密密钥的一部分**。换机器后可重新注册对应目录和原始 identity 文件，不需改变口令或重加密。

本次验证只使用 scratch 下合成凭据、回环 HTTP 和 Unix socket：覆盖双钱包不同 identity 并发、相同钱包写入顺序、命名 signin/verify、pending cookie 隔离、登出错误后刷新、外部会话变更、目录重复/替换、错误选择器、畸形 IPC、旧单钱包兼容等。测试版回环 origin 注入由 `cfg(test)` 限制，正式可执行文件没有可配置生产 origin。

**没有读取/复制/修改真实钱包或 key，没有生产网络请求，没有生产多钱包、真实扫码、资金动作或压力测试证明。** 原有历史实盘记录和命令 help 数量不能作为本功能的生产验证或完整功能等价证明。当前门禁及 release 身份见 `verification/multiwallet/`；重跑命令见其中说明。
