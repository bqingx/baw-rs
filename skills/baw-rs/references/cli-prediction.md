# prediction：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `prediction`

```text
Prediction markets: discovery, positions, trading

Usage: baw-rs --wallet-dir <WALLET_DIR> prediction [OPTIONS] <COMMAND>

Commands:
  category  Prediction market categories
  market    Prediction market queries (list, detail, search, order-book, last-trade-price)
  position  Prediction position queries (list, token, settled-history, pnl, portfolio)
  order     Prediction order queries (history)
  trade     Prediction trading commands (quote, place-order, cancel, redeem)
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

## `prediction category`

```text
Prediction market categories

Usage: baw-rs prediction category [OPTIONS] <COMMAND>

Commands:
  list  List prediction market categories (no auth required)
  help  Print this message or the help of the given subcommand(s)

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

## `prediction category list`

```text
List prediction market categories (no auth required)

Usage: baw-rs prediction category list [OPTIONS]

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

Examples:
  baw prediction category list
  baw prediction category list --json
```

## `prediction market`

```text
Prediction market queries (list, detail, search, order-book, last-trade-price)

Usage: baw-rs prediction market [OPTIONS] <COMMAND>

Commands:
  list              List prediction markets
  detail            Get prediction market details
  search            Search prediction markets by keyword
  order-book        Get order book for an outcome
  last-trade-price  Get last trade price for a market
  help              Print this message or the help of the given subcommand(s)

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

## `prediction market list`

```text
List prediction markets

Usage: baw-rs prediction market list [OPTIONS]

Options:
      --l1Category <category>
          L1 category filter (e.g. crypto, sports)
      --l2Category <category>
          L2 subcategory filter
      --sortBy <sort>
          Sort: RECOMMENDED, VOLUME, PARTICIPANTS, CREATED_TIME, END_DATE
      --json
          
      --orderBy <order>
          Order: ASC or DESC (default: DESC)
      --offset <offset>
          Pagination offset (default: 0)
      --timing
          
      --confirm
          
      --limit <limit>
          Page size (default: 20, max: 100)
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help

Examples:
  baw prediction market list
  baw prediction market list --l1Category crypto --limit 5
  baw prediction market list --sortBy VOLUME --orderBy DESC --json
```

## `prediction market detail`

```text
Get prediction market details

Usage: baw-rs prediction market detail [OPTIONS] --marketTopicId <id>

Options:
      --marketTopicId <id>
          Market topic ID
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

Examples:
  baw prediction market detail --marketTopicId 123456
  baw prediction market detail --marketTopicId 123456 --json
```

## `prediction market search`

```text
Search prediction markets by keyword

Usage: baw-rs prediction market search [OPTIONS] --query <query>

Options:
      --query <query>
          Search keyword (max 200 chars)
      --limit <limit>
          Max results (default: 10, max: 50)
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

Examples:
  baw prediction market search --query "Bitcoin"
  baw prediction market search --query "FIFA World Cup" --limit 5 --json
```

## `prediction market order-book`

```text
Get order book for an outcome

Usage: baw-rs prediction market order-book [OPTIONS] --marketId <id> --tokenId <tokenId>

Options:
      --marketId <id>
          Market ID (not marketTopicId, use market detail to find)
      --tokenId <tokenId>
          Outcome token ID
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

Examples:
  baw prediction market order-book --marketId 5237837 --tokenId 123456789 --json

Note: --marketId is the sub-market ID, not the marketTopicId.
Use 'baw prediction market detail --marketTopicId <id>' to find marketId.
```

## `prediction market last-trade-price`

```text
Get last trade price for a market

Usage: baw-rs prediction market last-trade-price [OPTIONS] --marketId <id>

Options:
      --marketId <id>
          Market ID (not marketTopicId, use market detail to find)
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

Examples:
  baw prediction market last-trade-price --marketId 5237837 --json

Note: --marketId is the sub-market ID, not the marketTopicId.
Use 'baw prediction market detail --marketTopicId <id>' to find marketId.
```

## `prediction position`

```text
Prediction position queries (list, token, settled-history, pnl, portfolio)

Usage: baw-rs prediction position [OPTIONS] <COMMAND>

Commands:
  list             List prediction positions with PnL summary
  token            Get position by token ID
  settled-history  List settled position history
  pnl              Query PNL records for prediction positions
  portfolio        Query prediction portfolio summary with active positions and unrealized PNL
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

## `prediction position list`

```text
List prediction positions with PnL summary

Usage: baw-rs prediction position list [OPTIONS]

Options:
      --tab <tab>
          Filter: ONGOING, ENDED, PENDING_CLAIM (default: ONGOING)
      --offset <offset>
          Pagination offset (default: 0)
      --limit <limit>
          Page size (default: 20, max: 100)
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

Examples:
  baw prediction position list
  baw prediction position list --tab PENDING_CLAIM --json
```

## `prediction position token`

```text
Get position by token ID

Usage: baw-rs prediction position token [OPTIONS] --tokenId <tokenId>

Options:
      --tokenId <tokenId>
          ERC-1155 Token ID
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

Examples:
  baw prediction position token --tokenId 123456789
  baw prediction position token --tokenId 123456789 --json
```

