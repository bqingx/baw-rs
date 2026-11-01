# 普通钱包 CLI 全局参数

```text
Native wallet client with explicit consent and preview-bound signing

Usage: baw-rs [OPTIONS] <COMMAND>

Commands:
  task           Native event task control for the wallet named by --wallet
  auth           
  market-order   
  contract-call  
  sign-message   
  wallet         
  approvals      Token approvals: list, detail, revoke
  cli-check      Check whether the installed CLI meets a required version
  skill-check    Check whether a skill has a newer version available
  wallets        Wallet lifecycle and named resident daemon. Never requires --wallet
  serve          Resident server: reuse one session and connection pool over a private socket
  help           Print this message or the help of the given subcommand(s)

Options:
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --wallet-dir <WALLET_DIR>
          Legacy single-wallet directory (compatibility; with --instance-id-file) [env: BINANCE_BAW_DIR=]
      --instance-id <INSTANCE_ID>
          [env: BINANCE_INSTANCE_ID=] [default: ""]
      --instance-id-file <PATH>
          Read the instance identity from a private (0600) file instead of the command line
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
  -V, --version
          Print version

Named wallets: baw-rs --wallet alice wallet status (needs `baw-rs wallets serve`).
Wallet lifecycle: baw-rs wallets --help (never requires --wallet).
Legacy single wallet: --wallet-dir plus --instance-id-file.
```
