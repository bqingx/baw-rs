# baw CLI parity map (installed `baw` 1.12.0 → this crate)
Generated from the installed CLI's own `--help` output. Authoritative for syntax: option names, required flags and defaults. Wire endpoints are mined separately from `dist/index.js`.

## Status

```
baw leaves: 102
baw-rs leaves: 26
missing: 79
```

## `approvals detail`

```text
Usage: baw approvals detail [options]

Show approval detail with recent operation records

Options:
  --binanceChainId <binanceChainId>  Binance chain ID
  --tokenContract <address>          Token contract address
  --spender <address>                Spender contract address
  --type <type>                      Approval type (approve | permit2)
  -h, --help                         display help for command
```

## `approvals list`

```text
Usage: baw approvals list [options]

List token approvals for your wallet

Options:
  --spender <address>    Filter by spender address
  --filterTypes <types>  Filter by risk type (high_risk, medium_risk,
                         non_interactive, others)
  --limit <number>       Number of results to return (default: 20)
  --offset <cursor>      Pagination cursor from previous query
  -h, --help             display help for command
```

## `approvals revoke`

```text
Usage: baw approvals revoke [options]

Revoke a token approval

Options:
  --binanceChainId <binanceChainId>  Binance chain ID
  --tokenContract <address>          Token contract address
  --spender <address>                Spender contract address
  --type <type>                      Approval type (approve | permit2)
  -h, --help                         display help for command
```

## `cli-check`

```text
Usage: baw cli-check [options]

Check if the CLI meets the required version

Options:
  --required-version <version>  Required CLI version
  -h, --help                    display help for command
```

## `defi claim`

```text
Usage: baw defi claim [options]

Claim LP fees, protocol/investment rewards, or matured redemptions

Options:
  --claimType <claimType>            Claim type: REWARD_PROTOCOL /
                                     REWARD_INVESTMENT / LP_FEE / REDEMPTION
  --investmentId <investmentId>      Investment product ID
  --defiProtocolId <defiProtocolId>  DeFi protocol ID (required for
                                     REWARD_PROTOCOL)
  --nftId <nftId>                    LP NFT tokenId (required for LP_FEE)
  --redemptionId <redemptionId>      Redemption record ID (required for
                                     REDEMPTION)
  --tokenAddress <tokenAddress>      Optional token address to limit the claim
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

Examples:
  baw defi claim --claimType LP_FEE --investmentId pancake_v3_xxx --nftId 12345
  baw defi claim --claimType REWARD_PROTOCOL --defiProtocolId aave --json
  baw defi claim --claimType REDEMPTION --investmentId lista_yusd --redemptionId redemp_001

```

## `defi deposit`

```text
Usage: baw defi deposit [options]

Deposit / stake / supply assets to a DeFi protocol

Options:
  --investmentId <investmentId>      Investment product ID
  --tokenAddress <tokenAddress>      Underlying token contract address
  --amount <amount>                  Amount to deposit (human-readable, e.g.
                                     "100")
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

Examples:
  baw defi deposit --investmentId venus_usdt --tokenAddress 0x... --amount 100
  baw defi deposit --investmentId venus_usdt --tokenAddress 0x... --amount 100 --gasLevel HIGH --json

```

## `defi investment-info`

```text
Usage: baw defi investment-info [options]

Get detailed information about a DeFi investment

Options:
  --investmentId <investmentId>  Investment ID
  -h, --help                     display help for command

Examples:
  baw defi investment-info --investmentId venus-bsc-usdt-supply --json

```

## `defi investment-list`

```text
Usage: baw defi investment-list [options]

List DeFi investment opportunities

Options:
  --investType <investType>          Investment type: Earn, Loan, LiquidityPool
  --defiProtocolId <defiProtocolId>  Filter by protocol ID
  --contractAddresses <addresses>    Filter by token contract addresses
                                     (comma-separated, max 2)
  --binanceChainId <binanceChainId>  Filter by Binance chain ID
  --sortField <sortField>            Sort by: apy, tvl (default: apy)
  --sortDirection <sortDirection>    Sort direction: ASC, DESC (default: DESC)
  --page <page>                      Page number (default: 1)
  --size <size>                      Page size (default: 20, max: 100)
  -h, --help                         display help for command

Examples:
  baw defi investment-list --investType Earn --json
  baw defi investment-list --investType Earn --defiProtocolId venus --json
  baw defi investment-list --investType LiquidityPool --sortField tvl --json

```

## `defi lp-add`

