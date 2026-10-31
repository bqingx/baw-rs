# 普通钱包 CLI 全局参数

```text
Native wallet client with explicit consent and preview-bound signing

Usage: baw-rs [OPTIONS] --wallet-dir <WALLET_DIR> <COMMAND>

Commands:
  auth           
  market-order   
  contract-call  
  sign-message   
  wallet         
  approvals      Token approvals: list, detail, revoke
  limit-order    Limit orders: buy, sell, list, cancel
  prediction     Prediction markets: discovery, positions, trading
  defi           DeFi: protocols, opportunities, positions, deposit/redeem/LP/claim
  x402-payment   x402 payments: preview an HTTP 402 challenge, then sign an option
  tracker        Wallet tracker: groups, addresses, monitors
  signal         Custom signal strategies and backtests
  leaderboard    Leaderboard, address analysis and alpha radar
  cli-check      Check whether the installed CLI meets a required version
  skill-check    Check whether a skill has a newer version available
  batch          Read one JSON argv array per stdin line, retaining the in-memory session
  serve          Resident server: reuse one session and connection pool over a private socket
  help           Print this message or the help of the given subcommand(s)

Options:
      --wallet-dir <WALLET_DIR>
          [env: BINANCE_BAW_DIR=]
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

Named multi-wallet daemon: baw-rs vaults --help (does not require --wallet-dir).
```
