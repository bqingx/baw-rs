# signal：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `signal`

```text
Custom signal strategies and backtests

Usage: baw-rs --wallet-dir <WALLET_DIR> signal [OPTIONS] <COMMAND>

Commands:
  list          Query custom signal feed (all sources by default)
  strategy      Manage custom signal strategies (create / update / delete / follow)
  backtest      Backtest management (list / detail / retry / schedule)
  explore       Explore official signal strategies
  credits       Query backtest credits info
  wallet-group  List wallet groups (for fomo-call --wallet-group-id)
  help          Print this message or the help of the given subcommand(s)

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

## `signal list`

```text
Query custom signal feed (all sources by default)

Usage: baw-rs signal list [OPTIONS]

Options:
  -c, --chain-id <chainId>
          Chain ID [default: 56]
  -n, --page-size <size>
          Number of signals per source [default: 100]
  -s, --source <source>
          Signal source: all | user | meme | smart-money [default: all]
      --json
          
      --strategy-id <id>
          Filter by strategy ID (USER_STRATEGY only)
      --strategy-type <type>
          Filter by strategy type: meme-rush | fomo-call (aliases: meme, fomo)
      --timing
          
      --confirm
          
      --sort-by <field>
          Sort: time | maxGain [default: time]
      --operation-id <OPERATION_ID>
          
      --time-range <range>
          Time filter: 5m | 1h | 24h
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `signal strategy`

```text
Manage custom signal strategies (create / update / delete / follow)

Usage: baw-rs signal strategy [OPTIONS] <COMMAND>

Commands:
  create         Create a new signal strategy (estimate → confirm → create)
  update         Update an existing strategy name or config
  delete         Delete a strategy (backtest task)
  follow         Follow a strategy. Hall strategies are copied to your own account first
  unfollow       Unfollow a strategy
  list           List all your strategies (owned), optionally filtered by --followed or --type
  list-followed  List strategies you follow
  help           Print this message or the help of the given subcommand(s)

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

## `signal strategy create`

```text
Create a new signal strategy (estimate → confirm → create)

Usage: baw-rs signal strategy create [OPTIONS] --chain-id <chainId> --type <type> --name <name> --config <config>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
  -n, --name <name>
          Strategy name (max 20 chars)
      --config <config>
          Strategy config (JSON string)
      --json
          
      --timing
          
      --wallet-group-id <groupId>
          Wallet group ID (required for fomo-call, merged into config.selectedGroups)
      --confirm
          
      --run-backtest
          Trigger backtest on creation (sets config.backtest.enabled=true)
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `signal strategy update`

```text
Update an existing strategy name or config

Usage: baw-rs signal strategy update [OPTIONS] --chain-id <chainId> --type <type> --job-id <jobId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
      --job-id <jobId>
          Job ID
      --json
          
  -n, --name <name>
          New strategy name
      --config <config>
          New strategy config (JSON string)
      --timing
          
      --confirm
          
  -y, --yes
          Skip confirmation prompt
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `signal strategy delete`

```text
Delete a strategy (backtest task)

Usage: baw-rs signal strategy delete [OPTIONS] --chain-id <chainId> --type <type> --job-id <jobId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
      --job-id <jobId>
          Job ID
      --json
          
  -y, --yes
          Skip confirmation prompt
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

## `signal strategy follow`

```text
Follow a strategy. Hall strategies are copied to your own account first

Usage: baw-rs signal strategy follow [OPTIONS] --chain-id <chainId> --type <type> --job-id <jobId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
      --job-id <jobId>
          Job ID
      --json
          
      --task-id <taskId>
          Task ID (used to look up the strategy; hall strategies default to 1) [default: 1]
  -n, --name <name>
          Name for the copied strategy (only used when copying a hall strategy; max 20 chars)
      --timing
          
      --confirm
          
  -y, --yes
          Skip the copy confirmation prompt for hall strategies
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `signal strategy unfollow`

```text
Unfollow a strategy

Usage: baw-rs signal strategy unfollow [OPTIONS] --chain-id <chainId> --type <type> --strategy-id <strategyId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
      --strategy-id <strategyId>
          Strategy ID
      --json
          
  -y, --yes
          Skip confirmation prompt
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

## `signal strategy list`

```text
List all your strategies (owned), optionally filtered by --followed or --type

Usage: baw-rs signal strategy list [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
      --followed
          Only show followed strategies
      --type <strategyType>
          Filter by strategy type: meme-rush | fomo-call
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

## `signal strategy list-followed`

```text
List strategies you follow

Usage: baw-rs signal strategy list-followed [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
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

## `signal backtest`

```text
Backtest management (list / detail / retry / schedule)

Usage: baw-rs signal backtest [OPTIONS] <COMMAND>

Commands:
  list      List backtest task stats
  detail    Show backtest detail (task info + token list)
  retry     Retry a failed backtest job (meme-rush only)
  schedule  Configure or query backtest schedule
  help      Print this message or the help of the given subcommand(s)

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

## `signal backtest list`

```text
List backtest task stats

Usage: baw-rs signal backtest list [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -p, --page <page>
          Page number (ignored when --all is used) [default: 1]
  -s, --size <size>
          Page size [default: 20]
      --backtest-days <days>
          Backtest period in days
      --json
          
      --all
          Fetch all pages automatically (overrides --page/--size)
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

## `signal backtest detail`

```text
Show backtest detail (task info + token list)

Usage: baw-rs signal backtest detail [OPTIONS] --chain-id <chainId> --strategy-id <strategyId>

Options:
  -c, --chain-id <chainId>
          Chain ID
      --strategy-id <strategyId>
          Strategy ID
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

## `signal backtest retry`

```text
Retry a failed backtest job (meme-rush only)

Usage: baw-rs signal backtest retry [OPTIONS] --chain-id <chainId> --job-id <jobId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --type <type>
          Strategy type (default: meme-rush) [default: meme-rush]
      --job-id <jobId>
          Job ID
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

## `signal backtest schedule`

```text
Configure or query backtest schedule

Usage: baw-rs signal backtest schedule [OPTIONS] --chain-id <chainId> --job-id <jobId>

Options:
  -c, --chain-id <chainId>
          Chain ID
      --job-id <jobId>
          Job ID
      --interval <interval>
          Schedule interval: 4H | 6H | 12H | 24H | OFF (omit to query only)
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

## `signal explore`

```text
Explore official signal strategies

Usage: baw-rs signal explore [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
      --backtest-days <days>
          Backtest period in days
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

## `signal credits`

```text
Query backtest credits info

Usage: baw-rs signal credits [OPTIONS]

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

## `signal wallet-group`

```text
List wallet groups (for fomo-call --wallet-group-id)

Usage: baw-rs signal wallet-group [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
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
