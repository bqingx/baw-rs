# approvals：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `approvals`

```text
Token approvals: list, detail, revoke

Usage: baw-rs --wallet-dir <WALLET_DIR> approvals [OPTIONS] <COMMAND>

Commands:
  list    
  detail  
  revoke  
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

## `approvals list`

```text
Usage: baw-rs approvals list [OPTIONS]

Options:
      --spender <address>
          Filter by spender address
      --filterTypes <types>
          Filter by risk type (high_risk, medium_risk, non_interactive, others)
      --limit <number>
          Number of results to return (default: 20) [default: 20]
      --json
          
      --offset <cursor>
          Pagination cursor from previous query
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

## `approvals detail`

```text
Usage: baw-rs approvals detail [OPTIONS] --binanceChainId <binanceChainId> --tokenContract <address> --spender <address> --type <type>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID
      --tokenContract <address>
          Token contract address
      --spender <address>
          Spender contract address
      --json
          
      --type <type>
          Approval type (approve | permit2)
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

## `approvals revoke`

```text
Usage: baw-rs approvals revoke [OPTIONS] --binanceChainId <binanceChainId> --tokenContract <address> --spender <address> --type <type>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID
      --tokenContract <address>
          Token contract address
      --spender <address>
          Spender contract address
      --json
          
      --type <type>
          Approval type (approve | permit2)
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