## `prediction position settled-history`

```text
List settled position history

Usage: baw-rs prediction position settled-history [OPTIONS]

Options:
      --l1Category <category>
          L1 category filter
      --filter <filter>
          Result filter: all, win, lose (default: all)
      --offset <offset>
          Pagination offset (default: 0)
      --json
          
      --limit <limit>
          Page size (default: 20, max: 100)
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

Examples:
  baw prediction position settled-history
  baw prediction position settled-history --filter win
  baw prediction position settled-history --filter lose --json
```

## `prediction position pnl`

```text
Query PNL records for prediction positions

Usage: baw-rs prediction position pnl [OPTIONS]

Options:
      --tokenId <tokenId>
          Filter by specific token ID
      --l1Category <category>
          L1 category filter: crypto, sports, all
      --offset <offset>
          Pagination offset (default: 0)
      --json
          
      --limit <limit>
          Page size (default: 20, max: 100)
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

Examples:
  baw prediction position pnl
  baw prediction position pnl --l1Category crypto
  baw prediction position pnl --tokenId abc123 --json
```

## `prediction position portfolio`

```text
Query prediction portfolio summary with active positions and unrealized PNL

Usage: baw-rs prediction position portfolio [OPTIONS]

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

Examples:
  baw prediction position portfolio
  baw prediction position portfolio --json
```

## `prediction order`

```text
Prediction order queries (history)

Usage: baw-rs prediction order [OPTIONS] <COMMAND>

Commands:
  history  List prediction order history
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

## `prediction order history`

```text
List prediction order history

Usage: baw-rs prediction order history [OPTIONS]

Options:
      --status <status>
          Filter by status: PENDING, SUBMITTED, FILLED, PARTIALLY_FILLED, CANCELLED, FAILED, EXPIRED
      --l1Category <category>
          L1 category filter
      --orderType <type>
          Filter by type: MARKET, LIMIT
      --json
          
      --offset <offset>
          Pagination offset (default: 0)
      --limit <limit>
          Page size (default: 20, max: 100)
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

Examples:
  baw prediction order history
  baw prediction order history --status FILLED --limit 10
  baw prediction order history --orderType LIMIT --json
```

## `prediction trade`

```text
Prediction trading commands (quote, place-order, cancel, redeem)

Usage: baw-rs prediction trade [OPTIONS] <COMMAND>

Commands:
  quote        Get a prediction trade quote
  place-order  Place a prediction order using a quote ID
  cancel       Cancel prediction orders
  redeem       Redeem winning prediction positions
  help         Print this message or the help of the given subcommand(s)

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

## `prediction trade quote`

```text
Get a prediction trade quote

Usage: baw-rs prediction trade quote [OPTIONS] --binanceChainId <binanceChainId> --tokenId <tokenId> --marketTopicId <marketTopicId> --side <side> --amount <amount> --orderType <type>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID: 56 (BSC), 137 (Polygon)
      --tokenId <tokenId>
          ERC1155 Outcome Token ID
      --marketTopicId <marketTopicId>
          Market topic ID. If --slippageBps is omitted, the market default slippage is fetched via this ID
      --json
          
      --side <side>
          Trade side: BUY or SELL
      --amount <amount>
          Trade amount in USDT (human-readable)
      --timing
          
      --confirm
          
      --orderType <type>
          Order type: MARKET or LIMIT
      --operation-id <OPERATION_ID>
          
      --slippageBps <bps>
          Slippage in basis points (default: use market slippageBps)
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --priceLimit <price>
          Limit price in USDT (required for LIMIT orders)
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help

Examples:
  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 10 --orderType MARKET
  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 5 --orderType LIMIT --priceLimit 0.6 --json
```

## `prediction trade place-order`

```text
Place a prediction order using a quote ID

Usage: baw-rs prediction trade place-order [OPTIONS] --quoteId <quoteId> --slippageBps <bps>

Options:
      --quoteId <quoteId>
          Quote ID from trade quote command
      --orderType <type>
          Order type: MARKET (default) or LIMIT
      --slippageBps <bps>
          Slippage in basis points (from quote response slippageBps field)
      --json
          
      --priceLimit <price>
          Limit price in USDT (required for LIMIT orders)
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

Examples:
  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000
  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000 --orderType LIMIT --priceLimit 0.6 --json
```

## `prediction trade cancel`

```text
Cancel prediction orders

Usage: baw-rs prediction trade cancel [OPTIONS] --orderIds <ids>

Options:
      --orderIds <ids>
          Comma-separated order IDs to cancel
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

Examples:
  baw prediction trade cancel --orderIds order_123,order_456
  baw prediction trade cancel --orderIds order_123 --json
```

## `prediction trade redeem`

```text
Redeem winning prediction positions

Usage: baw-rs prediction trade redeem [OPTIONS] --tokenIds <ids>

Options:
      --tokenIds <ids>
          Comma-separated list of winning token IDs
      --binanceChainId <binanceChainId>
          Binance chain ID filter
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

Examples:
  baw prediction trade redeem --tokenIds token_123,token_456
  baw prediction trade redeem --tokenIds token_123 --binanceChainId 56 --json
```