```text
Usage: baw defi lp-add [options]

Add liquidity to an LP position (create new or top up existing)

Options:
  --investmentId <investmentId>      LP investment product ID
  --tokenAddress <tokenAddress>      Token contract address matching --amount
  --amount <amount>                  Amount to deposit (human-readable)
  --nftId <nftId>                    V3/V4 LP NFT tokenId; use to top up an
                                     existing position
  --tickLower <tickLower>            Lower tick of the price range (raw int24,
                                     must be aligned to pool tickSpacing)
  --tickUpper <tickUpper>            Upper tick of the price range (raw int24,
                                     must be aligned to pool tickSpacing)
  --priceRange <percent>             Price range as a percentage of the current
                                     pool price (e.g. "5" for ±5%, range (0,
                                     50])
  --slippageBps <slippageBps>        Slippage in basis points: "auto" or 1-4999
                                     (default: "auto")
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

LP position source:
  • --nftId <id>                       Top up an existing position; --priceRange and ticks are ignored
  • --priceRange <percent>             New position centered on current price (e.g. 5 = ±5%)
  • --tickLower <int> --tickUpper <int>   New position with explicit ticks
  When --nftId is absent, --priceRange and explicit ticks are mutually exclusive (specify exactly one).

Examples:
  # New position via priceRange (recommended for first-time users)
  baw defi lp-add --investmentId pancake_v3_xxx --tokenAddress 0x... --amount 1 --priceRange 5
  # New position via explicit ticks
  baw defi lp-add --investmentId pancake_v3_xxx --tokenAddress 0x... --amount 1 --tickLower -887220 --tickUpper 887220
  # Top up existing position
  baw defi lp-add --investmentId pancake_v3_xxx --tokenAddress 0x... --amount 1 --nftId 12345 --json

```

## `defi lp-remove`

```text
Usage: baw defi lp-remove [options]

Remove liquidity from an LP position

Options:
  --investmentId <investmentId>      LP investment product ID
  --nftId <nftId>                    LP NFT tokenId
  --ratio <ratio>                    Removal ratio (0, 1]; "1" removes the full
                                     position
  --slippageBps <slippageBps>        Slippage in basis points: "auto" or 1-4999
                                     (default: "auto")
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

Examples:
  baw defi lp-remove --investmentId pancake_v3_xxx --nftId 12345 --ratio 1
  baw defi lp-remove --investmentId pancake_v3_xxx --nftId 12345 --ratio 0.5 --slippageBps 100 --json

```

## `defi position`

```text
Usage: baw defi position [options]

Query your DeFi positions across protocols

Options:
  --address <address>                Query a specific address (defaults to user
                                     wallet address)
  --binanceChainId <binanceChainId>  Filter by Binance chain ID (e.g. 56)
  --defiProtocolId <defiProtocolId>  Filter by DeFi protocol ID (e.g. venus,
                                     pancakeswap3)
  --refresh                          Force-refresh positions (skip cache,
                                     rate-limited)
  -h, --help                         display help for command

Examples:
  baw defi position --json
  baw defi position --address 0x66FAa5Ded68CC83a71BC172Fb03C17a6Bcb32f7a --json
  baw defi position --binanceChainId 56 --json
  baw defi position --defiProtocolId venus --json
  baw defi position --refresh --json

```

## `defi preview`

```text
Usage: baw defi preview [options]

Preview a DeFi transaction (does not broadcast)

Options:
  --action <action>                  Action: deposit / redeem / lp-add /
                                     lp-remove / claim
  --investmentId <investmentId>      Investment product ID
  --tokenAddress <tokenAddress>      Token contract address
  --amount <amount>                  Amount (human-readable)
  --ratio <ratio>                    Ratio (0, 1] (redeem / lp-remove)
  --nftId <nftId>                    LP NFT tokenId
  --tickLower <tickLower>            Lower tick of LP price range (lp-add only)
  --tickUpper <tickUpper>            Upper tick of LP price range (lp-add only)
  --priceRange <percent>             LP price range percentage, e.g. "5" for
                                     ±5% (lp-add only, mutually exclusive with
                                     --nftId / explicit ticks)
  --slippageBps <slippageBps>        Slippage in basis points: "auto" or 1-4999
  --claimType <claimType>            Claim type (claim only)
  --defiProtocolId <defiProtocolId>  DeFi protocol ID (claim REWARD_PROTOCOL)
  --redemptionId <redemptionId>      Redemption record ID (claim REDEMPTION)
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

Examples:
  baw defi preview --action deposit --investmentId X --tokenAddress 0x... --amount 100
  baw defi preview --action lp-remove --investmentId X --nftId 12345 --ratio 1 --json
  baw defi preview --action claim --claimType LP_FEE --investmentId X --nftId 12345

```

## `defi protocol-info`

```text
Usage: baw defi protocol-info [options]

Get detailed information about a DeFi protocol

Options:
  --defiProtocolId <defiProtocolId>  Protocol ID (e.g. venus, aave-v3)
  -h, --help                         display help for command

Examples:
  baw defi protocol-info --defiProtocolId venus --json
  baw defi protocol-info --defiProtocolId aave-v3 --json

```

## `defi protocol-list`

