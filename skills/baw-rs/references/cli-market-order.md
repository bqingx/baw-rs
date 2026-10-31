# market-order：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `market-order`

```text
Usage: baw-rs --wallet-dir <WALLET_DIR> market-order [OPTIONS] <COMMAND>

Commands:
  quote  
  swap   
  list   
  help   Print this message or the help of the given subcommand(s)

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

## `market-order quote`

```text
Usage: baw-rs market-order quote [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --fromToken <FROM_TOKEN> --toToken <TO_TOKEN> --fromTokenQty <FROM_TOKEN_QTY>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --fromToken <FROM_TOKEN>
          
      --toToken <TO_TOKEN>
          
      --fromTokenQty <FROM_TOKEN_QTY>
          
      --json
          
      --slippage <SLIPPAGE>
          [default: auto]
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

## `market-order swap`

```text
Usage: baw-rs market-order swap [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --fromToken <FROM_TOKEN> --toToken <TO_TOKEN> --fromTokenQty <FROM_TOKEN_QTY>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --fromToken <FROM_TOKEN>
          
      --toToken <TO_TOKEN>
          
      --fromTokenQty <FROM_TOKEN_QTY>
          
      --json
          
      --slippage <SLIPPAGE>
          [default: auto]
      --timing
          
      --confirm
          
      --mev <MEV>
          [default: true]
      --gasLevel <GAS_LEVEL>
          [default: MEDIUM]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --acknowledge-token-risk
          Explicit risk acceptance; this does not represent a token audit
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `market-order list`

```text
Usage: baw-rs market-order list [OPTIONS]

Options:
      --orderId <ORDER_ID>
          
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --status <STATUS>
          
      --fromToken <FROM_TOKEN>
          
      --json
          
      --timing
          
      --toToken <TO_TOKEN>
          
      --confirm
          
      --startTime <START_TIME>
          
      --endTime <END_TIME>
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --page <PAGE>
          [default: 1]
      --pageSize <PAGE_SIZE>
          [default: 20]
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
