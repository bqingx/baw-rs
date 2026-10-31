# OpenFour 内盘与单钱包极速路径

## 选择正确路线

- 仅当目标确认为相应 OpenFour Agent ID 专属内盘代币时，使用其合约调用路径；普通 DEX 交易另走 market-order。
- 不把站点上“模板启用”、单个代币 paused、代币交易 phase 混为一项。假设场景按假设分析，不回头要求现实钱包准备。
- 脚本侧报价参考是币安提供的 agent-id-exclusive-token-launches.mjs，不是“只有币安能算”的私有算法。
  链上 ABI 决定 calldata；baw-rs quote-buy 是独立读链实现。查询和编码不涉及本地私钥。
- 不硬编码“还没真实代币”“固定出块三秒”“固定1.37秒下限”等旧断言。状态信息必须实时核实。

## 三条构造路线

1. `openfour quote-buy --token ... --wallet ... --amount <BNB小数> --slippage-bps ...`：读链询价后编码。
   此路径当前仅原生BNB/WBNB，旧报价解码仍有u128边界，不用于宣称任意uint256的正确读链结果。
2. `openfour encode-buy` / `fast build`：纯离线，预先知道 Core/token/from、预算 wei、绝对最低得到量；
   不探测链、模板、报价或余额，不查 RPC，因此调用方必须提前确认适用性。
3. `fast serve`：持久进程内接订单对象，离线编码→内存预览→执行，复用同一会话与HTTP Client。

无报价的 minAmountOut 是**绝对最低数量**，不是百分比滑点。不是技术上必须等触发后询价才能编码。
零最低量需额外 allowZeroMinOut:true，表示明确放弃最低数量保护，不能自动替用户设定。
模板限定：BSC56、BNB付款/WBNB报价、buyByBudget、options=0、空proof。其他模板/支付资产不得直接套。

## 持久通道

```sh
"$B" fast serve --wallet-dir "$WALLET_DIR" --instance-id-file "$IDENTITY_FILE"
```

每行一单，末尾换行，逐行读取响应。以下**全是合成地址，不可真实交易**：

```json
{"operationId":"EXAMPLE-ONLY-001","confirm":true,"skipPrechecks":true,"buy":{"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"100000000000000","minAmountOut":"1000000000000000000"}}
```

- 最少HTTP模式每笔显式 skipPrechecks:true；默认false。新路径不依赖旧全局skip开关。
- 数量是十进制/0x**字符串**；支持完整uint256，溢出拒绝，地址必须完整且格式合规。
- confirm必须true且ID唯一；这不能替代用户对钱包、目标、预算、数量下限及触发条件的明确授权。
- 路径不写/读完整preview JSON；仍保留意图日志与一次性消费标记，不是零磁盘写入。
- 会话文件/identity在开始及execute前再次固定身份校验；变化后停止，不偷偷重登/重放。
- 新路径单钱包串行；不能为了宣称并发快而暗改本地所有权/后端状态。
- 后续坏帧不应取消已经发出的execute，先排空已接受订单；逐行响应与日志必须检查。
- 模拟明确成功、过期/绑定校验通过才发送execute；未知execute响应标记FAILED_OR_UNKNOWN。
- JSONL帧上限64KiB、队列16。调用方一问一答，避免预先塞入大量不可撤回订单。
- 外部脚本可持续持有一个fast子进程；若仓库提供 examples/fast_driver.py，先读代码再使用。
  它需要显式--execute，不是dry-run，超时不是链上取消，不自动重试。

## 计时与连接

每单 timings记录 encode/local_prep/journal/consume/state_check/queue/prechecks/preview/execute/total。
HTTP full_roundtrip 包括建连（如发生）、网络、后端等待、body drain和JSON解析，**不是纯后端处理**。
request_ms到headers，body_ms到body/解析完成；是full_roundtrip的细分，不能重复相加。
连接池复用取决于实际连接寿命；首次/重连仍可握手。用测试服务器accept计数证明mock复用，不能用静态ss证明每次请求。
真实延迟需在授权预算内跑实际选用最快路径；不为了采样无限真钱下单，也不把loopback结果说成生产下限。