```text
Usage: baw defi protocol-list [options]

List DeFi protocols with TVL and APY

Options:
  --binanceChainId <binanceChainId>  Filter by Binance chain ID (e.g. 56)
  --investType <investType>          Filter by invest type: Earn, Loan,
                                     LiquidityPool
  --sortField <sortField>            Sort by: tvl, apy (default: tvl)
  --sortDirection <sortDirection>    Sort direction: ASC, DESC (default: DESC)
  --page <page>                      Page number (default: 1)
  --size <size>                      Page size (default: 200, max: 200)
  -h, --help                         display help for command

Examples:
  baw defi protocol-list --json
  baw defi protocol-list --investType Earn --json
  baw defi protocol-list --sortField apy --sortDirection DESC --json

```

## `defi redeem`

```text
Usage: baw defi redeem [options]

Redeem / unstake assets from a DeFi protocol

Options:
  --investmentId <investmentId>      Investment product ID
  --tokenAddress <tokenAddress>      Underlying token contract address
  --amount <amount>                  Redeem amount (human-readable); mutually
                                     exclusive with --ratio
  --ratio <ratio>                    Redeem ratio (0, 1]; mutually exclusive
                                     with --amount
  --gasLevel <gasLevel>              Gas level: LOW / MEDIUM / HIGH (default:
                                     "MEDIUM")
  --binanceChainId <binanceChainId>  Binance chain ID (default: 56) (default:
                                     "56")
  -h, --help                         display help for command

Examples:
  baw defi redeem --investmentId venus_usdt --tokenAddress 0x... --amount 50
  baw defi redeem --investmentId venus_usdt --tokenAddress 0x... --ratio 1 --json

```

## `leaderboard alpha-radar`

```text
Usage: baw leaderboard alpha-radar [options]

Alpha radar query (find addresses holding target tokens)

Options:
  -c, --chain-id <chainId>   Chain ID
  -t, --tokens <tokens>      Comma-separated token addresses
  -m, --match-count <count>  Min number of tokens to match (>=1)
  -p, --period <period>      Period: 7d/30d/90d (default: "30d")
  --page <page>              Page number (from 0) (default: "0")
  --size <size>              Page size (max 20) (default: "20")
  -h, --help                 display help for command
```

## `leaderboard alpha-radar-config list`

```text
Usage: baw leaderboard alpha-radar-config list [options]

List alpha radar configs

Options:
  -c, --chain-id <chainId>  Chain ID
  -h, --help                display help for command
```

## `leaderboard alpha-radar-config save`

```text
Usage: baw leaderboard alpha-radar-config save [options]

Save alpha radar configs (--config is a JSON array of AlphaRadarConfigItem)

Options:
  -c, --chain-id <chainId>  Chain ID
  --config <config>         JSON array of AlphaRadarConfigItem
  -h, --help                display help for command
```

## `leaderboard analyze`

```text
Usage: baw leaderboard analyze [options]

Analyze a single address (reverse-lookup within top N)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address to analyze
  -p, --period <period>     Period: 7d/30d/90d (default: "30d")
  --top-n <n>               Scan top N entries (default 1000, max 5000)
                            (default: "1000")
  -h, --help                display help for command
```

## `leaderboard preset list`

```text
Usage: baw leaderboard preset list [options]

List saved presets

Options:
  -h, --help  display help for command
```

## `leaderboard preset save`

```text
Usage: baw leaderboard preset save [options]

Save presets (replaces all; --config is a JSON array of PresetItem)

Options:
  --config <json>  JSON array of preset items
  -h, --help       display help for command
```

## `leaderboard query`

```text
Usage: baw leaderboard query [options]

Query leaderboard (top traders)

Options:
  -c, --chain-id <chainId>  Chain ID
  -p, --period <period>     Period: 7d/30d/90d (default: "30d")
  -t, --tag <tag>           Tag: ALL/KOL/MPC (default: "ALL")
  --sort-by <n>             Sort field (0=PnL 20=win 30=vol 50=tx 60=active
                            70=rate 80=tokens) (default: "0")
  --order-by <n>            Order (0/2=desc 1=asc) (default: "0")
  --page <page>             Page number (from 0) (default: "0")
  --size <size>             Page size (max 20) (default: "20")
  -h, --help                display help for command
```

## `limit-order buy`

```text
Usage: baw limit-order buy [options]

Create a limit buy order (buy token at trigger price)

Options:
  --triggerPrice <triggerPrice>      Trigger price in USD (e.g., $100 or 100)
  --fromTokenQty <fromTokenQty>      Amount to spend
  --fromToken <fromToken>            Source token address
  --toToken <toToken>                Token to buy (address)
  --slippage <slippage>              Slippage tolerance: "auto" (default) or
                                     0-100
  --gasLevel <gasLevel>              Gas level: LOW, MEDIUM (default), HIGH
                                     (default: "MEDIUM")
  --mev <mev>                        MEV protection: true (default) or false
                                     (default: "true")
  --binanceChainId <binanceChainId>  Binance chain ID
  -h, --help                         display help for command

Examples:
  baw limit-order buy --binanceChainId 56 --triggerPrice $100 --fromTokenQty 10.1 --fromToken 0x55d3...7955 --toToken 0x...
  baw limit-order buy --binanceChainId 56 --triggerPrice $100 --fromTokenQty 0.4 --fromToken 0xeeee...eeee --toToken 0x...


```

