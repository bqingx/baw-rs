# x402-payment：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `x402-payment`

```text
x402 payments: preview an HTTP 402 challenge, then sign an option

Usage: baw-rs --wallet-dir <WALLET_DIR> x402-payment [OPTIONS] <COMMAND>

Commands:
  preview  Preview x402 payment options from a PaymentRequired response
  sign     Sign a selected x402 payment option and return the replay header value
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

## `x402-payment preview`

```text
Preview x402 payment options from a PaymentRequired response

Usage: baw-rs x402-payment preview [OPTIONS] --paymentRequirements <value>

Options:
      --paymentRequirements <value>
          The PaymentRequired payload — base64 `PAYMENT-REQUIRED` header value or raw JSON (auto-detected)
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

## `x402-payment sign`

```text
Sign a selected x402 payment option and return the replay header value

Usage: baw-rs x402-payment sign [OPTIONS] --paymentId <paymentId> --selectedIndex <index>

Options:
      --paymentId <paymentId>
          Payment id returned by x402-payment preview
      --selectedIndex <index>
          Selected payment option index returned by x402-payment preview
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
