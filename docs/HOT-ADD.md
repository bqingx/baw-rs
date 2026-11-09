# 钱包在线新增：锁修复

之前虽实现了 resident 的按名称热加载，`wallets serve` 却在整个生命周期持有 `registry.lock` 共享锁，`wallets add` 的独占注册锁始终失败。接入现有 w2/w3/w5/w7/w8 时确实需要停后台，不能把旧测试的直接修改注册表等同于 CLI 在线新增。

本轮拆分两种锁：

- `registry.lock`：只保护短期 registry.json 读/写事务。新增、加载、热加载都使用它，启动读取列表后释放。
- `registry-lifecycle.lock`：daemon 生命周期持有共享锁；remove/delete 必须取得独占锁，所以运行中即使某个新增钱包还没被加载，也不允许删除/注销。

固定锁顺序是生命周期屏障 → 短事务锁。新增只需事务锁，不等 daemon 退出；真实单 daemon 仍由原 daemon/socket 独占租约保证。注册已有目录还会临时取得该目录的 `.baw-rs-resident.lock`，直到 registry 更新持久化，不允许从其他 registry 或单钱包后台抢占正被使用的目录。

已有钱包接入方式不变：

```sh
baw-rs --registry /absolute/registry wallets add alice \
  --wallet-dir /absolute/existing-wallet \
  --instance-id-file /absolute/existing-identity
baw-rs --registry /absolute/registry --wallet alice wallet status
```

显式同时提供已有目录/身份时采用原数据；不重写 session/identity，不改权限，不重新登录。第一次业务引用时 resident 热加载；原钱包继续运行，PID 不变。不同钱包的会话有效性仍需分别核验。

## 不放宽的边界

- 同名重绑定、同 inode 目录的重复别名仍拒绝。
- 活跃后台期间，remove、--keep-data、--delete-data --confirm 都受生命周期屏障拒绝。不要通过手工修改 registry 或删锁绕过。
- 已在其他后台独占的目录不能被新注册抢走。
- 冲突或失败不覆写身份、session、已有 registry 数据。
- 文件/目录私密权限、identity、设备号/inode、socket single owner 校验保留。

**新锁协议需要更新后的 CLI 和 daemon 配套使用。** 没有为了兼容旧进程再保留一套协议；旧 daemon 仍持有旧长锁时，新 CLI 无法令它凭空支持 hot add。本轮新 artifact 尚未部署；现有 6 个钱包的后台不受代码/离线测试影响。以后的显式切换需先确认无未结束任务并优雅 drain，再运行核验过的新产物。

## 验证

`tests/multiwallet.rs::online_add_hot_loads_without_allowing_removal_rebinding_or_cross_registry_use` 用独立真实 CLI/IPC 进程和临时合成会话验证：后台运行时 add→新钱包 status→原钱包 status→PID 不变；在线删除（包括尚未热加载的钱包）拒绝；重绑定/重复别名/跨 registry 抢占/第二个 daemon 拒绝；身份/session bytes 和失败前的 registry bytes 不变。

multiwallet 15/15 通过；不是生产钱包热添加实测，未操作现有后台或真实会话。