## `limit-order cancel`

```text
Usage: baw limit-order cancel [options]

Cancel a pending limit order by its strategy ID. Cannot cancel executed orders

Options:
  --strategyId <strategyId>  Strategy ID of the limit order to cancel
  -h, --help                 display help for command

Examples:
  baw limit-order cancel --strategyId 9876543210
  baw limit-order cancel --strategyId 9876543210 --json


```

## `limit-order list`

```text
Usage: baw limit-order list [options]

List and query limit orders with filters

Options:
  --strategyId <id>                  Query specific limit order by strategy ID
                                     (ignores other filters)
  --binanceChainId <binanceChainId>  Filter by binanceChainId
  --status <status>                  Filter by order status: PENDING, WORKING,
                                     TRIGGERED, FINISHED, FAILED, EXPIRED, or
                                     CANCELED
  --fromToken <address>              Filter by source token contract address
  --toToken <address>                Filter by target token contract address
  --startTime <timestamp>            Filter orders created after this time
                                     (milliseconds timestamp)
  --endTime <timestamp>              Filter orders created before this time
                                     (milliseconds timestamp)
  --page <page>                      Page number for pagination (default: 1)
  --pageSize <size>                  Number of orders per page (default: 20,
                                     max: 100)
  -h, --help                         display help for command

Examples:
  baw limit-order list
  baw limit-order list --strategyId 9876543210
  baw limit-order list --status WORKING --page 1 --pageSize 20
  baw limit-order list --json


```

## `limit-order sell`

```text
Usage: baw limit-order sell [options]

Create a limit sell order (sell token at trigger price)

Options:
  --triggerPrice <triggerPrice>      Trigger price in USD (e.g., $100 or 100)
  --fromTokenQty <fromTokenQty>      Amount of tokens to sell
  --fromToken <fromToken>            Token to sell (address)
  --toToken <toToken>                Destination token address
  --slippage <slippage>              Slippage tolerance: "auto" (default) or
                                     0-100
  --gasLevel <gasLevel>              Gas level: LOW, MEDIUM (default), HIGH
                                     (default: "MEDIUM")
  --mev <mev>                        MEV protection: true (default) or false
                                     (default: "true")
  --binanceChainId <binanceChainId>  Binance chain ID
  -h, --help                         display help for command

Examples:
  baw limit-order sell --binanceChainId 56 --triggerPrice $100 --fromTokenQty 10.1 --fromToken 0x... --toToken 0x55d3...7955
  baw limit-order sell --binanceChainId 56 --triggerPrice $100 --fromTokenQty 10.1 --fromToken 0x... --toToken 0xeeee...eeee


```

## `prediction category list`

```text
Usage: baw prediction category list [options]

List prediction market categories (no auth required)

Options:
  -h, --help  display help for command

Examples:
  baw prediction category list
  baw prediction category list --json

```

## `prediction market detail`

```text
Usage: baw prediction market detail [options]

Get prediction market details

Options:
  --marketTopicId <id>  Market topic ID
  -h, --help            display help for command

Examples:
  baw prediction market detail --marketTopicId 123456
  baw prediction market detail --marketTopicId 123456 --json

```

## `prediction market last-trade-price`

```text
Usage: baw prediction market last-trade-price [options]

Get last trade price for a market

Options:
  --marketId <id>  Market ID (not marketTopicId, use market detail to find)
  -h, --help       display help for command

Examples:
  baw prediction market last-trade-price --marketId 5237837 --json

Note: --marketId is the sub-market ID, not the marketTopicId.
Use 'baw prediction market detail --marketTopicId <id>' to find marketId.

```

## `prediction market list`

```text
Usage: baw prediction market list [options]

List prediction markets

Options:
  --l1Category <category>  L1 category filter (e.g. crypto, sports)
  --l2Category <category>  L2 subcategory filter
  --sortBy <sort>          Sort: RECOMMENDED, VOLUME, PARTICIPANTS,
                           CREATED_TIME, END_DATE
  --orderBy <order>        Order: ASC or DESC (default: DESC) (default: "DESC")
  --offset <offset>        Pagination offset (default: 0)
  --limit <limit>          Page size (default: 20, max: 100)
  -h, --help               display help for command

Examples:
  baw prediction market list
  baw prediction market list --l1Category crypto --limit 5
  baw prediction market list --sortBy VOLUME --orderBy DESC --json

```

## `prediction market order-book`

```text
Usage: baw prediction market order-book [options]

Get order book for an outcome

Options:
  --marketId <id>      Market ID (not marketTopicId, use market detail to find)
  --tokenId <tokenId>  Outcome token ID
  -h, --help           display help for command

Examples:
  baw prediction market order-book --marketId 5237837 --tokenId 123456789 --json

Note: --marketId is the sub-market ID, not the marketTopicId.
Use 'baw prediction market detail --marketTopicId <id>' to find marketId.

```

