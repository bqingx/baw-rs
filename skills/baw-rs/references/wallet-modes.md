# 钱包选择与常驻模式

## 三个入口不能混用

- 普通 CLI：显式 `--wallet <NAME>`（命名钱包，走 resident daemon）或 wallet-dir + instance-id-file
  （传统单钱包），一条命令一个进程。
- 传统单钱包 `serve`：该钱包私有 socket，业务 CLI 加 `--warm`；连接不到时旧路径可能回退冷执行。
- `wallets`：registry 管理与常驻控制面（add/remove/list/status/serve/stop），本身不需要钱包。
  具名钱包的业务命令用全局 `--wallet <NAME>`；daemon 不可用时报告，不悄悄换钱包。

## 原生日常多钱包

先征得创建目录/注册/启动后台的授权；使用已存在的钱包及固定身份文件，不搬运/显示凭据。

```sh
# registry：--registry ＞ $BAW_RS_REGISTRY ＞ $HOME/wallets/registry
"$B" --registry "$REGISTRY" wallets add "$WALLET_NAME"     # 新建 0700 目录 + 0600 identity 并注册
"$B" --registry "$REGISTRY" wallets add alice --wallet-root /abs/wallets
"$B" --registry "$REGISTRY" wallets register "$WALLET_NAME" \
  --wallet-dir "$WALLET_DIR" --instance-id-file "$IDENTITY_FILE"   # 注册已有目录
"$B" --registry "$REGISTRY" wallets list
"$B" --registry "$REGISTRY" wallets serve
"$B" --registry "$REGISTRY" wallets status
"$B" --registry "$REGISTRY" --wallet "$WALLET_NAME" wallet address --json
"$B" --registry "$REGISTRY" --wallet "$WALLET_NAME" task status
"$B" --registry "$REGISTRY" wallets remove bob              # 默认保留数据
"$B" --registry "$REGISTRY" wallets remove bob --delete-data --confirm
```

serve 为长驻命令，运行方式由已授权的服务管理决定；不重复启动、不擅自安装 systemd。
add/remove/register 时已有服务可能持锁，先说明需维护窗口，不自动 stop。新增钱包不应影响旧钱包登录态。
`--wallet` 模式不允许 wallet-dir/instance-id/warm 等选择器覆盖；名字无效、未注册、未登录就报错，禁止 fallback。
管理子命令不接受 --json（本身就输出 JSON）；业务命令支持。不同钱包可并行，同钱包当前 resident 将整个
命令排队，连预览也包含在内。这只是本地实现行为，不证明链上或币安协议绝对不能并行。

旧形式仍可用：`vaults --registry R run NAME -- <业务参数>`（`vaults` 是 `wallets` 的隐藏别名），
与 `--wallet NAME <业务参数>` 等价；不要两套混写。

## 凭据与文件

目录要求 0700；身份/会话是用户所有、0600、普通单硬链接文件，不接受 symlink。
identity 是解密凭据，不绑定设备；只备份 session 而丢失 identity 无法恢复。
`wallets add` 生成随机 identity，从不覆盖已存在的 identity 文件。
普通 baw 与 baw-rs 独立管理会话。禁止自动 import/覆盖/迁移；不按“同一地址”推断两边登录同时有效。

## 常驻与日常隔离

用户选择狙击单钱包，日常多钱包保留。单钱包 `serve`/`--warm` 与已有 resident 争用同一钱包独占锁时应明确报错。
只有用户确认维护/切换后才能交接服务，不能通过拷贝会话或设置并发环境变量暗绕所有权。
