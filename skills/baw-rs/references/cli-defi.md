# defi：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `defi`

```text
DeFi: protocols, opportunities, positions, deposit/redeem/LP/claim

Usage: baw-rs --wallet-dir <WALLET_DIR> defi [OPTIONS] <COMMAND>

Commands:
  protocol-list    List DeFi protocols with TVL and APY
  protocol-info    Get detailed information about a DeFi protocol
  investment-list  List DeFi investment opportunities
  investment-info  Get detailed information about a DeFi investment
  position         Query your DeFi positions across protocols
  deposit          Deposit / stake / supply assets to a DeFi protocol
  redeem           Redeem / unstake assets from a DeFi protocol
  lp-add           Add liquidity to an LP position (create new or top up existing)
  lp-remove        Remove liquidity from an LP position
  claim            Claim LP fees, protocol/investment rewards, or matured redemptions
  preview          Preview a DeFi transaction (does not broadcast)
  help             Print this message or the help of the given subcommand(s)

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

## `defi protocol-list`

```text
List DeFi protocols with TVL and APY

Usage: baw-rs defi protocol-list [OPTIONS]

Options:
      --binanceChainId <binanceChainId>
          
      --investType <investType>
          
      --sortField <sortField>
          
      --json
          
      --sortDirection <sortDirection>
          
      --page <page>
          
      --timing
          
      --confirm
          
      --size <size>
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `defi protocol-info`

```text
Get detailed information about a DeFi protocol

Usage: baw-rs defi protocol-info [OPTIONS] --defiProtocolId <defiProtocolId>

Options:
      --defiProtocolId <defiProtocolId>
          
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

## `defi investment-list`

```text
List DeFi investment opportunities

Usage: baw-rs defi investment-list [OPTIONS] --investType <investType>

Options:
      --investType <investType>
          
      --defiProtocolId <defiProtocolId>
          
      --contractAddresses <addresses>
          
      --binanceChainId <binanceChainId>
          
      --json
          
      --sortField <sortField>
          
      --timing
          
      --confirm
          
      --sortDirection <sortDirection>
          
      --operation-id <OPERATION_ID>
          
      --page <page>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --size <size>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `defi investment-info`

```text
Get detailed information about a DeFi investment

Usage: baw-rs defi investment-info [OPTIONS] --investmentId <investmentId>

Options:
      --investmentId <investmentId>
          
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

## `defi position`

```text
Query your DeFi positions across protocols

Usage: baw-rs defi position [OPTIONS]

Options:
      --address <address>
          
      --binanceChainId <binanceChainId>
          
      --defiProtocolId <defiProtocolId>
          
      --json
          
      --refresh
          Force-refresh positions (skip cache, rate-limited)
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

## `defi deposit`

```text
Deposit / stake / supply assets to a DeFi protocol

Usage: baw-rs defi deposit [OPTIONS] --investmentId <investmentId> --tokenAddress <tokenAddress> --amount <amount>

Options:
      --investmentId <investmentId>
          
      --tokenAddress <tokenAddress>
          
      --amount <amount>
          
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --json
          
      --binanceChainId <binanceChainId>
          [default: 56]
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

## `defi redeem`

```text
Redeem / unstake assets from a DeFi protocol

Usage: baw-rs defi redeem [OPTIONS] --investmentId <investmentId> --tokenAddress <tokenAddress>

Options:
      --investmentId <investmentId>
          
      --tokenAddress <tokenAddress>
          
      --amount <amount>
          Redeem amount (human-readable); mutually exclusive with --ratio
      --json
          
      --ratio <ratio>
          Redeem ratio (0, 1]; mutually exclusive with --amount
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --timing
          
      --binanceChainId <binanceChainId>
          [default: 56]
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

## `defi lp-add`

```text
Add liquidity to an LP position (create new or top up existing)

Usage: baw-rs defi lp-add [OPTIONS] --investmentId <investmentId> --tokenAddress <tokenAddress> --amount <amount>

Options:
      --investmentId <investmentId>
          
      --tokenAddress <tokenAddress>
          
      --amount <amount>
          
      --json
          
      --nftId <nftId>
          V3/V4 LP NFT tokenId; use to top up an existing position
      --tickLower <tickLower>
          Lower tick of the price range (raw int24, aligned to pool tickSpacing)
      --timing
          
      --confirm
          
      --tickUpper <tickUpper>
          Upper tick of the price range (raw int24, aligned to pool tickSpacing)
      --operation-id <OPERATION_ID>
          
      --priceRange <percent>
          Price range as a percentage of the current pool price (e.g. "5" for ±5%)
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --slippageBps <slippageBps>
          [default: auto]
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --warm
          Route this command through the resident warm server when it is running
      --binanceChainId <binanceChainId>
          [default: 56]
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `defi lp-remove`

```text
Remove liquidity from an LP position

Usage: baw-rs defi lp-remove [OPTIONS] --investmentId <investmentId> --nftId <nftId> --ratio <ratio>

Options:
      --investmentId <investmentId>
          
      --nftId <nftId>
          
      --ratio <ratio>
          Removal ratio (0, 1]; "1" removes the full position
      --json
          
      --slippageBps <slippageBps>
          [default: auto]
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --timing
          
      --binanceChainId <binanceChainId>
          [default: 56]
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

## `defi claim`

```text
Claim LP fees, protocol/investment rewards, or matured redemptions

Usage: baw-rs defi claim [OPTIONS] --claimType <claimType>

Options:
      --claimType <claimType>
          Claim type: REWARD_PROTOCOL / REWARD_INVESTMENT / LP_FEE / REDEMPTION
      --investmentId <investmentId>
          
      --defiProtocolId <defiProtocolId>
          DeFi protocol ID (required for REWARD_PROTOCOL)
      --json
          
      --nftId <nftId>
          LP NFT tokenId (required for LP_FEE)
      --redemptionId <redemptionId>
          Redemption record ID (required for REDEMPTION)
      --timing
          
      --confirm
          
      --tokenAddress <tokenAddress>
          Optional token address to limit the claim
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --binanceChainId <binanceChainId>
          [default: 56]
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `defi preview`

```text
Preview a DeFi transaction (does not broadcast)

Usage: baw-rs defi preview [OPTIONS] --action <action>

Options:
      --action <action>
          Action: deposit / redeem / lp-add / lp-remove / claim
      --investmentId <investmentId>
          
      --tokenAddress <tokenAddress>
          
      --amount <amount>
          
      --json
          
      --ratio <ratio>
          Ratio (0, 1] (redeem / lp-remove)
      --timing
          
      --confirm
          
      --nftId <nftId>
          
      --operation-id <OPERATION_ID>
          
      --tickLower <tickLower>
          Lower tick of LP price range (lp-add only)
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --tickUpper <tickUpper>
          Upper tick of LP price range (lp-add only)
      --priceRange <percent>
          LP price range percentage, e.g. "5" for ±5%
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
      --slippageBps <slippageBps>
          Slippage in basis points: "auto" or 1-4999
      --claimType <claimType>
          Claim type (claim only)
      --defiProtocolId <defiProtocolId>
          DeFi protocol ID (claim REWARD_PROTOCOL)
      --redemptionId <redemptionId>
          Redemption record ID (claim REDEMPTION)
      --gasLevel <gasLevel>
          [default: MEDIUM]
      --binanceChainId <binanceChainId>
          [default: 56]
  -h, --help
          Print help
```
