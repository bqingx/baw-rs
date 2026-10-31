# tracker：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `tracker`

```text
Wallet tracker: groups, addresses, monitors

Usage: baw-rs --wallet-dir <WALLET_DIR> tracker [OPTIONS] <COMMAND>

Commands:
  token    Token-dimension monitor (agent own group / public SMY-KOL)
  tx       Transaction-dimension monitor (agent own group / public SMY-KOL)
  follow   List addresses the current user follows (read-only)
  group    Manage tracker address groups
  address  Manage tracker addresses
  ws       Subscribe to WSP push events via WebSocket
  help     Print this message or the help of the given subcommand(s)

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

## `tracker token`

```text
Token-dimension monitor (agent own group / public SMY-KOL)

Usage: baw-rs tracker token [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID (agent own group)
      --tag-type <type>
          Public mode tag: kol / smy (omit for agent own-group mode)
      --json
          
      --token-size <n>
          Number of recent tokens (default 70)
      --period <period>
          Period: 1m/5m/1h/4h/24h (default 24h) [default: 24h]
      --timing
          
      --confirm
          
      --filter-risk
          Filter risk tokens
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `tracker tx`

```text
Transaction-dimension monitor (agent own group / public SMY-KOL)

Usage: baw-rs tracker tx [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID (agent own group)
      --tag-type <type>
          Public mode tag: kol / smy (omit for agent own-group mode)
      --json
          
      --trade-side <sides>
          Comma-separated: 19=buy 11=first-buy 29=sell 21=clear
      --min-value <n>
          Min tx USD value
      --timing
          
      --confirm
          
      --max-value <n>
          Max tx USD value
      --filter-risk
          Filter risk tokens
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `tracker follow`

```text
List addresses the current user follows (read-only)

Usage: baw-rs tracker follow [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
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

## `tracker group`

```text
Manage tracker address groups

Usage: baw-rs tracker group [OPTIONS] <COMMAND>

Commands:
  create  Create a new group
  list    List address groups
  update  Rename a group
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

## `tracker group create`

```text
Create a new group

Usage: baw-rs tracker group create [OPTIONS] --chain-id <chainId> --name <name>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -n, --name <name>
          Group name
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

## `tracker group list`

```text
List address groups

Usage: baw-rs tracker group list [OPTIONS] --chain-id <chainId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
      --no-all-group
          Exclude the "All" virtual group
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

## `tracker group update`

```text
Rename a group

Usage: baw-rs tracker group update [OPTIONS] --chain-id <chainId> --group-id <groupId> --name <name>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID
  -n, --name <name>
          New group name
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

## `tracker address`

```text
Manage tracker addresses

Usage: baw-rs tracker address [OPTIONS] <COMMAND>

Commands:
  add       Add a single address (wraps import)
  batch     Batch import addresses into a group
  delete    Delete address(es) from a group (supports batch)
  follow    Follow an address (G9-A)
  link      Link an address to a group (address/group/link)
  list      Paginated address list (address/page)
  search    Fuzzy search addresses within a group
  unfollow  Unfollow an address (G9-A)
  update    Update address label/note (merges address/update + address/label/update)
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

## `tracker address add`

```text
Add a single address (wraps import)

Usage: baw-rs tracker address add [OPTIONS] --chain-id <chainId> --address <address>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --address <address>
          Address
  -l, --label <label>
          Label
  -g, --group-id <groupId>
          Group ID (default group if omitted)
      --json
          
      --no-overwrite
          Do not overwrite existing label/emoji
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

## `tracker address batch`

```text
Batch import addresses into a group

Usage: baw-rs tracker address batch [OPTIONS] --chain-id <chainId> --addresses <list>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --addresses <list>
          Comma-separated addresses, or JSON array of {address,label}
  -g, --group-id <groupId>
          Group ID (default group if omitted)
      --json
          
      --no-overwrite
          Do not overwrite existing label/emoji
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

## `tracker address delete`

```text
Delete address(es) from a group (supports batch)

Usage: baw-rs tracker address delete [OPTIONS] --chain-id <chainId> --group-id <groupId> --addresses <list>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID
  -a, --addresses <list>
          Comma-separated addresses to delete
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

## `tracker address follow`

```text
Follow an address (G9-A)

Usage: baw-rs tracker address follow [OPTIONS] --chain-id <chainId> --address <address>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --address <address>
          Address
  -g, --group-id <groupId>
          Group ID
      --json
          
  -l, --label <label>
          Label
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

## `tracker address link`

```text
Link an address to a group (address/group/link)

Usage: baw-rs tracker address link [OPTIONS] --chain-id <chainId> --address <address>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --address <address>
          Address
  -g, --group-id <groupId>
          Target group ID (default group if omitted)
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

## `tracker address list`

```text
Paginated address list (address/page)

Usage: baw-rs tracker address list [OPTIONS] --chain-id <chainId> --group-id <groupId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID
      --page <page>
          Page number (from 1) [default: 1]
      --json
          
      --size <size>
          Page size (max 100) [default: 20]
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

## `tracker address search`

```text
Fuzzy search addresses within a group

Usage: baw-rs tracker address search [OPTIONS] --chain-id <chainId> --group-id <groupId>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -g, --group-id <groupId>
          Group ID
  -a, --address <address>
          Address filter (fuzzy)
      --json
          
  -l, --label <label>
          Label filter (fuzzy)
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

## `tracker address unfollow`

```text
Unfollow an address (G9-A)

Usage: baw-rs tracker address unfollow [OPTIONS] --chain-id <chainId> --address <address>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --address <address>
          Address
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

## `tracker address update`

```text
Update address label/note (merges address/update + address/label/update)

Usage: baw-rs tracker address update [OPTIONS] --chain-id <chainId> --address <address> --label <label>

Options:
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  -a, --address <address>
          Address
  -l, --label <label>
          New label/note
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

## `tracker ws`

```text
Subscribe to WSP push events via WebSocket

Usage: baw-rs tracker ws [OPTIONS]

Options:
      --smy
          Listen to Smart Money events
      --kol
          Listen to KOL events (requires -c)
      --wallet <chains>
          Listen to chain-level events (comma-separated: BSC,SOL,BASE,ETH)
      --following
          Listen to current user followings (requires login + -c, mutually exclusive with others)
      --json
          
      --address <addr>
          Listen to specific address events (requires -c)
      --timing
          
      --address-list <file>
          Batch address file, one per line (requires -c, max 100)
      --confirm
          
  -c, --chain-id <chainId>
          Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --duration <sec>
          Auto-disconnect after N seconds (0 or omit = no limit)
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
