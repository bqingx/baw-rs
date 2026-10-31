# limit-order：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `limit-order`

```text
Limit orders: buy, sell, list, cancel

Usage: baw-rs --wallet-dir <WALLET_DIR> limit-order [OPTIONS] <COMMAND>

Commands:
  buy     Create a limit buy order (buy token at trigger price)
  sell    Create a limit sell order (sell token at trigger price)
  list    List and query limit orders with filters
  cancel  Cancel a pending limit order by its strategy ID. Cannot cancel executed orders
  help    Print this message or the help of the given subcommand(s)

Options:
      --json
          
      --timing
          
      --confirm
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `limit-order buy`

```text
Create a limit buy order (buy token at trigger price)

Usage: baw-rs limit-order buy [OPTIONS] --triggerPrice <triggerPrice> --fromTokenQty <fromTokenQty> --fromToken <fromToken> --toToken <toToken> --binanceChainId <binanceChainId>

Options:
      --triggerPrice <triggerPrice>
          Trigger price in USD (e.g. $100 or 100)
      --fromTokenQty <fromTokenQty>
          Amount to spend (`buy`) or amount of tokens to sell (`sell`)
      --fromToken <fromToken>
          Source token address (`buy`) or token to sell (`sell`)
      --json
          
      --toToken <toToken>
          Token to buy (address) (`buy`) or destination token address (`sell`)
      --slippage <slippage>
          Slippage tolerance: "auto" (default) or 0-100 [default: auto]
      --timing
          
      --confirm
          
      --gasLevel <gasLevel>
          Gas level: LOW, MEDIUM (default), HIGH [default: MEDIUM]
      --mev <mev>
          MEV protection: true (default) or false [default: true]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --binanceChainId <binanceChainId>
          Binance chain ID
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `limit-order sell`

```text
Create a limit sell order (sell token at trigger price)

Usage: baw-rs limit-order sell [OPTIONS] --triggerPrice <triggerPrice> --fromTokenQty <fromTokenQty> --fromToken <fromToken> --toToken <toToken> --binanceChainId <binanceChainId>

Options:
      --triggerPrice <triggerPrice>
          Trigger price in USD (e.g. $100 or 100)
      --fromTokenQty <fromTokenQty>
          Amount to spend (`buy`) or amount of tokens to sell (`sell`)
      --fromToken <fromToken>
          Source token address (`buy`) or token to sell (`sell`)
      --json
          
      --toToken <toToken>
          Token to buy (address) (`buy`) or destination token address (`sell`)
      --slippage <slippage>
          Slippage tolerance: "auto" (default) or 0-100 [default: auto]
      --timing
          
      --confirm
          
      --gasLevel <gasLevel>
          Gas level: LOW, MEDIUM (default), HIGH [default: MEDIUM]
      --mev <mev>
          MEV protection: true (default) or false [default: true]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --binanceChainId <binanceChainId>
          Binance chain ID
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `limit-order list`

```text
List and query limit orders with filters

Usage: baw-rs limit-order list [OPTIONS]

Options:
      --strategyId <id>
          Query specific limit order by strategy ID (ignores other filters)
      --binanceChainId <binanceChainId>
          Filter by binanceChainId
      --status <status>
          Filter by order status: PENDING, WORKING, TRIGGERED, FINISHED, FAILED, EXPIRED, or CANCELED
      --fromToken <address>
          Filter by source token contract address
      --json
          
      --timing
          
      --toToken <address>
          Filter by target token contract address
      --confirm
          
      --startTime <timestamp>
          Filter orders created after this time (milliseconds timestamp)
      --endTime <timestamp>
          Filter orders created before this time (milliseconds timestamp)
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --page <page>
          Page number for pagination [default: 1]
      --pageSize <size>
          Number of orders per page (max: 100) [default: 20]
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `limit-order cancel`

```text
Cancel a pending limit order by its strategy ID. Cannot cancel executed orders

Usage: baw-rs limit-order cancel [OPTIONS] --strategyId <strategyId>

Options:
      --strategyId <strategyId>
          Strategy ID of the limit order to cancel
      --json
          
      --timing
          
      --confirm
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
