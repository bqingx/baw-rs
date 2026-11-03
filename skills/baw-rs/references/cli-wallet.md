# wallet：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`（或传统 `--wallet-dir` 组合）；`wallets/openfour` 各有独立参数入口。

## `wallet`

```text
Usage: baw-rs wallet [OPTIONS] <COMMAND>

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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
          
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
          
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
          
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --symbol <SYMBOL>
          
      --tokenAddress <TOKEN_ADDRESS>
          
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --hideLowValue <HIDE_LOW_VALUE>
          
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

## `wallet tx-history`

```text
Usage: baw-rs wallet tx-history [OPTIONS]

Options:
      --type <HISTORY_TYPE>
          [default: all]
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --size <SIZE>
          [default: 20]
      --nextCursor <NEXT_CURSOR>
          
      --tx <TX>
          
      --json
          
      --startTime <START_TIME>
          
      --endTime <END_TIME>
          
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

## `wallet left-quota`

```text
Usage: baw-rs wallet left-quota [OPTIONS]

Options:
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
          
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --amount <AMOUNT>
          
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --max
          
      --recipient <RECIPIENT>
          
      --tokenAddress <TOKEN_ADDRESS>
          
      --gasLevel <GAS_LEVEL>
          [default: MEDIUM]
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

## `wallet speed-up`

```text
Speed up a pending transaction by resubmitting with higher gas

Usage: baw-rs wallet speed-up [OPTIONS] <txHash>

Arguments:
  <txHash>  Transaction hash of the pending transaction (0x-prefixed, 66 characters)

Options:
      --level <level>
          Gas bump level: LOW (×1.1), HIGH (×1.25) [default: HIGH]
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --binanceChainId <binanceChainId>
          Binance chain ID (optional, auto-detected from txHash)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --binanceChainId <binanceChainId>
          Binance chain ID, e.g. 56 (BSC)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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

## `wallet operation`

```text
Read-only view of the local intent journal: was this operation id submitted?

Usage: baw-rs wallet operation [OPTIONS]

Options:
      --operation-id <ID>
          Operation id to explain (the same id passed to a guarded write)
      --list
          List every operation id recorded for this wallet, newest last
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json
      --timing
      --confirm
      --operation-id <OPERATION_ID>
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
  -h, --help
          Print help
```

本地读取 `.baw-rs-operations/` 下的意图记录，**不联网、不解密、不写文件**。用于回答“这笔到底提交没有”：

| `verdict` | 含义 | `retrySafe` |
|---|---|---|
| `UNCERTAIN` | 记录仍是 `SUBMITTING`：进程在请求与响应之间结束，或响应未到 | 恒为 `false` |
| `RESPONSE_RECEIVED` | 已拿到后端响应，附 `recovery`（`txHash`/`orderId`/`status`，若存在） | 恒为 `false` |
| `FAILED_OR_UNKNOWN` | 后端拒绝或传输失败 | 恒为 `false` |
| `UNRECOGNISED` | 记录格式非本版本理解，需人工查看 | 恒为 `false` |

`nextStep` 给出下一步该做什么（例如用 `wallet tx-history` 对账、不要用新 operation-id 重发）。目录/文件必须属主、0700/0600、非符号链接，否则拒绝读取；缺失的 id 返回错误而不是空结果。`--operation-id` 与 `--list` 互斥。
