# contract-call：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `contract-call`

```text
Usage: baw-rs --wallet-dir <WALLET_DIR> contract-call [OPTIONS] <COMMAND>

Commands:
  preview  
  execute  
  run      Preview then execute in one process, only if the pinned policy matches
  fire     Preview then execute in one process with no policy gate and no human digest review. Consent is still `--confirm` + a unique `--operation-id`; the preview's risks and authority changes are returned in the output for the caller to log
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

## `contract-call preview`

```text
Usage: baw-rs contract-call preview [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --from <FROM>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --from <FROM>
          
      --to <TO>
          
      --json
          
      --value <VALUE>
          
      --inputData <INPUT_DATA>
          
      --timing
          
      --confirm
          
      --unsignedTx <UNSIGNED_TX>
          
      --gasLimit <GAS_LIMIT>
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `contract-call execute`

```text
Usage: baw-rs contract-call execute [OPTIONS] --requestId <REQUEST_ID>

Options:
      --requestId <REQUEST_ID>
          
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

## `contract-call run`

```text
Preview then execute in one process, only if the pinned policy matches

Usage: baw-rs contract-call run [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --from <FROM> --to <TO> --policy <POLICY>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --from <FROM>
          
      --to <TO>
          
      --json
          
      --value <VALUE>
          
      --inputData <INPUT_DATA>
          
      --timing
          
      --confirm
          
      --gasLimit <GAS_LIMIT>
          
      --operation-id <OPERATION_ID>
          
      --policy <POLICY>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `contract-call fire`

```text
Preview then execute in one process with no policy gate and no human digest review. Consent is still `--confirm` + a unique `--operation-id`; the preview's risks and authority changes are returned in the output for the caller to log

Usage: baw-rs contract-call fire [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --from <FROM> --to <TO>

Options:
      --binanceChainId <BINANCE_CHAIN_ID>
          
      --from <FROM>
          
      --to <TO>
          
      --json
          
      --value <VALUE>
          
      --inputData <INPUT_DATA>
          
      --timing
          
      --confirm
          
      --gasLimit <GAS_LIMIT>
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
