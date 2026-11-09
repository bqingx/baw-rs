# market-order：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`（或传统 `--wallet-dir` 组合）；`pipeline` 属于实例，不带钱包；`wallets/openfour` 各有独立参数入口。

## `market-order`

```text
Usage: baw-rs market-order [OPTIONS] <COMMAND>

Commands:
  quote
  swap
  intent  Local intent only: no quote, simulation, session, or execution authorization
  help    Print this message or the help of the given subcommand(s)

Options:
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
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

## `market-order quote`

```text
Usage: baw-rs market-order quote [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --fromToken <FROM_TOKEN> --toToken <TO_TOKEN> --fromTokenQty <FROM_TOKEN_QTY> --slippage <SLIPPAGE>

Options:
      --provider <PROVIDER>
          OKX is the only trade provider. No fallback [default: okx] [possible values: okx]
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --okx-config <OKX_CONFIG>
          Private OKX profile (0600)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --fromTokenDecimals <FROM_TOKEN_DECIMALS>

      --binanceChainId <BINANCE_CHAIN_ID>

      --fromToken <FROM_TOKEN>

      --json

      --toToken <TO_TOKEN>

      --fromTokenQty <FROM_TOKEN_QTY>

      --timing

      --confirm

      --slippage <SLIPPAGE>
          Explicit slippage percentage; auto is not supported. Never increased for token tax
      --max-from-tax-percent <MAX_FROM_TAX_PERCENT>
          Explicit input transfer-tax metadata cap in percent. Zero keeps tax tokens disabled [default: 0]
      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --max-to-tax-percent <MAX_TO_TAX_PERCENT>
          Explicit output transfer-tax metadata cap in percent. Net wallet minimum still applies [default: 0]
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `market-order swap`

```text
Usage: baw-rs market-order swap [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --fromToken <FROM_TOKEN> --toToken <TO_TOKEN> --fromTokenQty <FROM_TOKEN_QTY> --slippage <SLIPPAGE> --mev <MEV>

Options:
      --provider <PROVIDER>
          OKX is the only trade provider. No fallback [default: okx] [possible values: okx]
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --okx-config <OKX_CONFIG>
          Private OKX profile (0600)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --fromTokenDecimals <FROM_TOKEN_DECIMALS>

      --binanceChainId <BINANCE_CHAIN_ID>

      --fromToken <FROM_TOKEN>

      --json

      --toToken <TO_TOKEN>

      --fromTokenQty <FROM_TOKEN_QTY>

      --timing

      --confirm

      --slippage <SLIPPAGE>
          Explicit slippage percentage; auto is not supported. Never increased for token tax
      --max-from-tax-percent <MAX_FROM_TAX_PERCENT>
          Explicit input transfer-tax metadata cap in percent. Zero keeps tax tokens disabled [default: 0]
      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --max-to-tax-percent <MAX_TO_TAX_PERCENT>
          Explicit output transfer-tax metadata cap in percent. Net wallet minimum still applies [default: 0]
      --mev <MEV>
          Must be explicit. Protected submission is not yet supported by the signing backend
      --warm
          Route this command through the resident warm server when it is running
      --gasLevel <GAS_LEVEL>
          [default: MEDIUM]
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
      --acknowledge-token-risk

      --okx-min-output-units <OKX_MIN_OUTPUT_UNITS>
          Optional fixed net-output floor in base units. Omit to use fresh OKX/slippage minima
      --auto-approve

  -h, --help
          Print help
```

## `market-order intent`

```text
Local intent only: no quote, simulation, session, or execution authorization

Usage: baw-rs market-order intent [OPTIONS] --binanceChainId <BINANCE_CHAIN_ID> --fromToken <FROM_TOKEN> --toToken <TO_TOKEN> --fromTokenQty <FROM_TOKEN_QTY> --slippage <SLIPPAGE> --mev <MEV>

Options:
      --provider <PROVIDER>
          OKX is the only trade provider. No fallback [default: okx] [possible values: okx]
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --okx-config <OKX_CONFIG>
          Private OKX profile (0600)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --fromTokenDecimals <FROM_TOKEN_DECIMALS>

      --binanceChainId <BINANCE_CHAIN_ID>

      --fromToken <FROM_TOKEN>

      --json

      --toToken <TO_TOKEN>

      --fromTokenQty <FROM_TOKEN_QTY>

      --timing

      --confirm

      --slippage <SLIPPAGE>
          Explicit slippage percentage; auto is not supported. Never increased for token tax
      --max-from-tax-percent <MAX_FROM_TAX_PERCENT>
          Explicit input transfer-tax metadata cap in percent. Zero keeps tax tokens disabled [default: 0]
      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --max-to-tax-percent <MAX_TO_TAX_PERCENT>
          Explicit output transfer-tax metadata cap in percent. Net wallet minimum still applies [default: 0]
      --mev <MEV>
          Must be explicit. Protected submission is not yet supported by the signing backend
      --warm
          Route this command through the resident warm server when it is running
      --gasLevel <GAS_LEVEL>
          [default: MEDIUM]
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
      --acknowledge-token-risk

      --okx-min-output-units <OKX_MIN_OUTPUT_UNITS>
          Optional fixed net-output floor in base units. Omit to use fresh OKX/slippage minima
      --auto-approve

  -h, --help
          Print help
```
