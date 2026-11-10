# 普通钱包 CLI 全局参数

```text
Native wallet client with explicit consent and preview-bound signing

Usage: baw-rs [OPTIONS] <COMMAND>

Commands:
  pipeline       Instance-owned multi-wallet tasks and sequential pipelines; no --wallet
  auth
  market-order
  contract-call
  sign-message
  wallet
  approvals      Token approvals: list, detail, revoke
  cli-check      Check whether the installed CLI meets a required version
  skill-check    Check whether a skill has a newer version available
  wallets        Wallet lifecycle and named resident daemon. Never requires --wallet
  help           Print this message or the help of the given subcommand(s)

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
  -V, --version
          Print version

Named wallets: baw-rs --wallet alice wallet status (needs `baw-rs wallets serve`).
Wallet lifecycle: baw-rs wallets --help (never requires --wallet).
```
