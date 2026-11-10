# 实例级 Task / 流水线

`pipeline` 与 `task` 是同一套命令，属于整个实例（registry），**不带 `--wallet`**；需要命名 daemon 在运行。
完整语义见仓库 `docs/PIPELINES.md`，本页是操作手册。

## 配置文件

放在仓库外，父目录 0700、文件 0600、普通文件（非符号链接）。所有地址/RPC 必须是真实值，示例里的占位符不能上链。

```json
{
  "id": "demo",
  "chain_id": "56",
  "rpc_url": "wss://<chain>.g.alchemy.com/v2/<KEY>",
  "steps": [
    {
      "task_id": "buy",
      "trigger": {"kind": "event", "address": "0x<合约>", "topics": ["0x<事件签名哈希>"], "from_block": 12345000},
      "valid_for_ms": 600000,
      "completion": "confirmed",
      "confirmations": 1,
      "actions": [
        {"wallet": "w1", "argv": ["market-order", "swap", "--okx-config", "<绝对路径>",
          "--binanceChainId", "56", "--fromToken", "0x<USDT>", "--toToken", "0x<目标币>",
          "--fromTokenQty", "10", "--fromTokenDecimals", "18", "--slippage", "10",
          "--mev", "false", "--acknowledge-token-risk", "--auto-approve"]},
        {"wallet": "w2", "argv": ["contract-call", "fire", "--binanceChainId", "56",
          "--from", "0x<w2 地址>", "--to", "0x<合约>", "--value", "0", "--inputData", "0x<calldata>"]}
      ]
    }
  ]
}
```

| 字段 | 规则 |
|---|---|
| `id` / `task_id` | 字母数字等安全字符；同 id 不能覆盖已有流水线 |
| `rpc_url` | WSS；整个实例同时只允许一个 endpoint（与 OKX 配置里的 rpc_url 同址） |
| `steps` | 1–256 个 task，顺序执行；钱包集合可重叠 |
| `actions` | 每个 task 1–16 个**不同**钱包，并行执行；只支持 `contract-call fire` 和 `market-order swap` |
| `argv` | 钱包相对参数；**不能**带 `--wallet`、`--confirm`、`--operation-id`、`--skip-prechecks` 等（operation-id 由系统按 流水线/步骤/钱包 确定性生成） |
| `trigger` | `{"kind":"immediate"}` · `{"kind":"time","unix_ms":N}` · `{"kind":"block","number":N}` · `{"kind":"event","address":A,"topics":[...1-4个],"from_block":N}` |
| `valid_for_ms` | 正整数；从本步额度准备完成、开始监听时起算，覆盖等待+执行+回执；恢复不续期 |
| `completion` | `confirmed`（默认，全部成员链上成功才进下一步）或 `submitted`（拿到有效 txHash 即推进，不保证成功） |
| `confirmations` | 默认 1 |

`event` 是**已出块**日志（WSS `logs` 订阅 + 最多 500 块回补，每次 10 块），不是 pending 交易监听。
`from_block` 是显式回补起点：若只想要就绪之后的新事件，等 WAITING 后再用当前块号。

## 操作流程

```sh
"$B" pipeline describe --config "$CFG"   # 纯本地摘要，给用户审核；不 load、不联网、不授权
"$B" pipeline load --config "$CFG"       # 装入实例，不执行
"$B" pipeline arm "$ID" --confirm        # 用户确认全部钱包/金额/目标/滑点/步骤后才执行
"$B" pipeline status "$ID"               # 轮询；短间隔，不要长 sleep 干等
"$B" pipeline cancel "$ID"
"$B" pipeline reconcile "$ID"            # 只读对账
"$B" pipeline resume "$ID" --confirm     # 只续跑 Pending
```

### 状态怎么读

- `status`：`LOADED` → `RUNNING` → `COMPLETED`；异常时 `PAUSED` / `INTERRUPTED` / `CANCELLED` / `PERSISTENCE_ERROR`；`reconcile` 后可能为 `RECONCILED`（可 resume）。**RUNNING 不代表已就绪**。
- `results.phase`：`PREPARING`（额度准备，最多 120 秒）→ `STARTING_LISTENER` → `WAITING`（监听就绪，此时才通知用户）→ `EXECUTING`。
- 每个成员的结果含 txHash、`walletExecuteMs`、`hotPathTiming`、`stateReadStats`；这些是本地分段计时，不是事件到上链延迟。

### arm 做了什么

1. 绑定各钱包目录/会话/identity 指纹（之后重新登录或换 identity，旧任务不能继续用）。
2. 当前步骤开始监听前：读一次额度；不足且 argv 含 `--auto-approve` → **立即精确授权本次数量并花 Gas**，只等首个成功回执；不足且没授权许可 → 停止，不开监听。
3. 预编译固定动作；触发后只做 OKX `/swap` + 币安 preview + execute 三次核心请求，不查额度、不补授权。
4. 后续步骤各自在自己的监听前准备，不提前合并授权。

## 暂停与恢复

会暂停的情况：后端拒绝、需要 App 确认、结果未知、只有 orderId 没有 txHash、`PENDING_CONFIRMATION`、到期、WSS 断线/订阅失败（没有自动重连）。
进程崩溃重启后：在途动作变 Uncertain，流水线变 `INTERRUPTED`，不会自动重发。

1. `pipeline reconcile ID`：从结果或本钱包确定性日志里找 txHash，核对链、发送方、规范块、确认数；"查不到"不等于"没提交"。
2. 未知/失败已解决、原截止时间仍有效时，经用户确认 `pipeline resume ID --confirm`：只执行 Pending，Submitted/Confirmed 不重发，不续期。
3. 已取消、已过期、身份变化、准备已开始却没有就绪记录 → resume 拒绝。需要的话用新 id 重新建任务，由用户重新确认。

取消/未触发不会撤销已成功的授权，授权 Gas 也退不回来。
