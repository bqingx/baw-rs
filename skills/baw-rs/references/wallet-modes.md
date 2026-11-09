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
"$B" --registry "$REGISTRY" wallets add "$WALLET_NAME" \
  --wallet-dir "$WALLET_DIR" --instance-id-file "$IDENTITY_FILE"   # 注册已有目录
"$B" --registry "$REGISTRY" wallets list
"$B" --registry "$REGISTRY" wallets serve
"$B" --registry "$REGISTRY" wallets status
"$B" --registry "$REGISTRY" --wallet "$WALLET_NAME" wallet address --json
"$B" --registry "$REGISTRY" pipeline status "$PIPELINE_ID"
"$B" --registry "$REGISTRY" wallets remove bob              # 默认保留数据
"$B" --registry "$REGISTRY" wallets remove bob --delete-data --confirm
```

serve 为长驻命令，运行方式由已授权的服务管理决定；不重复启动、不擅自安装 systemd。
配套新版 CLI/daemon 的 add 可在线新增，首次业务引用热加载；移除/删除仍需先经授权停止后台。新锁协议分离了短事务与生命周期锁，不允许重绑定/重复目录别名/跨 registry 抢占运行中钱包。旧 daemon 仍持旧长锁时须显式升级，不能靠新版 CLI 单边解锁或自动 stop。新增不改写原身份/会话。
`--wallet` 模式不允许 wallet-dir/instance-id/warm 等选择器覆盖；名字无效、未注册、未登录就报错，禁止 fallback。
管理子命令本身输出 JSON，--json 可选；业务命令也支持。不同钱包可并行，同钱包当前 resident 将整个
命令排队，连预览也包含在内。这只是本地实现行为，不证明链上或币安协议绝对不能并行。

隐藏 `wallets run/register` 已删除，不再接受旧包装；`vaults` 只是 `wallets` 的当前别名，不提供第二套入口。业务统一使用 `--wallet NAME <业务参数>`。

## 手动总览

wallets overview 要求配套新版 daemon，默认只读本地钱包/Task/journal白名单快照（会话存在不等于CONNECTED），不后台轮询。--refresh 才查login/settings；最多16钱包、并发4、单钱包6秒，不改财务Client/cache或持久Cookie。活动/中断未解决Task、忙钱包或未加载钱包跳过。刷新保留钱包互斥，Arm/Resume在准入前明确报忙而不消费授权/期限；普通随后命令可能等最多该钱包6秒，不是事件热路径固定延迟。临时GET若换Cookie，原持久会话标UNKNOWN，不自动续期。快照per_item_not_atomic，部分读取/截断不等于无风险。

## 凭据与文件

目录要求 0700；身份/会话是用户所有、0600、普通单硬链接文件，不接受 symlink。
identity 是解密凭据，不绑定设备；只备份 session 而丢失 identity 无法恢复。
`wallets add` 生成随机 identity，从不覆盖已存在的 identity 文件。
普通 baw 与 baw-rs 独立管理会话。禁止自动 import/覆盖/迁移；不按“同一地址”推断两边登录同时有效。

## 常驻与任务

新任务全部使用 named daemon 的实例级流水线；旧单钱包任务和 typed executor 已删除。`task` 只是 `pipeline` 的别名。
普通目录形式 `serve`/`--warm` 仍用于兼容业务命令，与已有 resident 争用同一钱包独占锁时明确报错。
只有用户确认维护/切换后才能交接服务，不能通过拷贝会话或设置并发环境变量暗绕所有权。
