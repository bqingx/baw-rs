# leaderboard：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `leaderboard`

```text
Leaderboard, address analysis and alpha radar

Usage: baw-rs --wallet-dir <WALLET_DIR> leaderboard [OPTIONS] <COMMAND>

Commands:
  query               Query leaderboard (top traders)
  analyze             Analyze a single address (reverse-lookup within top N)
  alpha-radar         Alpha radar query (find addresses holding target tokens)
  preset              Manage leaderboard filter presets
  alpha-radar-config  Manage alpha radar configs
  help                Print this message or the help of the given subcommand(s)

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

## `leaderboard query`

```text
Query leaderboard (top traders)

Usage: baw-rs leaderboard query [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -p, --period <period>
          Period: 7d/30d/90d [default: 30d]
  -t, --tag <tag>
          Tag: ALL/KOL/MPC [default: ALL]
      --json
          
      --sort-by <n>
          Sort field (0=PnL 20=win 30=vol 50=tx 60=active 70=rate 80=tokens) [default: 0]
      --order-by <n>
          Order (0/2=desc 1=asc) [default: 0]
      --timing
          
      --confirm
          
      --page <page>
          Page number (from 0) [default: 0]
      --operation-id <OPERATION_ID>
          
      --size <size>
          Page size (max 20) [default: 20]
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `leaderboard analyze`

```text
Analyze a single address (reverse-lookup within top N)

Usage: baw-rs leaderboard analyze [OPTIONS] --chain-id <chainId> --address <address>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -a, --address <address>
          Address to analyze
  -p, --period <period>
          Period: 7d/30d/90d [default: 30d]
      --json
          
      --top-n <n>
          Scan top N entries (default 1000, max 5000) [default: 1000]
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

## `leaderboard alpha-radar`

```text
Alpha radar query (find addresses holding target tokens)

Usage: baw-rs leaderboard alpha-radar [OPTIONS] --chain-id <chainId> --tokens <tokens> --match-count <count>

Options:
  -c, --chain-id <chainId>
          Chain ID
  -t, --tokens <tokens>
          Comma-separated token addresses
  -m, --match-count <count>
          Min number of tokens to match (>=1)
      --json
          
  -p, --period <period>
          Period: 7d/30d/90d [default: 30d]
      --page <page>
          Page number (from 0) [default: 0]
      --timing
          
      --confirm
          
      --size <size>
          Page size (max 20) [default: 20]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `leaderboard preset`

```text
Manage leaderboard filter presets

Usage: baw-rs leaderboard preset [OPTIONS] <COMMAND>

Commands:
  list  List saved presets
  save  Save presets (replaces all; --config is a JSON array of PresetItem)
  help  Print this message or the help of the given subcommand(s)

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

## `leaderboard preset list`

```text
List saved presets

Usage: baw-rs leaderboard preset list [OPTIONS]

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

## `leaderboard preset save`

```text
Save presets (replaces all; --config is a JSON array of PresetItem)

Usage: baw-rs leaderboard preset save [OPTIONS] --config <json>

Options:
      --config <json>
          JSON array of preset items
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

## `leaderboard alpha-radar-config`

```text
Manage alpha radar configs

Usage: baw-rs leaderboard alpha-radar-config [OPTIONS] <COMMAND>

Commands:
  list  List alpha radar configs
  save  Save alpha radar configs (--config is a JSON array of AlphaRadarConfigItem)
  help  Print this message or the help of the given subcommand(s)

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

## `leaderboard alpha-radar-config list`

```text
List alpha radar configs

Usage: baw-rs leaderboard alpha-radar-config list [OPTIONS] --chain-id <chainId>

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

## `leaderboard alpha-radar-config save`

```text
Save alpha radar configs (--config is a JSON array of AlphaRadarConfigItem)

Usage: baw-rs leaderboard alpha-radar-config save [OPTIONS] --chain-id <chainId> --config <config>

Options:
  -c, --chain-id <chainId>
          Chain ID
      --config <config>
          JSON array of AlphaRadarConfigItem
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
