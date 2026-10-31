# wallet：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `wallet`

```text
Usage: baw-rs --wallet-dir <WALLET_DIR> wallet [OPTIONS] <COMMAND>

Commands:
  status          
  address         
  chains          
  settings        
  gas-price       
  tx-lock         
  agent-id        
  balance         Normalized balances (including RWA), or lossless tokenList with --raw
  tx-history      
  left-quota      
  send            
  speed-up        Speed up a pending transaction by resubmitting with higher gas
  cancel          Cancel a pending transaction by sending a replacement tx with higher gas
  resolve-domain  Resolve a domain name (.eth/.bnb/.ai Agent ID) to an address
  help            Print this message or the help of the given subcommand(s)

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

## `wallet status`

```text
Usage: baw-rs wallet status [OPTIONS]

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

## `wallet address`

```text
Usage: baw-rs wallet address [OPTIONS]

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

## `wallet chains`

```text
Usage: baw-rs wallet chains [OPTIONS]

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

## `wallet settings`

```text
Usage: baw-rs wallet settings [OPTIONS]

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

## `wallet gas-price`

```text
Usage: baw-rs wallet gas-price [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
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

## `wallet tx-lock`

```text
Usage: baw-rs wallet tx-lock [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
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

## `wallet agent-id`

```text
Usage: baw-rs wallet agent-id [OPTIONS]

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

## `wallet balance`

```text
Normalized balances (including RWA), or lossless tokenList with --raw

Usage: baw-rs wallet balance [OPTIONS]

Options:
      --raw
          
      --symbol <SYMBOL>
          
      --tokenAddress <TOKEN_ADDRESS>
          
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --json
          
      --hideLowValue <HIDE_LOW_VALUE>
          
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

## `wallet tx-history`

```text
Usage: baw-rs wallet tx-history [OPTIONS]

Options:
      --type <HISTORY_TYPE>
          [default: all]
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --size <SIZE>
          [default: 20]
      --json
          
      --nextCursor <NEXT_CURSOR>
          
      --timing
          
      --tx <TX>
          
      --confirm
          
      --startTime <START_TIME>
          
      --endTime <END_TIME>
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `wallet left-quota`

```text
Usage: baw-rs wallet left-quota [OPTIONS]

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

## `wallet send`

```text
Usage: baw-rs wallet send [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --recipient <RECIPIENT> --tokenAddress <TOKEN_ADDRESS> <--amount <AMOUNT>|--max>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --amount <AMOUNT>
          
      --max
          
      --json
          
      --recipient <RECIPIENT>
          
      --timing
          
      --tokenAddress <TOKEN_ADDRESS>
          
      --confirm
          
      --gasLevel <GAS_LEVEL>
          [default: MEDIUM]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `wallet speed-up`

```text
Speed up a pending transaction by resubmitting with higher gas

Usage: baw-rs wallet speed-up [OPTIONS] <txHash>

Arguments:
  <txHash>  Transaction hash of the pending transaction (0x-prefixed, 66 characters)

Options:
      --level <level>
          Gas bump level: LOW (×1.1), HIGH (×1.25) [default: HIGH]
      --binanceChainId <binanceChainId>
          Binance chain ID (optional, auto-detected from txHash)
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

## `wallet cancel`

```text
Cancel a pending transaction by sending a replacement tx with higher gas

Usage: baw-rs wallet cancel [OPTIONS] <txHash>

Arguments:
  <txHash>  Transaction hash of the pending transaction (0x-prefixed, 66 characters)

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID (optional, auto-detected from txHash)
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

## `wallet resolve-domain`

```text
Resolve a domain name (.eth/.bnb/.ai Agent ID) to an address

Usage: baw-rs wallet resolve-domain [OPTIONS] --name <name> --binanceChainId <binanceChainId>

Options:
      --name <name>
          Domain name to resolve, e.g. alice.eth / alice.bnb
      --binanceChainId <binanceChainId>
          Binance chain ID, e.g. 56 (BSC)
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
