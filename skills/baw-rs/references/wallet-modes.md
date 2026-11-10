# 钱包选择与常驻进程

## 入口

| 入口 | 用途 | 带 `--wallet`？ |
|---|---|---|
| `"$B" --wallet NAME <command>` | 日常业务；经命名 daemon 执行 | 必须 |
| `"$B" wallets <add/remove/list/status/overview/serve/stop>` | 注册表与 daemon 管理（`vaults` 是隐藏别名） | 不能带 |
| `"$B" pipeline/task ...` | 实例级任务 | 不能带 |
| `"$B" openfour ...` | 链上只读/离线编码，不需要钱包 | 不能带 |
| `"$B" wallets journal NAME (--operation-id ID \| --list)` | 本地操作日志；不连 daemon、不读 session、未登录也能用 | 不能带 |

旧的单钱包入口（顶层 `--wallet-dir` / `--instance-id` / `--instance-id-file`、warm 模式开关、顶层 `serve`）已移除：这些参数现在直接报"未知参数"，没有冷执行回退。名字不存在、未登录、daemon 未运行都直接报错，不会回退到别的钱包。

## 命名 daemon

```sh
"$B" wallets status --json      # 先查；本地、零网络
nohup "$B" wallets serve >/dev/null 2>&1 &   # 仅在未运行时启动
"$B" wallets stop               # 停止接新请求、排空在途请求后退出
```

- 注册表：`--registry` > `$BAW_RS_REGISTRY` > `$HOME/wallets/registry`。socket 是 `<registry>/daemon.sock`（0600，校验同 UID）。
- 每钱包独立客户端、连接池、cookie、会话；不同钱包并行，同一钱包的命令按 FIFO 串行，排队超过 30 秒的请求被拒绝（未执行）。
- 运行中 `wallets add` 新增的钱包在第一次被引用时热加载，无需重启；在线删除/重绑定会被拒绝，需先 stop。
- 不重复启动 serve，不擅自安装 systemd 或替换正在运行的二进制。

## 钱包生命周期

```sh
"$B" wallets add NAME                                   # 新建 0700 目录 + 0600 随机 identity，然后要 auth 登录
"$B" wallets add NAME --wallet-dir D --instance-id-file K   # 接入已有目录，只登记不覆写
"$B" wallets list                                       # 名称与路径；signedIn 只看 session 文件是否存在
"$B" wallets remove NAME                                # 只注销，保留数据
"$B" wallets remove NAME --delete-data --confirm        # 删除 session/identity/目录（需 daemon 已停）
```

- identity 是解密 session 的密钥，不绑定设备；只备份 session 而丢了 identity 无法恢复。
- 官方 `baw` 与 baw-rs 的会话各自管理；格式同源，可用官方登录后的目录 + 显式 instance id 文件接入，但不要自动导入/覆盖。
- `wallets overview` 默认只读本地快照；`--refresh` 才查登录/设置（每钱包最多 6 秒，与 arm/resume 互斥，报忙时不要自动重试）。
- `wallets journal NAME --list` / `--operation-id ID` 只读该钱包目录下的 `.baw-rs-operations/`，daemon 未运行也能对账。

## 文件要求

目录 0700、identity/session/配置 0600，属主为当前用户，普通单硬链接文件，不接受符号链接。权限不符时报错，不要 chmod/删除"修复"他人文件，先问用户。
