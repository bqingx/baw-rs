# 固定最低到账改为可选

本轮只修改代码和做离线验收/构建，不部署、不改写真实 Task、profile、身份/会话或交易授权，不沿用已完成测试的资金许可。

## 用户层面

默认可表达：监听指定事件，命中后用 1 USDT 换 BNB，滑点 10%，5 分钟内一次。**不再要求额外填写一个监听前就固定的 BNB 到账数量。**

- 不传 `--okx-min-output-units`：按触发/执行时的新 OKX route 的最低值校验，不固定准备期报价。
- 显式传 `--okx-min-output-units N`：仍额外保留该用户固定底线。N 必须为有效正 uint256；显式 0/空值/非法数不视作“关闭”。
- 不用默认 0、1 wei 或无限容忍来冒充可选，不修改用户滑点/税率/输入金额。若监听期间市场变化，默认模式不承诺最初查询时能收到的数量。

对于当前 CLI，无固定底线的完整意图可以先纯本地查看：

```sh
baw-rs market-order intent \
  --binanceChainId 56 \
  --fromToken 0x55d398326f99059ff775485246999027b3197955 \
  --toToken 0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee \
  --fromTokenQty 1 --fromTokenDecimals 18 --slippage 10 --mev false
```

这不是执行命令。真实 swap/arm 仍要求原确认、操作 ID、risk 接受、额度/权限/模拟和防重流程；可选底线没有自动授予任何资金权限。

## 必须保留的执行保护

每次触发只获取当前 fresh route，取：

- `out`：当前 OKX 报价的输出数量；
- `reported`：当前 route 的 `tx.minReceiveAmount`；
- `relative`：当前 `out` 按用户明确滑点计算的整数 floor；
- `fixed`：仅用户明确填写时存在的额外固定值。

必须同时满足：

```text
reported 存在、是合法 uint256 且 > 0
relative <= reported <= out
如果 fixed 存在：reported >= fixed
成功的新 Preview：0 < 钱包输入扣款 <= 本次输入
成功的新 Preview：钱包税后净到账 >= reported
```

所以省略 fixed 绝不允许缺失/零/非法/过低/高于报价的 provider minimum，也不能跳过模拟或接受净到账不足。relative 的计算继续精确整数运算；不改 OKX calldata、不本地解码 ABI/selector，不声称独立验证过内嵌链上最低值。API 元数据与模拟都不是未来成交保证。

监听前仍做代币/税率/风险/价格影响与必要额度准备。准备报价只做这些检查，**不从它自动生成一个固定最低值**。显式 fixed 仍会在必要授权前检查当时预报价是否已经低于该底线，避免明显无效任务花授权 Gas。

## 绑定与历史

`SwapArgs.okx_min_output_units` 始终是 Option 并完整进入 durable intent/prepared binding：

- None、Some(A)、Some(B) 不同，不能把旧 proof 的固定值删掉或替换后复用。
- 新模式没有修改 calldata 信任模型，继续 pinned_okx_opaque_v3；既有显式 floor 的参数/绑定不改写、不自动升级语义。
- 旧状态、已授权任务、原监听/准备截止、取消和未知状态防重不变。修改代码不等于修改或续开任何真实任务。
- 触发成功路径仍为一次 OKX swap、一次 Preview、一次 Execute；不新增查询、修复、重试、补授权或重算准备期底线。

结果与本地意图摘要增加 `minimumOutputPolicy`：mode 为 `fresh_route` 或 `fresh_route_plus_fixed_floor`，fixedUserMinimumUnits 在缺省时为 null。实际执行结果另记录当前 reported/slippage/runtime 净最低值，`onchainMinimumVerified=false`。原显式字符串保持不变，报告规范化值不修改已绑定参数。

## 验收

- CLI 缺省通过，显式 0/非法值仍失败；None/Some/不同值进入不同意图。
- 独立及 prepared 无固定 floor 成功，准备过的触发仍严格三核心 HTTP。
- 缺/零/舍入成零/畸形/低于滑点/高于报价的 fresh reported，以及模拟净额不足，都在 Execute 前停止。
- Some 固定底线仍阻止更低 reported；旧明确参数没有被改写。
- prepared proof 改动 floor 模式或数值在拉 route 前本地拒绝。
- 控制器：准备报价输出 2e15，触发时变成 1.6e15、reported 1.584e15、模拟净到账 1.6e15；None 正常执行，不冻结准备期 1.98e15 floor，不偷偷合成 1 wei。
- 带税净额验证、后台未知/失败暂停、deadline/cancel/身份绑定、一次性 preview、提交前 journal 全部保留。

当前全量 **405 Rust passed / 0 failed / 1 manual benchmark ignored**，Clippy -D warnings 通过。日志 `/tmp/baw-optional-minimum-tests.log`、`/tmp/baw-optional-minimum-clippy.log`、`/tmp/baw-optional-minimum-release.log`。本轮无实盘测试。
