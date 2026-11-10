# sign-message：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`；`pipeline` 属于实例，不带钱包；`wallets` 管理注册表与 daemon，也不带钱包。

## `sign-message`

```text
Usage: baw-rs sign-message [OPTIONS] <COMMAND>

Commands:
  preview
  execute
  result
  history
  help     Print this message or the help of the given subcommand(s)

Options:
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `sign-message preview`

```text
Usage: baw-rs sign-message preview [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --message <MESSAGE> --signType <SIGN_TYPE>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --message <MESSAGE>

      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --signType <SIGN_TYPE>

      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `sign-message execute`

```text
Usage: baw-rs sign-message execute [OPTIONS] --requestId <REQUEST_ID>

Options:
      --requestId <REQUEST_ID>

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `sign-message result`

```text
Usage: baw-rs sign-message result [OPTIONS] --order-id <ORDER_ID>

Options:
      --order-id <ORDER_ID>

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `sign-message history`

```text
Usage: baw-rs sign-message history [OPTIONS]

Options:
      --binanceChainId <BINANCE_CHAIN_ID>

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --limit <LIMIT>
          [default: 20]
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --nextToken <NEXT_TOKEN>

      --startTime <START_TIME>

      --timing

      --confirm

      --endTime <END_TIME>

      --operation-id <OPERATION_ID>

      --sortType <SORT_TYPE>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
