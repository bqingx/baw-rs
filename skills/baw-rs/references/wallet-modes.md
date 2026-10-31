# 钱包选择、批处理与常驻模式

## 四种入口不能混用

- 普通 CLI：显式 wallet-dir + instance-id-file，一条命令一个进程。
- 传统单钱包 `serve`：该钱包私有 socket，业务 CLI 加 `--warm`；连接不到时旧路径可能回退冷执行。
- `vaults`：registry 管具名钱包；日常多钱包选择 NAME，run 调用已有 daemon；不可用时报告，不悄悄改钱包。
- `fast serve`：单钱包 stdin/stdout JSONL，极速用途。一次启动持续写订单；不是旧 socket 的别名。

## 原生日常多钱包

先征得创建目录/注册/启动后台的授权；使用已存在的钱包及固定身份文件，不搬运/显示凭据。

```sh
"$B" vaults --registry "$REGISTRY" register "$WALLET_NAME" \
  --wallet-dir "$WALLET_DIR" --instance-id-file "$IDENTITY_FILE"
"$B" vaults --registry "$REGISTRY" list
"$B" vaults --registry "$REGISTRY" serve
"$B" vaults --registry "$REGISTRY" status
"$B" vaults --registry "$REGISTRY" run "$WALLET_NAME" -- wallet address --json
```

serve 为长驻命令，运行方式由已授权的服务管理决定；不重复启动、不擅自安装 systemd。
register 时已有服务可能持锁，先说明需维护窗口，不自动 stop。新增钱包不应影响旧钱包登录态。
run 的 `--` 后不允许 wallet-dir/instance-id/registry 等选择器覆盖；名字无效/未登录就报错，禁止 fallback。
外层不接受 --json；内层业务支持。不同钱包可并行，同钱包当前 resident 将整个命令排队，连预览也包含在内。
这只是本地实现行为，不证明链上或币安协议绝对不能并行。

## batch

普通 CLI 的 batch 从 stdin 每行读一个 JSON argv 数组，复用会话。只读批处理，不能借顶层 confirm、
operation-id 或摘要让 batch 写交易。不要和 fast 的“每行一个订单对象”混用。

## 凭据与文件

目录要求 0700；身份/会话是用户所有、0600、普通单硬链接文件，不接受 symlink。
identity 是解密凭据，不绑定设备；只备份 session 而丢失 identity 无法恢复。
普通 baw 与 baw-rs 独立管理会话。禁止自动 import/覆盖/迁移；不按“同一地址”推断两边登录同时有效。

## 极速与日常隔离

用户选择狙击单钱包，日常多钱包保留。fast 与已有 resident 争用同一钱包独占锁时应明确报错。
只有用户确认维护/切换后才能交接服务，不能通过拷贝会话或设置并发环境变量暗绕所有权。