## `prediction market search`

```text
Usage: baw prediction market search [options]

Search prediction markets by keyword

Options:
  --query <query>  Search keyword (max 200 chars)
  --limit <limit>  Max results (default: 10, max: 50)
  -h, --help       display help for command

Examples:
  baw prediction market search --query "Bitcoin"
  baw prediction market search --query "FIFA World Cup" --limit 5 --json

```

## `prediction order history`

```text
Usage: baw prediction order history [options]

List prediction order history

Options:
  --status <status>        Filter by status: PENDING, SUBMITTED, FILLED,
                           PARTIALLY_FILLED, CANCELLED, FAILED, EXPIRED
  --l1Category <category>  L1 category filter
  --orderType <type>       Filter by type: MARKET, LIMIT
  --offset <offset>        Pagination offset (default: 0)
  --limit <limit>          Page size (default: 20, max: 100)
  -h, --help               display help for command

Examples:
  baw prediction order history
  baw prediction order history --status FILLED --limit 10
  baw prediction order history --orderType LIMIT --json

```

## `prediction position list`

```text
Usage: baw prediction position list [options]

List prediction positions with PnL summary

Options:
  --tab <tab>        Filter: ONGOING, ENDED, PENDING_CLAIM (default: ONGOING)
  --offset <offset>  Pagination offset (default: 0)
  --limit <limit>    Page size (default: 20, max: 100)
  -h, --help         display help for command

Examples:
  baw prediction position list
  baw prediction position list --tab PENDING_CLAIM --json

```

## `prediction position pnl`

```text
Usage: baw prediction position pnl [options]

Query PNL records for prediction positions

Options:
  --tokenId <tokenId>      Filter by specific token ID
  --l1Category <category>  L1 category filter: crypto, sports, all
  --offset <offset>        Pagination offset (default: 0)
  --limit <limit>          Page size (default: 20, max: 100)
  -h, --help               display help for command

Examples:
  baw prediction position pnl
  baw prediction position pnl --l1Category crypto
  baw prediction position pnl --tokenId abc123 --json

```

## `prediction position portfolio`

```text
Usage: baw prediction position portfolio [options]

Query prediction portfolio summary with active positions and unrealized PNL

Options:
  -h, --help  display help for command

Examples:
  baw prediction position portfolio
  baw prediction position portfolio --json

```

## `prediction position settled-history`

```text
Usage: baw prediction position settled-history [options]

List settled position history

Options:
  --l1Category <category>  L1 category filter
  --filter <filter>        Result filter: all, win, lose (default: all)
  --offset <offset>        Pagination offset (default: 0)
  --limit <limit>          Page size (default: 20, max: 100)
  -h, --help               display help for command

Examples:
  baw prediction position settled-history
  baw prediction position settled-history --filter win
  baw prediction position settled-history --filter lose --json

```

## `prediction position token`

```text
Usage: baw prediction position token [options]

Get position by token ID

Options:
  --tokenId <tokenId>  ERC-1155 Token ID
  -h, --help           display help for command

Examples:
  baw prediction position token --tokenId 123456789
  baw prediction position token --tokenId 123456789 --json

```

## `prediction trade cancel`

```text
Usage: baw prediction trade cancel [options]

Cancel prediction orders

Options:
  --orderIds <ids>  Comma-separated order IDs to cancel
  -h, --help        display help for command

Examples:
  baw prediction trade cancel --orderIds order_123,order_456
  baw prediction trade cancel --orderIds order_123 --json

```

## `prediction trade place-order`

```text
Usage: baw prediction trade place-order [options]

Place a prediction order using a quote ID

Options:
  --quoteId <quoteId>   Quote ID from trade quote command
  --orderType <type>    Order type: MARKET (default) or LIMIT
  --slippageBps <bps>   Slippage in basis points (from quote response
                        slippageBps field)
  --priceLimit <price>  Limit price in USDT (required for LIMIT orders)
  -h, --help            display help for command

Examples:
  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000
  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000 --orderType LIMIT --priceLimit 0.6 --json

```

## `prediction trade quote`

```text
Usage: baw prediction trade quote [options]

Get a prediction trade quote

Options:
  --binanceChainId <binanceChainId>  Binance chain ID: 56 (BSC), 137 (Polygon)
  --tokenId <tokenId>                ERC1155 Outcome Token ID
  --marketTopicId <marketTopicId>    Market topic ID. If --slippageBps is
                                     omitted, the market default slippage is
                                     fetched via this ID
  --side <side>                      Trade side: BUY or SELL
  --amount <amount>                  Trade amount in USDT (human-readable)
  --orderType <type>                 Order type: MARKET or LIMIT
  --slippageBps <bps>                Slippage in basis points (default: use
                                     market slippageBps)
  --priceLimit <price>               Limit price in USDT (required for LIMIT
                                     orders)
  -h, --help                         display help for command

Examples:
  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 10 --orderType MARKET
  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 5 --orderType LIMIT --priceLimit 0.6 --json

```

