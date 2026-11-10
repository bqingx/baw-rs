# pipeline：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`；`pipeline` 属于实例，不带钱包；`wallets/openfour` 各有独立参数入口。

## `pipeline`

```text
Instance-owned multi-wallet tasks and sequential pipelines; no --wallet

Usage: baw-rs pipeline [OPTIONS] <COMMAND>

Commands:
  describe   Describe a fixed definition locally, without loading/arming it or granting a deadline
  load
  arm        Prepare allowances before enabling triggers; authorized approvals may spend gas now
  status
  cancel
  reconcile  Read-only chain reconciliation of the active task; never submits a write
  resume     Continue only pending actions, without replay or deadline renewal
  help       Print this message or the help of the given subcommand(s)

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

## `pipeline describe`

```text
Describe a fixed definition locally, without loading/arming it or granting a deadline

Usage: baw-rs pipeline describe [OPTIONS] --config <CONFIG>

Options:
      --config <CONFIG>

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

## `pipeline load`

```text
Usage: baw-rs pipeline load [OPTIONS] --config <CONFIG>

Options:
      --config <CONFIG>

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

## `pipeline arm`

```text
Prepare allowances before enabling triggers; authorized approvals may spend gas now

Usage: baw-rs pipeline arm [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --confirm
          Confirm pre-trigger preparation and all fixed task actions
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `pipeline status`

```text
Usage: baw-rs pipeline status [OPTIONS] <ID>

Arguments:
  <ID>

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

## `pipeline cancel`

```text
Usage: baw-rs pipeline cancel [OPTIONS] <ID>

Arguments:
  <ID>

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

## `pipeline reconcile`

```text
Read-only chain reconciliation of the active task; never submits a write

Usage: baw-rs pipeline reconcile [OPTIONS] <ID>

Arguments:
  <ID>

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

## `pipeline resume`

```text
Continue only pending actions, without replay or deadline renewal

Usage: baw-rs pipeline resume [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --confirm

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