## `prediction trade redeem`

```text
Usage: baw prediction trade redeem [options]

Redeem winning prediction positions

Options:
  --tokenIds <ids>                   Comma-separated list of winning token IDs
  --binanceChainId <binanceChainId>  Binance chain ID filter
  -h, --help                         display help for command

Examples:
  baw prediction trade redeem --tokenIds token_123,token_456
  baw prediction trade redeem --tokenIds token_123 --binanceChainId 56 --json

```

## `signal backtest detail`

```text
Usage: baw signal backtest detail [options]

Show backtest detail (task info + token list)

Options:
  -c, --chain-id <chainId>    Chain ID
  --strategy-id <strategyId>  Strategy ID
  -h, --help                  display help for command
```

## `signal backtest list`

```text
Usage: baw signal backtest list [options]

List backtest task stats

Options:
  -c, --chain-id <chainId>  Chain ID
  -p, --page <page>         Page number (ignored when --all is used) (default:
                            "1")
  -s, --size <size>         Page size (default: "20")
  --backtest-days <days>    Backtest period in days
  --all                     Fetch all pages automatically (overrides
                            --page/--size)
  -h, --help                display help for command
```

## `signal backtest retry`

```text
Usage: baw signal backtest retry [options]

Retry a failed backtest job (meme-rush only)

Options:
  -c, --chain-id <chainId>  Chain ID
  -t, --type <type>         Strategy type (default: meme-rush) (default:
                            "meme-rush")
  --job-id <jobId>          Job ID
  -h, --help                display help for command
```

## `signal backtest schedule`

```text
Usage: baw signal backtest schedule [options]

Configure or query backtest schedule

Options:
  -c, --chain-id <chainId>  Chain ID
  --job-id <jobId>          Job ID
  --interval <interval>     Schedule interval: 4H | 6H | 12H | 24H | OFF (omit
                            to query only)
  -h, --help                display help for command
```

## `signal credits`

```text
Usage: baw signal credits [options]

Query backtest credits info

Options:
  -h, --help  display help for command
```

## `signal explore`

```text
Usage: baw signal explore [options]

Explore official signal strategies

Options:
  -c, --chain-id <chainId>  Chain ID
  --backtest-days <days>    Backtest period in days
  -h, --help                display help for command
```

## `signal list`

```text
Usage: baw signal list [options]

Query custom signal feed (all sources by default)

Options:
  -c, --chain-id <chainId>  Chain ID (default: "56")
  -n, --page-size <size>    Number of signals per source (default: "100")
  -s, --source <source>     Signal source: all | user | meme | smart-money
                            (default: "all")
  --strategy-id <id>        Filter by strategy ID (USER_STRATEGY only)
  --strategy-type <type>    Filter by strategy type: meme-rush | fomo-call
                            (USER_STRATEGY only; aliases: meme, fomo)
  --sort-by <field>         Sort: time | maxGain (default: "time")
  --time-range <range>      Time filter: 5m | 1h | 24h
  -h, --help                display help for command
```

## `signal strategy create`

```text
Usage: baw signal strategy create [options]

Create a new signal strategy (estimate → confirm → create)

Options:
  -c, --chain-id <chainId>     Chain ID
  -t, --type <type>            Strategy type: meme-rush | fomo-call (aliases:
                               meme, fomo)
  -n, --name <name>            Strategy name (max 20 chars)
  --config <config>            Strategy config (JSON string)
  --wallet-group-id <groupId>  Wallet group ID (required for fomo-call, merged
                               into config.selectedGroups)
  --run-backtest               Trigger backtest on creation (sets
                               config.backtest.enabled=true)
  -h, --help                   display help for command
```

## `signal strategy delete`

```text
Usage: baw signal strategy delete [options]

Delete a strategy (backtest task)

Options:
  -c, --chain-id <chainId>  Chain ID
  -t, --type <type>         Strategy type: meme-rush | fomo-call (aliases:
                            meme, fomo)
  --job-id <jobId>          Job ID
  -y, --yes                 Skip confirmation prompt
  -h, --help                display help for command
```

## `signal strategy follow`

```text
Usage: baw signal strategy follow [options]

Follow a strategy. Hall strategies are copied to your own account first, then
followed.

Options:
  -c, --chain-id <chainId>  Chain ID
  -t, --type <type>         Strategy type: meme-rush | fomo-call (aliases:
                            meme, fomo)
  --job-id <jobId>          Job ID
  --task-id <taskId>        Task ID (used to look up the strategy; hall
                            strategies default to 1) (default: "1")
  -n, --name <name>         Name for the copied strategy (only used when
                            copying a hall strategy; max 20 chars)
  -y, --yes                 Skip the copy confirmation prompt for hall
                            strategies
  -h, --help                display help for command
```

## `signal strategy list`

```text
Usage: baw signal strategy list [options]

List all your strategies (owned), optionally filtered by --followed or --type

Options:
  -c, --chain-id <chainId>  Chain ID
  --followed                Only show followed strategies
  --type <strategyType>     Filter by strategy type: meme-rush | fomo-call
  -h, --help                display help for command
```

## `signal strategy list-followed`

```text
Usage: baw signal strategy list-followed [options]

List strategies you follow

Options:
  -c, --chain-id <chainId>  Chain ID
  -h, --help                display help for command
```

## `signal strategy unfollow`

```text
Usage: baw signal strategy unfollow [options]

Unfollow a strategy

Options:
  -c, --chain-id <chainId>    Chain ID
  -t, --type <type>           Strategy type: meme-rush | fomo-call (aliases:
                              meme, fomo)
  --strategy-id <strategyId>  Strategy ID
  -y, --yes                   Skip confirmation prompt
  -h, --help                  display help for command
```

## `signal strategy update`

```text
Usage: baw signal strategy update [options]

Update an existing strategy name or config

Options:
  -c, --chain-id <chainId>  Chain ID
  -t, --type <type>         Strategy type: meme-rush | fomo-call (aliases:
                            meme, fomo)
  --job-id <jobId>          Job ID
  -n, --name <name>         New strategy name
  --config <config>         New strategy config (JSON string)
  -y, --yes                 Skip confirmation prompt
  -h, --help                display help for command
```

## `signal wallet-group`

```text
Usage: baw signal wallet-group [options]

List wallet groups (for fomo-call --wallet-group-id)

Options:
  -c, --chain-id <chainId>  Chain ID
  -h, --help                display help for command
```

## `skill-check`

```text
Usage: baw skill-check [options]

Check if a skill has a newer version available

Options:
  --skill-name <skillName>     Skill name to check
  --current-version <version>  Current version to compare against
  -h, --help                   display help for command
```

## `tracker address add`

```text
Usage: baw tracker address add [options]

Add a single address (wraps import)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address
  -l, --label <label>       Label
  -g, --group-id <groupId>  Group ID (default group if omitted)
  --no-overwrite            Do not overwrite existing label/emoji
  -h, --help                display help for command
```

## `tracker address batch`

```text
Usage: baw tracker address batch [options]

Batch import addresses into a group

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --addresses <list>    Comma-separated addresses, or JSON array of
                            {address,label}
  -g, --group-id <groupId>  Group ID (default group if omitted)
  --no-overwrite            Do not overwrite existing label/emoji
  -h, --help                display help for command
```

## `tracker address delete`

```text
Usage: baw tracker address delete [options]

Delete address(es) from a group (supports batch)

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID
  -a, --addresses <list>    Comma-separated addresses to delete
  -y, --yes                 Skip confirmation prompt
  -h, --help                display help for command
```

## `tracker address follow`

```text
Usage: baw tracker address follow [options]

Follow an address (G9-A)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address
  -g, --group-id <groupId>  Group ID
  -l, --label <label>       Label
  -h, --help                display help for command
```

## `tracker address link`

```text
Usage: baw tracker address link [options]

Link an address to a group (address/group/link)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address
  -g, --group-id <groupId>  Target group ID (default group if omitted)
  -y, --yes                 Skip confirmation prompt
  -h, --help                display help for command
```

## `tracker address list`

```text
Usage: baw tracker address list [options]

Paginated address list (address/page)

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID
  --page <page>             Page number (from 1) (default: "1")
  --size <size>             Page size (max 100) (default: "20")
  -h, --help                display help for command
```

## `tracker address search`

```text
Usage: baw tracker address search [options]

Fuzzy search addresses within a group

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID
  -a, --address <address>   Address filter (fuzzy)
  -l, --label <label>       Label filter (fuzzy)
  -h, --help                display help for command
```

## `tracker address unfollow`

```text
Usage: baw tracker address unfollow [options]

Unfollow an address (G9-A)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address
  -h, --help                display help for command
```

## `tracker address update`

```text
Usage: baw tracker address update [options]

Update address label/note (merges address/update + address/label/update)

Options:
  -c, --chain-id <chainId>  Chain ID
  -a, --address <address>   Address
  -l, --label <label>       New label/note
  -h, --help                display help for command
```

## `tracker follow`

```text
Usage: baw tracker follow [options]

List addresses the current user follows (read-only)

Options:
  -c, --chain-id <chainId>  Chain ID
  -h, --help                display help for command
```

## `tracker group create`

```text
Usage: baw tracker group create [options]

Create a new group

Options:
  -c, --chain-id <chainId>  Chain ID
  -n, --name <name>         Group name
  -h, --help                display help for command
```

## `tracker group list`

```text
Usage: baw tracker group list [options]

List address groups

Options:
  -c, --chain-id <chainId>  Chain ID
  --no-all-group            Exclude the "All" virtual group
  -h, --help                display help for command
```

## `tracker group update`

```text
Usage: baw tracker group update [options]

Rename a group

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID
  -n, --name <name>         New group name
  -h, --help                display help for command
```

## `tracker token`

```text
Usage: baw tracker token [options]

Token-dimension monitor (agent own group / public SMY-KOL)

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID (agent own group)
  --tag-type <type>         Public mode tag: kol / smy (omit for agent
                            own-group mode)
  --token-size <n>          Number of recent tokens (default 70)
  --period <period>         Period: 1m/5m/1h/4h/24h (default 24h)
  --filter-risk             Filter risk tokens (default: false)
  -h, --help                display help for command
```

## `tracker tx`

```text
Usage: baw tracker tx [options]

Transaction-dimension monitor (agent own group / public SMY-KOL)

Options:
  -c, --chain-id <chainId>  Chain ID
  -g, --group-id <groupId>  Group ID (agent own group)
  --tag-type <type>         Public mode tag: kol / smy (omit for agent
                            own-group mode)
  --trade-side <sides>      Comma-separated: 19=buy 11=first-buy 29=sell
                            21=clear
  --min-value <n>           Min tx USD value
  --max-value <n>           Max tx USD value
  --filter-risk             Filter risk tokens (default: false)
  -h, --help                display help for command
```

## `tracker ws`

```text
Usage: baw tracker ws [options]

Subscribe to WSP push events via WebSocket

Options:
  --smy                     Listen to Smart Money events
  --kol                     Listen to KOL events (requires -c)
  --wallet <chains>         Listen to chain-level events (comma-separated:
                            BSC,SOL,BASE,ETH)
  --following               Listen to current user followings (requires login +
                            -c, mutually exclusive with others)
  --address <addr>          Listen to specific address events (requires -c)
  --address-list <file>     Batch address file, one per line (requires -c, max
                            100)
  -c, --chain-id <chainId>  Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
  --duration <sec>          Auto-disconnect after N seconds (0 or omit = no
                            limit)
  -h, --help                display help for command
```

## `wallet cancel`

```text
Usage: baw wallet cancel [options] <txHash>

Cancel a pending transaction by sending a replacement tx with higher gas

Options:
  --binanceChainId <binanceChainId>  Binance chain ID (optional, auto-detected
                                     from txHash)
  -h, --help                         display help for command

Examples:
  baw wallet cancel 0xabc123...def456
  baw wallet cancel 0xabc123...def456 --binanceChainId 56
  baw wallet cancel 0xabc123...def456 --json


```

## `wallet resolve-domain`

```text
Usage: baw wallet resolve-domain [options]

Resolve a domain name to an address (supported suffixes/chains are determined
by the backend)

Options:
  --name <name>                      Domain name to resolve, e.g. alice.eth /
                                     alice.bnb
  --binanceChainId <binanceChainId>  Binance chain ID, e.g. 56 (BSC)
  -h, --help                         display help for command

Examples:
  baw wallet resolve-domain --name alice.eth --binanceChainId 56
  baw wallet resolve-domain --name alice.bnb --binanceChainId 56 --json

Note:
  The resolved address must already be in your Binance App address book
  before it can be used as a recipient for "baw wallet send". If it isn't,
  add it in Binance App -> Wallet -> Settings -> Address Book first.


```

## `wallet speed-up`

```text
Usage: baw wallet speed-up [options] <txHash>

Speed up a pending transaction by resubmitting with higher gas

Options:
  --level <level>                    Gas bump level: LOW (×1.1), HIGH (×1.25)
                                     (default: "HIGH")
  --binanceChainId <binanceChainId>  Binance chain ID (optional, auto-detected
                                     from txHash)
  -h, --help                         display help for command

Examples:
  baw wallet speed-up 0xabc123...def456
  baw wallet speed-up 0xabc123...def456 --level LOW
  baw wallet speed-up 0xabc123...def456 --level HIGH --binanceChainId 56
  baw wallet speed-up 0xabc123...def456 --json


```

## `x402-payment preview`

```text
Usage: baw x402-payment preview [options]

Preview x402 payment options from a PaymentRequired response

Options:
  --paymentRequirements <value>  The PaymentRequired payload — base64
                                 `PAYMENT-REQUIRED` header value or raw JSON
                                 (auto-detected)
  -h, --help                     display help for command

Examples:
  # raw JSON (easiest when scripting from the 402 response body)
  baw x402-payment preview --paymentRequirements '{"x402Version":2,"accepts":[...]}' --json

  # base64 (avoids shell quoting issues with large or embedded payloads)
  baw x402-payment preview --paymentRequirements "$(cat 402.json | base64)" --json

```

## `x402-payment sign`

```text
Usage: baw x402-payment sign [options]

Sign a selected x402 payment option and return the replay header value

Options:
  --paymentId <paymentId>  Payment id returned by x402-payment preview
  --selectedIndex <index>  Selected payment option index returned by
                           x402-payment preview
  -h, --help               display help for command

Examples:
  baw x402-payment sign --paymentId 550e8400-e29b-41d4-a716-446655440000 --selectedIndex 1 --json

```

