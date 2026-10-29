//! Prediction markets: discovery, positions, trading. Wire protocol mirrored from `baw`.
//!
//! Every endpoint below is quoted from the installed bundle
//! (`@binance/agentic-wallet/dist/index.js`), which builds them from one constant:
//! `static BASE_PATH=process.env.PREDICTION_API_PREFIX||"/bapi/defi/v1/public/wallet-direct/prediction"`.
//! All leaves are `POST` with a JSON body; the response envelope is unwrapped by
//! `Client::request`, so each call here sees the bundle's `(...).data`.
use crate::client::Client;
use chrono::{Local as LocalTime, TimeZone};
use clap::Args;
use reqwest::Method;
use serde_json::{json, Map, Value};

/// Bundle: `static BASE_PATH=...||"/bapi/defi/v1/public/wallet-direct/prediction"`.
const P: &str = "/bapi/defi/v1/public/wallet-direct/prediction";
/// Bundle: `var za=1e15,Ja=1e5,Ka=100,Ya=20` — pagination bounds shared by every list leaf.
const MAX_OFFSET: u64 = 100_000;
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 20;
/// Bundle: `Te(o.limit,50)` inside `prediction market search`.
const MAX_SEARCH_LIMIT: u32 = 50;
/// Bundle: `function tr(t){...e<=Number.MAX_SAFE_INTEGER}` — ids are safe integers.
const MAX_ID: u64 = 9_007_199_254_740_991;

#[derive(clap::Subcommand, Debug)]
pub enum PredictionCommand {
    /// Prediction market categories
    Category {
        #[command(subcommand)]
        command: CategoryCommand,
    },
    /// Prediction market queries (list, detail, search, order-book, last-trade-price)
    Market {
        #[command(subcommand)]
        command: MarketCommand,
    },
    /// Prediction position queries (list, token, settled-history, pnl, portfolio)
    Position {
        #[command(subcommand)]
        command: PositionCommand,
    },
    /// Prediction order queries (history)
    Order {
        #[command(subcommand)]
        command: OrderCommand,
    },
    /// Prediction trading commands (quote, place-order, cancel, redeem)
    Trade {
        #[command(subcommand)]
        command: TradeCommand,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum CategoryCommand {
    /// List prediction market categories (no auth required)
    #[command(
        after_help = "Examples:\n  baw prediction category list\n  baw prediction category list --json"
    )]
    List,
}

#[derive(clap::Subcommand, Debug)]
pub enum MarketCommand {
    /// List prediction markets
    #[command(
        after_help = "Examples:\n  baw prediction market list\n  baw prediction market list --l1Category crypto --limit 5\n  baw prediction market list --sortBy VOLUME --orderBy DESC --json"
    )]
    List(MarketListArgs),
    /// Get prediction market details
    #[command(
        after_help = "Examples:\n  baw prediction market detail --marketTopicId 123456\n  baw prediction market detail --marketTopicId 123456 --json"
    )]
    Detail(MarketDetailArgs),
    /// Search prediction markets by keyword
    #[command(
        after_help = "Examples:\n  baw prediction market search --query \"Bitcoin\"\n  baw prediction market search --query \"FIFA World Cup\" --limit 5 --json"
    )]
    Search(MarketSearchArgs),
    /// Get order book for an outcome
    #[command(
        after_help = "Examples:\n  baw prediction market order-book --marketId 5237837 --tokenId 123456789 --json\n\nNote: --marketId is the sub-market ID, not the marketTopicId.\nUse 'baw prediction market detail --marketTopicId <id>' to find marketId."
    )]
    OrderBook(OrderBookArgs),
    /// Get last trade price for a market
    #[command(
        after_help = "Examples:\n  baw prediction market last-trade-price --marketId 5237837 --json\n\nNote: --marketId is the sub-market ID, not the marketTopicId.\nUse 'baw prediction market detail --marketTopicId <id>' to find marketId."
    )]
    LastTradePrice(LastTradePriceArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum PositionCommand {
    /// List prediction positions with PnL summary
    #[command(
        after_help = "Examples:\n  baw prediction position list\n  baw prediction position list --tab PENDING_CLAIM --json"
    )]
    List(PositionListArgs),
    /// Get position by token ID
    #[command(
        after_help = "Examples:\n  baw prediction position token --tokenId 123456789\n  baw prediction position token --tokenId 123456789 --json"
    )]
    Token(PositionTokenArgs),
    /// List settled position history
    #[command(
        after_help = "Examples:\n  baw prediction position settled-history\n  baw prediction position settled-history --filter win\n  baw prediction position settled-history --filter lose --json"
    )]
    SettledHistory(SettledHistoryArgs),
    /// Query PNL records for prediction positions
    #[command(
        after_help = "Examples:\n  baw prediction position pnl\n  baw prediction position pnl --l1Category crypto\n  baw prediction position pnl --tokenId abc123 --json"
    )]
    Pnl(PnlArgs),
    /// Query prediction portfolio summary with active positions and unrealized PNL
    #[command(
        after_help = "Examples:\n  baw prediction position portfolio\n  baw prediction position portfolio --json"
    )]
    Portfolio,
}

#[derive(clap::Subcommand, Debug)]
pub enum OrderCommand {
    /// List prediction order history
    #[command(
        after_help = "Examples:\n  baw prediction order history\n  baw prediction order history --status FILLED --limit 10\n  baw prediction order history --orderType LIMIT --json"
    )]
    History(OrderHistoryArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum TradeCommand {
    /// Get a prediction trade quote
    #[command(
        after_help = "Examples:\n  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 10 --orderType MARKET\n  baw prediction trade quote --binanceChainId 56 --tokenId abc123 --marketTopicId 100 --side BUY --amount 5 --orderType LIMIT --priceLimit 0.6 --json"
    )]
    Quote(TradeQuoteArgs),
    /// Place a prediction order using a quote ID
    #[command(
        after_help = "Examples:\n  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000\n  baw prediction trade place-order --quoteId quote_abc123 --slippageBps 1000 --orderType LIMIT --priceLimit 0.6 --json"
    )]
    PlaceOrder(PlaceOrderArgs),
    /// Cancel prediction orders
    #[command(
        after_help = "Examples:\n  baw prediction trade cancel --orderIds order_123,order_456\n  baw prediction trade cancel --orderIds order_123 --json"
    )]
    Cancel(CancelArgs),
    /// Redeem winning prediction positions
    #[command(
        after_help = "Examples:\n  baw prediction trade redeem --tokenIds token_123,token_456\n  baw prediction trade redeem --tokenIds token_123 --binanceChainId 56 --json"
    )]
    Redeem(RedeemArgs),
}

#[derive(Args, Clone, Debug, Default)]
pub struct MarketListArgs {
    /// L1 category filter (e.g. crypto, sports)
    #[arg(long = "l1Category", value_name = "category")]
    pub l1_category: Option<String>,
    /// L2 subcategory filter
    #[arg(long = "l2Category", value_name = "category")]
    pub l2_category: Option<String>,
    /// Sort: RECOMMENDED, VOLUME, PARTICIPANTS, CREATED_TIME, END_DATE
    #[arg(long = "sortBy", value_name = "sort")]
    pub sort_by: Option<String>,
    /// Order: ASC or DESC (default: DESC)
    #[arg(long = "orderBy", value_name = "order")]
    pub order_by: Option<String>,
    /// Pagination offset (default: 0)
    #[arg(long = "offset", value_name = "offset")]
    pub offset: Option<String>,
    /// Page size (default: 20, max: 100)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct MarketDetailArgs {
    /// Market topic ID
    #[arg(long = "marketTopicId", value_name = "id")]
    pub market_topic_id: String,
}

#[derive(Args, Clone, Debug, Default)]
pub struct MarketSearchArgs {
    /// Search keyword (max 200 chars)
    #[arg(long = "query", value_name = "query")]
    pub query: String,
    /// Max results (default: 10, max: 50)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct OrderBookArgs {
    /// Market ID (not marketTopicId, use market detail to find)
    #[arg(long = "marketId", value_name = "id")]
    pub market_id: String,
    /// Outcome token ID
    #[arg(long = "tokenId", value_name = "tokenId")]
    pub token_id: String,
}

#[derive(Args, Clone, Debug, Default)]
pub struct LastTradePriceArgs {
    /// Market ID (not marketTopicId, use market detail to find)
    #[arg(long = "marketId", value_name = "id")]
    pub market_id: String,
}

#[derive(Args, Clone, Debug, Default)]
pub struct PositionListArgs {
    /// Filter: ONGOING, ENDED, PENDING_CLAIM (default: ONGOING)
    #[arg(long = "tab", value_name = "tab")]
    pub tab: Option<String>,
    /// Pagination offset (default: 0)
    #[arg(long = "offset", value_name = "offset")]
    pub offset: Option<String>,
    /// Page size (default: 20, max: 100)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct PositionTokenArgs {
    /// ERC-1155 Token ID
    #[arg(long = "tokenId", value_name = "tokenId")]
    pub token_id: String,
}

#[derive(Args, Clone, Debug, Default)]
pub struct SettledHistoryArgs {
    /// L1 category filter
    #[arg(long = "l1Category", value_name = "category")]
    pub l1_category: Option<String>,
    /// Result filter: all, win, lose (default: all)
    #[arg(long = "filter", value_name = "filter")]
    pub filter: Option<String>,
    /// Pagination offset (default: 0)
    #[arg(long = "offset", value_name = "offset")]
    pub offset: Option<String>,
    /// Page size (default: 20, max: 100)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct PnlArgs {
    /// Filter by specific token ID
    #[arg(long = "tokenId", value_name = "tokenId")]
    pub token_id: Option<String>,
    /// L1 category filter: crypto, sports, all
    #[arg(long = "l1Category", value_name = "category")]
    pub l1_category: Option<String>,
    /// Pagination offset (default: 0)
    #[arg(long = "offset", value_name = "offset")]
    pub offset: Option<String>,
    /// Page size (default: 20, max: 100)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct OrderHistoryArgs {
    /// Filter by status: PENDING, SUBMITTED, FILLED, PARTIALLY_FILLED, CANCELLED, FAILED, EXPIRED
    #[arg(long = "status", value_name = "status")]
    pub status: Option<String>,
    /// L1 category filter
    #[arg(long = "l1Category", value_name = "category")]
    pub l1_category: Option<String>,
    /// Filter by type: MARKET, LIMIT
    #[arg(long = "orderType", value_name = "type")]
    pub order_type: Option<String>,
    /// Pagination offset (default: 0)
    #[arg(long = "offset", value_name = "offset")]
    pub offset: Option<String>,
    /// Page size (default: 20, max: 100)
    #[arg(long = "limit", value_name = "limit")]
    pub limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct TradeQuoteArgs {
    /// Binance chain ID: 56 (BSC), 137 (Polygon)
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: String,
    /// ERC1155 Outcome Token ID
    #[arg(long = "tokenId", value_name = "tokenId")]
    pub token_id: String,
    /// Market topic ID. If --slippageBps is omitted, the market default slippage is fetched via this ID
    #[arg(long = "marketTopicId", value_name = "marketTopicId")]
    pub market_topic_id: String,
    /// Trade side: BUY or SELL
    #[arg(long = "side", value_name = "side")]
    pub side: String,
    /// Trade amount in USDT (human-readable)
    #[arg(long = "amount", value_name = "amount")]
    pub amount: String,
    /// Order type: MARKET or LIMIT
    #[arg(long = "orderType", value_name = "type")]
    pub order_type: String,
    /// Slippage in basis points (default: use market slippageBps)
    #[arg(long = "slippageBps", value_name = "bps")]
    pub slippage_bps: Option<String>,
    /// Limit price in USDT (required for LIMIT orders)
    #[arg(long = "priceLimit", value_name = "price")]
    pub price_limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct PlaceOrderArgs {
    /// Quote ID from trade quote command
    #[arg(long = "quoteId", value_name = "quoteId")]
    pub quote_id: String,
    /// Order type: MARKET (default) or LIMIT
    #[arg(long = "orderType", value_name = "type")]
    pub order_type: Option<String>,
    /// Slippage in basis points (from quote response slippageBps field)
    #[arg(long = "slippageBps", value_name = "bps")]
    pub slippage_bps: String,
    /// Limit price in USDT (required for LIMIT orders)
    #[arg(long = "priceLimit", value_name = "price")]
    pub price_limit: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct CancelArgs {
    /// Comma-separated order IDs to cancel
    #[arg(long = "orderIds", value_name = "ids")]
    pub order_ids: String,
}

#[derive(Args, Clone, Debug, Default)]
pub struct RedeemArgs {
    /// Comma-separated list of winning token IDs
    #[arg(long = "tokenIds", value_name = "ids")]
    pub token_ids: String,
    /// Binance chain ID filter
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
}

/// Exactly the three trade leaves that write; every other leaf is a read.
pub fn is_mutation(command: &PredictionCommand) -> bool {
    matches!(
        command,
        PredictionCommand::Trade {
            command: TradeCommand::PlaceOrder(_)
                | TradeCommand::Cancel(_)
                | TradeCommand::Redeem(_)
        }
    )
}

/// Deterministic intent digest input: identifiers, amounts, side only.
pub fn mutation_payload(command: &PredictionCommand) -> Result<Value, String> {
    match command {
        PredictionCommand::Trade {
            command: TradeCommand::PlaceOrder(args),
        } => {
            let (quote_id, order_type, slippage_bps, price_limit) = place_order_checks(args)?;
            Ok(json!({
                "operation": "prediction.place-order",
                "quoteId": quote_id,
                "orderType": order_type,
                "slippageBps": slippage_bps,
                "priceLimit": price_limit,
            }))
        }
        PredictionCommand::Trade {
            command: TradeCommand::Cancel(args),
        } => Ok(json!({
            "operation": "prediction.cancel",
            "orderIds": cancel_ids(args, "order ids must be a non-empty comma-separated list")?,
        })),
        PredictionCommand::Trade {
            command: TradeCommand::Redeem(args),
        } => {
            let (token_ids, chain) = redeem_checks(args)?;
            Ok(json!({
                "operation": "prediction.redeem",
                "tokenIds": token_ids,
                "binanceChainId": chain,
            }))
        }
        _ => Err("not a mutation".into()),
    }
}

pub async fn run(client: &mut Client, command: &PredictionCommand) -> Result<Value, String> {
    match command {
        PredictionCommand::Category { command } => category(client, command).await,
        PredictionCommand::Market { command } => market(client, command).await,
        PredictionCommand::Position { command } => position(client, command).await,
        PredictionCommand::Order { command } => order(client, command).await,
        PredictionCommand::Trade { command } => trade(client, command).await,
    }
}

async fn category(client: &mut Client, command: &CategoryCommand) -> Result<Value, String> {
    match command {
        // Bundle: `async listCategories(){return this.request(t.CATEGORY_LIST,{method:"POST",body:{}})}`
        // with `static CATEGORY_LIST=`${t.BASE_PATH}/category/list``.
        CategoryCommand::List => {
            client
                .request(Method::POST, &format!("{P}/category/list"), Some(json!({})))
                .await
        }
    }
}

async fn market(client: &mut Client, command: &MarketCommand) -> Result<Value, String> {
    match command {
        // Bundle: `async listMarkets(e){return(await this.api.listMarkets({...})).data}`
        MarketCommand::List(args) => {
            let body = market_list_body(args)?;
            client
                .request(Method::POST, &format!("{P}/agent/market/list"), Some(body))
                .await
        }
        // Bundle: `async getMarketDetail(e){let n=await this.api.getMarketDetail({marketTopicId:e});
        //          if(n.data==null)throw...; return n.data}`.
        MarketCommand::Detail(args) => {
            let id = positive_id(
                &args.market_topic_id,
                "invalid marketTopicId: must be a positive integer",
            )?;
            let data = client
                .request(
                    Method::POST,
                    &format!("{P}/agent/market/detail"),
                    Some(json!({"marketTopicId": id})),
                )
                .await?;
            if data.is_null() {
                return Err("market topic not found".into());
            }
            Ok(data)
        }
        // Bundle: `async searchMarkets(e){return(await this.api.searchMarkets({query:e.query,
        //          ...e.limit!==void 0&&{topK:e.limit}})).data}`.
        MarketCommand::Search(args) => {
            let body = market_search_body(args)?;
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/market/search"),
                    Some(body),
                )
                .await
        }
        // Bundle: `async queryOrderBook(e){return(await this.api.queryOrderBook({marketId:e.marketId,
        //          tokenId:e.tokenId})).data}`.
        MarketCommand::OrderBook(args) => {
            let market_id = positive_id(
                &args.market_id,
                "invalid marketId: must be a positive integer",
            )?;
            let token = token_id(&args.token_id, "invalid tokenId")?;
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/order-book"),
                    Some(json!({"marketId": market_id, "tokenId": token})),
                )
                .await
        }
        // Bundle: `async getLastTradePrice(e){return(await this.api.getLastTradePrice({marketId:e})).data}`
        // with `static LAST_TRADE_PRICE=`${t.BASE_PATH}/agent/order-book/last-trade-price``.
        MarketCommand::LastTradePrice(args) => {
            let market_id = positive_id(
                &args.market_id,
                "invalid marketId: must be a positive integer",
            )?;
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/order-book/last-trade-price"),
                    Some(json!({"marketId": market_id})),
                )
                .await
        }
    }
}

async fn position(client: &mut Client, command: &PositionCommand) -> Result<Value, String> {
    match command {
        // Bundle: `async queryPositions(e){return(await this.api.queryPositions({...e.tab&&{tab:e.tab},
        //          offset:e.offset??0,limit:e.limit??20})).data}`; the CLI supplies `tab:o.tab??"ONGOING"`.
        PositionCommand::List(args) => {
            let tab = args.tab.as_deref().unwrap_or("ONGOING");
            if !["ONGOING", "ENDED", "PENDING_CLAIM"].contains(&tab) {
                return Err("invalid tab; valid: ONGOING, ENDED, PENDING_CLAIM".into());
            }
            let mut body = Map::new();
            body.insert("tab".into(), json!(tab));
            body.insert("offset".into(), json!(offset(args.offset.as_deref())?));
            body.insert(
                "limit".into(),
                json!(limit(
                    args.limit.as_deref(),
                    MAX_LIMIT,
                    DEFAULT_LIMIT,
                    "invalid limit (1-100)"
                )?),
            );
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/position/list"),
                    Some(Value::Object(body)),
                )
                .await
        }
        // Bundle: `async getPositionByToken(e){let n=await this.api.getPositionByToken({tokenId:e});
        //          return n.data.position??n.data}`.
        PositionCommand::Token(args) => {
            let token = token_id(&args.token_id, "invalid tokenId")?;
            let data = client
                .request(
                    Method::POST,
                    &format!("{P}/agent/position/token"),
                    Some(json!({"tokenId": token})),
                )
                .await?;
            Ok(data
                .get("position")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or(data))
        }
        // Bundle: `async querySettledHistory(e){let n=Pd(e.filter);return(await
        //          this.api.querySettledHistory({...e.l1Category&&{l1Category:e.l1Category},
        //          ...n!==void 0&&{result:n},offset:...,limit:...})).data}`
        // with `function Pd(t){return!t||t==="all"?void 0:{win:1,lose:0}[t.toLowerCase()]}`.
        PositionCommand::SettledHistory(args) => {
            let mut body = Map::new();
            if let Some(category) = &args.l1_category {
                body.insert("l1Category".into(), json!(text(category, 64)?));
            }
            match args.filter.as_deref() {
                None | Some("all") => {}
                Some("win") => {
                    body.insert("result".into(), json!(1));
                }
                Some("lose") => {
                    body.insert("result".into(), json!(0));
                }
                Some(_) => return Err("invalid filter; valid: all, win, lose".into()),
            }
            body.insert("offset".into(), json!(offset(args.offset.as_deref())?));
            body.insert(
                "limit".into(),
                json!(limit(
                    args.limit.as_deref(),
                    MAX_LIMIT,
                    DEFAULT_LIMIT,
                    "invalid limit (1-100)"
                )?),
            );
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/position/settled-history"),
                    Some(Value::Object(body)),
                )
                .await
        }
        // Bundle: `async queryPnl(e){return(await this.api.queryPnl({...e.tokenId&&{tokenId:e.tokenId},
        //          ...e.l1Category&&{l1Category:e.l1Category},offset:...,limit:...})).data}`.
        PositionCommand::Pnl(args) => {
            let mut body = Map::new();
            if let Some(token) = &args.token_id {
                body.insert("tokenId".into(), json!(token_id(token, "invalid tokenId")?));
            }
            if let Some(category) = &args.l1_category {
                body.insert("l1Category".into(), json!(text(category, 64)?));
            }
            body.insert("offset".into(), json!(offset(args.offset.as_deref())?));
            body.insert(
                "limit".into(),
                json!(limit(
                    args.limit.as_deref(),
                    MAX_LIMIT,
                    DEFAULT_LIMIT,
                    "invalid limit (1-100)"
                )?),
            );
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/pnl/query"),
                    Some(Value::Object(body)),
                )
                .await
        }
        // Bundle: `async getPortfolio(){return(await this.api.getPortfolio()).data}` with
        // `async getPortfolio(){return this.request(t.GET_PORTFOLIO,{method:"POST",body:{}})}`.
        PositionCommand::Portfolio => {
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/pnl/portfolio"),
                    Some(json!({})),
                )
                .await
        }
    }
}

async fn order(client: &mut Client, command: &OrderCommand) -> Result<Value, String> {
    match command {
        // Bundle: `async queryOrderHistory(e){return(await this.api.queryOrderHistory({...e.status&&{status:e.status},
        //          ...e.l1Category&&{l1Category:e.l1Category},...e.orderType&&{orderType:e.orderType},
        //          offset:...,limit:...})).data}`.
        OrderCommand::History(args) => {
            let mut body = Map::new();
            if let Some(status) = &args.status {
                if ![
                    "PENDING",
                    "SUBMITTED",
                    "FILLED",
                    "PARTIALLY_FILLED",
                    "CANCELLED",
                    "FAILED",
                    "EXPIRED",
                ]
                .contains(&status.as_str())
                {
                    return Err("invalid status; valid: PENDING, SUBMITTED, FILLED, PARTIALLY_FILLED, CANCELLED, FAILED, EXPIRED".into());
                }
                body.insert("status".into(), json!(status));
            }
            if let Some(category) = &args.l1_category {
                body.insert("l1Category".into(), json!(text(category, 64)?));
            }
            if let Some(order_type) = &args.order_type {
                if !["MARKET", "LIMIT"].contains(&order_type.as_str()) {
                    return Err("invalid orderType; valid: MARKET, LIMIT".into());
                }
                body.insert("orderType".into(), json!(order_type));
            }
            body.insert("offset".into(), json!(offset(args.offset.as_deref())?));
            body.insert(
                "limit".into(),
                json!(limit(
                    args.limit.as_deref(),
                    MAX_LIMIT,
                    DEFAULT_LIMIT,
                    "invalid limit (1-100)"
                )?),
            );
            client
                .request(
                    Method::POST,
                    &format!("{P}/agent/order/history"),
                    Some(Value::Object(body)),
                )
                .await
        }
    }
}

async fn trade(client: &mut Client, command: &TradeCommand) -> Result<Value, String> {
    match command {
        TradeCommand::Quote(args) => quote(client, args).await,
        // Bundle: `async placeOrder(e){return{orderId:(await this.api.placeOrder({quoteId:e.quoteId,
        //          orderType:e.orderType||"MARKET",...slippageBps...{slippageBps:...},
        //          ...priceLimit...{priceLimit:...},fundingSource:"MPC"})).data.orderId}}`.
        TradeCommand::PlaceOrder(args) => {
            let (quote_id, order_type, slippage_bps, price_limit) = place_order_checks(args)?;
            let mut body = Map::new();
            body.insert("quoteId".into(), json!(quote_id));
            body.insert("orderType".into(), json!(order_type));
            body.insert("slippageBps".into(), json!(slippage_bps));
            if let Some(price_limit) = price_limit {
                body.insert("priceLimit".into(), json!(price_limit));
            }
            body.insert("fundingSource".into(), json!("MPC"));
            let data = client
                .request(
                    Method::POST,
                    &format!("{P}/agent/trade/place-order-bundle"),
                    Some(Value::Object(body)),
                )
                .await?;
            Ok(project(&data, &["orderId"]))
        }
        // Bundle: `async cancelOrders(e){let r=(await this.api.batchCancel({cancelInfoList:e.cancelInfoList})).data;
        //          return{canceled:r.canceled||[],failed:(r.failed||[]).map(o=>({orderId:o.orderId,reason:o.reason}))}}`
        // with the CLI building `s.map(c=>({orderId:c}))` from `--orderIds`.
        TradeCommand::Cancel(args) => {
            let order_ids = cancel_ids(args, "order ids must be a non-empty comma-separated list")?;
            let cancel_info_list = order_ids
                .iter()
                .map(|order_id| json!({"orderId": order_id}))
                .collect::<Vec<_>>();
            let data = client
                .request(
                    Method::POST,
                    &format!("{P}/agent/trade/batch-cancel"),
                    Some(json!({"cancelInfoList": cancel_info_list})),
                )
                .await?;
            let canceled = data
                .get("canceled")
                .filter(|value| value.is_array())
                .cloned()
                .unwrap_or_else(|| json!([]));
            let failed = data
                .get("failed")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| project(row, &["orderId", "reason"]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(json!({"canceled": canceled, "failed": failed}))
        }
        // Bundle: `async redeemPositions(e){let r=(await this.api.batchRedeem({tokenIds:e.tokenIds,
        //          ...e.binanceChainId&&{binanceChainId:e.binanceChainId}})).data;
        //          return{batchId:r.batchId,results:(r.results||[]).map(o=>({requestId:o.requestId,
        //          txHash:o.txHash,status:o.status,error:o.error}))}}`.
        TradeCommand::Redeem(args) => {
            let (token_ids, chain) = redeem_checks(args)?;
            let mut body = Map::new();
            body.insert("tokenIds".into(), json!(token_ids));
            if let Some(chain) = chain {
                body.insert("binanceChainId".into(), json!(chain));
            }
            let data = client
                .request(
                    Method::POST,
                    &format!("{P}/agent/batch-redeem"),
                    Some(Value::Object(body)),
                )
                .await?;
            let mut out = project(&data, &["batchId"]);
            let results = data
                .get("results")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| project(row, &["requestId", "txHash", "status", "error"]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            out["results"] = Value::Array(results);
            Ok(out)
        }
    }
}

/// Bundle: `async getQuote(e){let r=(await this.api.getQuote({binanceChainId:e.binanceChainId,
/// tokenId:e.tokenId,side:e.side,amountIn:kd(e.amount),orderType:e.orderType,
/// ...slippageBps...{slippageBps:...},...feeRateBps...{...},...priceLimit...{priceLimit:...}})).data;
/// return{quoteId:r.quoteId,...,amountIn:Cn(r.amountIn),...}}` — the service reshapes the quote, so
/// `quote_view` mirrors that projection (18-decimal amounts rendered as decimals, `expireAt` formatted).
async fn quote(client: &mut Client, args: &TradeQuoteArgs) -> Result<Value, String> {
    if !["BUY", "SELL"].contains(&args.side.as_str()) {
        return Err("invalid side; valid: BUY, SELL".into());
    }
    amount(&args.amount)?;
    if !["MARKET", "LIMIT"].contains(&args.order_type.as_str()) {
        return Err("invalid orderType; valid: MARKET, LIMIT".into());
    }
    if args.order_type == "LIMIT" && !has_price_limit(&args.price_limit) {
        return Err("priceLimit is required for LIMIT orders".into());
    }
    let slippage_bps = match &args.slippage_bps {
        Some(raw) => Some(json!(slippage_bps(raw)?)),
        // The CLI falls back to the market default slippage and swallows lookup failures:
        // `if(s===void 0&&o.marketTopicId!==void 0)try{let l=await i.getMarketDetail(Number(o.marketTopicId));
        //  l.slippageBps!==void 0&&(s=l.slippageBps)}catch{}`.
        None => market_slippage(client, &args.market_topic_id).await,
    };
    let mut body = Map::new();
    body.insert(
        "binanceChainId".into(),
        json!(text(&args.binance_chain_id, 32).map_err(|_| String::from("invalid binanceChainId"))?),
    );
    body.insert(
        "tokenId".into(),
        json!(token_id(&args.token_id, "invalid tokenId")?),
    );
    body.insert("side".into(), json!(args.side));
    body.insert("amountIn".into(), json!(to_fixed_18(&args.amount)));
    body.insert("orderType".into(), json!(args.order_type));
    if let Some(slippage_bps) = slippage_bps {
        body.insert("slippageBps".into(), slippage_bps);
    }
    if let Some(price_limit) = &args.price_limit {
        body.insert("priceLimit".into(), json!(price_limit));
    }
    let data = client
        .request(
            Method::POST,
            &format!("{P}/agent/trade/get-quote"),
            Some(Value::Object(body)),
        )
        .await?;
    Ok(quote_view(&data))
}

async fn market_slippage(client: &mut Client, market_topic_id: &str) -> Option<Value> {
    let id = positive_id(market_topic_id, "invalid marketTopicId").ok()?;
    let data = client
        .request(
            Method::POST,
            &format!("{P}/agent/market/detail"),
            Some(json!({"marketTopicId": id})),
        )
        .await
        .ok()?;
    data.get("slippageBps")
        .filter(|value| !value.is_null())
        .cloned()
}

fn market_list_body(args: &MarketListArgs) -> Result<Value, String> {
    let mut body = Map::new();
    if let Some(category) = &args.l1_category {
        body.insert("l1Category".into(), json!(text(category, 64)?));
    }
    if let Some(category) = &args.l2_category {
        body.insert("l2Category".into(), json!(text(category, 64)?));
    }
    if let Some(sort) = &args.sort_by {
        if ![
            "RECOMMENDED",
            "VOLUME",
            "PARTICIPANTS",
            "CREATED_TIME",
            "END_DATE",
        ]
        .contains(&sort.as_str())
        {
            return Err(
                "invalid sortBy; valid: RECOMMENDED, VOLUME, PARTICIPANTS, CREATED_TIME, END_DATE"
                    .into(),
            );
        }
        body.insert("sortBy".into(), json!(sort));
    }
    let order_by = args.order_by.as_deref().unwrap_or("DESC");
    if !["ASC", "DESC"].contains(&order_by) {
        return Err("invalid orderBy; valid: ASC, DESC".into());
    }
    body.insert("orderBy".into(), json!(order_by));
    body.insert("offset".into(), json!(offset(args.offset.as_deref())?));
    body.insert(
        "limit".into(),
        json!(limit(
            args.limit.as_deref(),
            MAX_LIMIT,
            DEFAULT_LIMIT,
            "invalid limit (1-100)"
        )?),
    );
    Ok(Value::Object(body))
}

fn market_search_body(args: &MarketSearchArgs) -> Result<Value, String> {
    let query = text(&args.query, 200)
        .map_err(|_| String::from("invalid query: must be non-empty, max 200 chars"))?;
    // Help text says "default: 10" but the bundle routes this through the shared `Te` helper,
    // which defaults to 20 (`var ...Ya=20`) and always sends `topK`.
    let top_k = limit(
        args.limit.as_deref(),
        MAX_SEARCH_LIMIT,
        DEFAULT_LIMIT,
        "invalid limit (1-50)",
    )?;
    Ok(json!({"query": query, "topK": top_k}))
}

/// Returns `(quoteId, orderType, slippageBps, priceLimit)`; LIMIT requires a price limit.
fn place_order_checks(
    args: &PlaceOrderArgs,
) -> Result<(String, String, u32, Option<String>), String> {
    let quote_id = text(&args.quote_id, 200).map_err(|_| String::from("invalid quoteId"))?;
    let order_type = match args.order_type.as_deref() {
        None => String::from("MARKET"),
        Some(value) if ["MARKET", "LIMIT"].contains(&value) => value.to_owned(),
        Some(_) => return Err("invalid orderType; valid: MARKET, LIMIT".into()),
    };
    if order_type == "LIMIT" && !has_price_limit(&args.price_limit) {
        return Err("priceLimit is required for LIMIT orders".into());
    }
    let bps = slippage_bps(&args.slippage_bps)?;
    let price_limit = args
        .price_limit
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Ok((quote_id, order_type, bps, price_limit))
}

fn cancel_ids(args: &CancelArgs, message: &'static str) -> Result<Vec<String>, String> {
    let ids = comma_list(&args.order_ids, message)?;
    Ok(ids)
}

fn redeem_checks(args: &RedeemArgs) -> Result<(Vec<String>, Option<String>), String> {
    let token_ids = comma_list(
        &args.token_ids,
        "token ids must be a non-empty comma-separated list",
    )?;
    let chain = args
        .binance_chain_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| text(value, 32))
        .transpose()
        .map_err(|_| String::from("invalid binanceChainId"))?;
    Ok((token_ids, chain))
}

/// Bundle: `o.orderIds.split(",").map(c=>c.trim()).filter(Boolean)` guarded by a non-empty check.
fn comma_list(raw: &str, message: &'static str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        values.push(text(trimmed, 200).map_err(|_| message.to_owned())?);
    }
    if values.is_empty() {
        return Err(message.into());
    }
    Ok(values)
}

fn has_price_limit(price_limit: &Option<String>) -> bool {
    price_limit
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn project(row: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for key in keys {
        if let Some(value) = row.get(*key) {
            out.insert((*key).to_owned(), value.clone());
        }
    }
    Value::Object(out)
}

/// Bundle: `return{quoteId:r.quoteId,tokenId:r.tokenId,side:r.side,amountIn:Cn(r.amountIn),
/// amountOut:Cn(r.amountOut),averagePrice:r.averagePrice,lastPrice:r.lastPrice,priceImpact:r.priceImpact,
/// feeAmount:Cn(r.feeAmount),minReceive:Cn(r.minReceive),expireAt:X(r.expireAt),protocol:r.protocol,
/// chainId:r.chainId,orderType:r.orderType,slippageBps:r.slippageBps,marketTitle:r.marketTitle,
/// outcomeName:r.outcomeName}` — absent keys stay absent, `amount*`/`fee`/`minReceive` always present.
fn quote_view(data: &Value) -> Value {
    let mut out = Map::new();
    for key in ["quoteId", "tokenId", "side"] {
        if let Some(value) = data.get(key) {
            out.insert(key.to_owned(), value.clone());
        }
    }
    out.insert("amountIn".into(), from_fixed_18(data.get("amountIn")));
    out.insert("amountOut".into(), from_fixed_18(data.get("amountOut")));
    for key in ["averagePrice", "lastPrice", "priceImpact"] {
        if let Some(value) = data.get(key) {
            out.insert(key.to_owned(), value.clone());
        }
    }
    out.insert("feeAmount".into(), from_fixed_18(data.get("feeAmount")));
    out.insert("minReceive".into(), from_fixed_18(data.get("minReceive")));
    out.insert("expireAt".into(), formatted_time(data.get("expireAt")));
    for key in [
        "protocol",
        "chainId",
        "orderType",
        "slippageBps",
        "marketTitle",
        "outcomeName",
    ] {
        if let Some(value) = data.get(key) {
            out.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(out)
}

/// Bundle: `function X(t){if(t==null||t===0)return null;...}` — milliseconds to
/// `YYYY-MM-DDTHH:MM:SS±HH:MM` in the process time zone (the bundle uses `Date` local time).
fn formatted_time(value: Option<&Value>) -> Value {
    let Some(millis) = value.and_then(Value::as_i64).filter(|millis| *millis != 0) else {
        return Value::Null;
    };
    match LocalTime.timestamp_millis_opt(millis).single() {
        Some(stamp) => Value::String(stamp.format("%Y-%m-%dT%H:%M:%S%:z").to_string()),
        None => Value::Null,
    }
}

/// Bundle: `function kd(t){let e=t.split("."),n=e[0]||"0",r=(e[1]||"").padEnd(18,"0").slice(0,18);
/// return(n+r).replace(/^0+/,"")||"0"}` — human decimal to 18-decimal fixed point (truncating).
fn to_fixed_18(text: &str) -> String {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (text, ""),
    };
    let whole = if whole.is_empty() { "0" } else { whole };
    let mut fraction: String = fraction.chars().take(18).collect();
    while fraction.len() < 18 {
        fraction.push('0');
    }
    let joined = format!("{whole}{fraction}");
    let trimmed = joined.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Bundle: `function Cn(t){if(!t)return"0";let e=t.padStart(19,"0"),n=e.slice(0,e.length-18)||"0",
/// o=e.slice(e.length-18).replace(/0+$/,"");return o?`${n}.${o}`:n}`.
fn from_fixed_18(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::String("0".into()),
        Some(Value::String(text)) if text.is_empty() => Value::String("0".into()),
        Some(Value::String(text)) => Value::String(decimal_from_fixed(text)),
        Some(other) => other.clone(),
    }
}

fn decimal_from_fixed(text: &str) -> String {
    let mut padded = String::new();
    let width = text.chars().count();
    for _ in width..19 {
        padded.push('0');
    }
    padded.push_str(text);
    let chars: Vec<char> = padded.chars().collect();
    let split = chars.len() - 18;
    let whole: String = chars[..split].iter().collect();
    let whole = if whole.is_empty() {
        String::from("0")
    } else {
        whole
    };
    let fraction: String = chars[split..].iter().collect();
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        whole
    } else {
        format!("{whole}.{fraction}")
    }
}

/// Bundle: `function ve(t,e=xa){return typeof t!="string"||t.length>e||$a.test(t)?null:t.trim()}`
/// with `$a=/[\x00-\x1F\x7F<>{}|\\^`\[\]]/`.
fn text(raw: &str, max: usize) -> Result<String, String> {
    if raw.len() > max || raw.chars().any(forbidden) {
        return Err("invalid text value".into());
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("invalid text value".into());
    }
    Ok(trimmed.to_owned())
}

fn forbidden(value: char) -> bool {
    value.is_control()
        || matches!(
            value,
            '<' | '>' | '{' | '}' | '|' | '\\' | '^' | '`' | '[' | ']'
        )
}

/// Bundle: `function tr(t){...parseInt...e<=0||e>Number.MAX_SAFE_INTEGER?null:e}` (also used by `en`).
fn positive_id(raw: &str, message: &'static str) -> Result<u64, String> {
    let value = unsigned(raw).ok_or(message)?;
    if value == 0 || value > MAX_ID {
        return Err(message.into());
    }
    Ok(value)
}

/// Bundle: `function Oe(t){if(t===void 0)return 0;...e<0||e>Ja?null:e}` with `Ja=1e5`.
fn offset(raw: Option<&str>) -> Result<u64, String> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    let value = unsigned(raw).ok_or("invalid offset")?;
    if value > MAX_OFFSET {
        return Err("invalid offset".into());
    }
    Ok(value)
}

/// Bundle: `function Te(t,e=Ka){if(t===void 0)return Ya;...n<1||n>e?null:n}` with `Ka=100,Ya=20`.
fn limit(raw: Option<&str>, max: u32, default: u32, message: &'static str) -> Result<u32, String> {
    let Some(raw) = raw else {
        return Ok(default);
    };
    let value = unsigned(raw).ok_or(message)?;
    if value < 1 || value > u64::from(max) {
        return Err(message.into());
    }
    Ok(value as u32)
}

/// Bundle: `function tn(t){...e<0||e>1e4?null:e}` for `--slippageBps`.
fn slippage_bps(raw: &str) -> Result<u32, String> {
    let value = unsigned(raw).ok_or("invalid slippageBps (0-10000)")?;
    if value > 10_000 {
        return Err("invalid slippageBps (0-10000)".into());
    }
    Ok(value as u32)
}

/// Bundle: `function se(t){.../^\d+(\.\d+)?$/.test(t)...e>0&&e<=Qa}` with `Qa=1e18`.
fn amount(raw: &str) -> Result<(), String> {
    let invalid = || String::from("invalid amount");
    if raw.is_empty() || raw.len() > 512 {
        return Err(invalid());
    }
    let mut seen_dot = false;
    for byte in raw.bytes() {
        match byte {
            b'0'..=b'9' => {}
            b'.' if !seen_dot => seen_dot = true,
            _ => return Err(invalid()),
        }
    }
    if raw.starts_with('.') || raw.ends_with('.') {
        return Err(invalid());
    }
    let value = crate::rwa::decimal(raw).map_err(|_| invalid())?;
    let zero = crate::rwa::decimal("0").map_err(|_| invalid())?;
    let ceiling = crate::rwa::decimal("1000000000000000000").map_err(|_| invalid())?;
    if value <= zero || value > ceiling {
        return Err(invalid());
    }
    Ok(())
}

/// Bundle: `function Zt(t){.../^[a-zA-Z0-9_-]+$/||/^0x[a-fA-F0-9]+$/...}`.
fn token_id(raw: &str, message: &'static str) -> Result<String, String> {
    let value = text(raw, 200).map_err(|_| message.to_owned())?;
    let plain = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    let hex = value
        .strip_prefix("0x")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if plain || hex {
        Ok(value)
    } else {
        Err(message.into())
    }
}

fn unsigned(raw: &str) -> Option<u64> {
    if raw.is_empty() || raw.len() > 18 || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Loopback backend that asserts the request line of every reply in order.
    async fn server(
        replies: Vec<(String, Value)>,
    ) -> (Client, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (expected, data) in replies {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .expect("no connection")
                        .unwrap();
                let mut input = Vec::new();
                let mut chunk = [0u8; 4096];
                let head = loop {
                    let read = socket.read(&mut chunk).await.unwrap();
                    assert!(read > 0, "connection closed before headers");
                    input.extend_from_slice(&chunk[..read]);
                    if let Some(end) = input.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&input[..head]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                while input.len() < head + length {
                    let read = socket.read(&mut chunk).await.unwrap();
                    assert!(read > 0, "connection closed before body");
                    input.extend_from_slice(&chunk[..read]);
                }
                let request = String::from_utf8(input).unwrap();
                assert!(
                    request.starts_with(&expected),
                    "expected {expected}, got {:?}",
                    request.lines().next().unwrap_or_default()
                );
                requests.push(request);
                let body = json!({"code":"000000","data":data}).to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
            requests
        });
        let credentials = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        (Client::test_client(credentials, &origin).unwrap(), task)
    }

    fn endpoint(path: &str) -> String {
        format!("POST {P}{path} ")
    }

    fn body(request: &str) -> Value {
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    fn credentials() -> crate::session::Credentials {
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    #[tokio::test]
    async fn category_list_posts_empty_body() {
        let data = json!({"categories":[{"id":"crypto","name":"Crypto","subcategories":[{"id":"btc","name":"BTC"}]}]});
        let (mut client, task) = server(vec![(endpoint("/category/list"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Category {
                command: CategoryCommand::List,
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        let requests = task.await.unwrap();
        assert_eq!(body(&requests[0]), json!({}));
    }

    #[tokio::test]
    async fn market_list_sends_filters_order_by_and_pagination() {
        let data = json!({"marketTopics":[{"marketTopicId":100}],"total":1,"hasMore":false});
        let (mut client, task) = server(vec![(endpoint("/agent/market/list"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::List(MarketListArgs {
                    l1_category: Some("crypto".into()),
                    l2_category: Some("btc".into()),
                    sort_by: Some("VOLUME".into()),
                    offset: Some("5".into()),
                    limit: Some("5".into()),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"l1Category":"crypto","l2Category":"btc","sortBy":"VOLUME","orderBy":"DESC","offset":5,"limit":5})
        );
    }

    #[tokio::test]
    async fn market_detail_sends_numeric_topic_id_and_returns_data() {
        let data = json!({"marketTopicId":123456,"title":"FIFA World Cup","markets":[]});
        let (mut client, task) =
            server(vec![(endpoint("/agent/market/detail"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::Detail(MarketDetailArgs {
                    market_topic_id: "123456".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"marketTopicId":123456})
        );
    }

    #[tokio::test]
    async fn market_search_sends_query_and_top_k() {
        let data = json!([{"marketTopicId":100,"title":"Bitcoin"}]);
        let (mut client, task) =
            server(vec![(endpoint("/agent/market/search"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::Search(MarketSearchArgs {
                    query: "Bitcoin".into(),
                    limit: Some("5".into()),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"query":"Bitcoin","topK":5})
        );
    }

    #[tokio::test]
    async fn market_order_book_sends_market_and_token_ids() {
        let data = json!({"tokenId":"123456789","bids":[{"price":"0.4","size":10}],"asks":[]});
        let (mut client, task) = server(vec![(endpoint("/agent/order-book"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::OrderBook(OrderBookArgs {
                    market_id: "5237837".into(),
                    token_id: "123456789".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"marketId":5237837,"tokenId":"123456789"})
        );
    }

    #[tokio::test]
    async fn market_last_trade_price_sends_only_market_id() {
        let data = json!({"marketId":5237837,"lastTradePrice":"0.61"});
        let (mut client, task) = server(vec![(
            endpoint("/agent/order-book/last-trade-price"),
            data.clone(),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::LastTradePrice(LastTradePriceArgs {
                    market_id: "5237837".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(body(&task.await.unwrap()[0]), json!({"marketId":5237837}));
    }

    #[tokio::test]
    async fn position_list_defaults_to_ongoing_tab() {
        let data = json!({"summary":{"totalValue":1.5},"positions":[]});
        let (mut client, task) =
            server(vec![(endpoint("/agent/position/list"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Position {
                command: PositionCommand::List(PositionListArgs {
                    tab: Some("PENDING_CLAIM".into()),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"tab":"PENDING_CLAIM","offset":0,"limit":20})
        );
    }

    #[tokio::test]
    async fn position_token_unwraps_position_key() {
        let position = json!({"tokenId":"123456789","shares":"5","pnl":"0.5"});
        let (mut client, task) = server(vec![(
            endpoint("/agent/position/token"),
            json!({"position": position.clone(), "unused":true}),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Position {
                command: PositionCommand::Token(PositionTokenArgs {
                    token_id: "123456789".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, position);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"tokenId":"123456789"})
        );
    }

    #[tokio::test]
    async fn position_settled_history_maps_win_filter_to_result_one() {
        let data = json!({"positions":[{"tokenId":"t1","isWinner":true}],"totalCount":1});
        let (mut client, task) = server(vec![(
            endpoint("/agent/position/settled-history"),
            data.clone(),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Position {
                command: PositionCommand::SettledHistory(SettledHistoryArgs {
                    l1_category: Some("crypto".into()),
                    filter: Some("win".into()),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"l1Category":"crypto","result":1,"offset":0,"limit":20})
        );
    }

    #[tokio::test]
    async fn position_pnl_sends_token_and_category_filters() {
        let data = json!({"pnlList":[],"totalCount":0});
        let (mut client, task) = server(vec![(endpoint("/agent/pnl/query"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Position {
                command: PositionCommand::Pnl(PnlArgs {
                    token_id: Some("abc123".into()),
                    l1_category: Some("crypto".into()),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"tokenId":"abc123","l1Category":"crypto","offset":0,"limit":20})
        );
    }

    #[tokio::test]
    async fn position_portfolio_posts_empty_body() {
        let data = json!({"walletAddress":"0xabc","activePositionsCount":0});
        let (mut client, task) =
            server(vec![(endpoint("/agent/pnl/portfolio"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Position {
                command: PositionCommand::Portfolio,
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(body(&task.await.unwrap()[0]), json!({}));
    }

    #[tokio::test]
    async fn order_history_sends_status_category_and_type_filters() {
        let data = json!({"orders":[],"total":0});
        let (mut client, task) =
            server(vec![(endpoint("/agent/order/history"), data.clone())]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Order {
                command: OrderCommand::History(OrderHistoryArgs {
                    status: Some("FILLED".into()),
                    l1_category: Some("crypto".into()),
                    order_type: Some("LIMIT".into()),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, data);
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"status":"FILLED","l1Category":"crypto","orderType":"LIMIT","offset":0,"limit":20})
        );
    }

    #[tokio::test]
    async fn trade_quote_converts_amount_and_reshapes_response() {
        let data = json!({
            "quoteId":"quote_abc123","tokenId":"123456789","side":"BUY",
            "amountIn":"10000000000000000000","amountOut":"16000000000000000000",
            "averagePrice":"0.625","lastPrice":"0.62","priceImpact":"0.001",
            "feeAmount":"10000000000000000","minReceive":"15000000000000000000",
            "expireAt":1767225600000i64,"protocol":"clob","chainId":"56","orderType":"MARKET",
            "slippageBps":1000,"marketTitle":"Bitcoin above 100k","outcomeName":"Yes",
            "unknownField":"dropped"
        });
        let (mut client, task) = server(vec![(endpoint("/agent/trade/get-quote"), data)]).await;
        let out = run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::Quote(quote_args()),
            },
        )
        .await
        .unwrap();
        assert_eq!(out["amountIn"], "10");
        assert_eq!(out["amountOut"], "16");
        assert_eq!(out["feeAmount"], "0.01");
        assert_eq!(out["minReceive"], "15");
        assert_eq!(out["averagePrice"], "0.625");
        assert_eq!(out["slippageBps"], 1000);
        assert_eq!(out["outcomeName"], "Yes");
        assert!(out.get("unknownField").is_none());
        let expire_at = out["expireAt"].as_str().unwrap();
        assert_eq!(expire_at.len(), 25, "{expire_at}");
        assert_eq!(&expire_at[4..5], "-");
        assert_eq!(&expire_at[10..11], "T");
        let requests = task.await.unwrap();
        assert_eq!(
            body(&requests[0]),
            json!({"binanceChainId":"56","tokenId":"123456789","side":"BUY","amountIn":"10000000000000000000","orderType":"MARKET","slippageBps":1000})
        );
    }

    #[tokio::test]
    async fn trade_quote_fetches_market_default_slippage_when_omitted() {
        let (mut client, task) = server(vec![
            (
                endpoint("/agent/market/detail"),
                json!({"marketTopicId":100,"slippageBps":250}),
            ),
            (
                endpoint("/agent/trade/get-quote"),
                json!({"quoteId":"quote_1","amountIn":"0","amountOut":"0","feeAmount":"0","minReceive":"0"}),
            ),
        ])
        .await;
        let mut args = quote_args();
        args.slippage_bps = None;
        run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::Quote(args),
            },
        )
        .await
        .unwrap();
        let requests = task.await.unwrap();
        assert_eq!(body(&requests[0]), json!({"marketTopicId":100}));
        assert_eq!(body(&requests[1])["slippageBps"], 250);
    }

    #[tokio::test]
    async fn trade_place_order_defaults_to_market_and_reports_order_id() {
        let (mut client, task) = server(vec![(
            endpoint("/agent/trade/place-order-bundle"),
            json!({"orderId":"order_1"}),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::PlaceOrder(PlaceOrderArgs {
                    quote_id: "quote_abc123".into(),
                    slippage_bps: "1000".into(),
                    ..Default::default()
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out, json!({"orderId":"order_1"}));
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"quoteId":"quote_abc123","orderType":"MARKET","slippageBps":1000,"fundingSource":"MPC"})
        );
    }

    #[tokio::test]
    async fn trade_cancel_sends_cancel_info_list_and_reports_outcomes() {
        let (mut client, task) = server(vec![(
            endpoint("/agent/trade/batch-cancel"),
            json!({"canceled":["order_123"],"failed":[{"orderId":"order_456","reason":"FILLED"}]}),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::Cancel(CancelArgs {
                    order_ids: " order_123 , order_456 ".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            out,
            json!({"canceled":["order_123"],"failed":[{"orderId":"order_456","reason":"FILLED"}]})
        );
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"cancelInfoList":[{"orderId":"order_123"},{"orderId":"order_456"}]})
        );
    }

    #[tokio::test]
    async fn trade_redeem_sends_token_ids_and_optional_chain() {
        let (mut client, task) = server(vec![(
            endpoint("/agent/batch-redeem"),
            json!({"batchId":"batch_1","results":[{"requestId":"r1","txHash":"0xdead","status":"SUBMITTED"}]}),
        )])
        .await;
        let out = run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::Redeem(RedeemArgs {
                    token_ids: "token_123,token_456".into(),
                    binance_chain_id: Some("56".into()),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(out["batchId"], "batch_1");
        assert_eq!(out["results"][0]["requestId"], "r1");
        assert!(out["results"][0].get("error").is_none());
        assert_eq!(
            body(&task.await.unwrap()[0]),
            json!({"tokenIds":["token_123","token_456"],"binanceChainId":"56"})
        );
    }

    #[tokio::test]
    async fn market_detail_rejects_null_payload() {
        let (mut client, task) =
            server(vec![(endpoint("/agent/market/detail"), Value::Null)]).await;
        let error = run(
            &mut client,
            &PredictionCommand::Market {
                command: MarketCommand::Detail(MarketDetailArgs {
                    market_topic_id: "42".into(),
                }),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "market topic not found");
        task.await.unwrap();
    }

    #[tokio::test]
    async fn backend_error_envelope_is_not_echoed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = Vec::new();
            let mut chunk = [0u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            input.extend_from_slice(&chunk[..read]);
            let reply = br#"{"code":"123456","message":"secret-backend-detail","data":null}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.len(),
                        std::str::from_utf8(reply).unwrap()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            input
        });
        let mut client = Client::test_client(credentials(), &origin).unwrap();
        let error = run(
            &mut client,
            &PredictionCommand::Order {
                command: OrderCommand::History(OrderHistoryArgs::default()),
            },
        )
        .await
        .unwrap_err();
        assert!(
            error.starts_with("Binance API code") && !error.contains("secret"),
            "{error}"
        );
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn missing_required_option_never_reaches_network() {
        use clap::Parser;
        let base = [
            "baw-rs",
            "--wallet-dir",
            "/tmp",
            "--instance-id",
            "synthetic",
        ];
        for leaf in [
            vec!["prediction", "market", "detail"],
            vec!["prediction", "market", "search"],
            vec!["prediction", "market", "order-book", "--marketId", "1"],
            vec!["prediction", "position", "token"],
            vec!["prediction", "trade", "place-order", "--quoteId", "q1"],
            vec!["prediction", "trade", "cancel"],
            vec!["prediction", "trade", "redeem"],
        ] {
            let argv: Vec<&str> = base.iter().copied().chain(leaf.iter().copied()).collect();
            assert!(
                crate::cli::Cli::try_parse_from(argv.clone()).is_err(),
                "parsed without required option: {argv:?}"
            );
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &origin).unwrap();
        let mut args = quote_args();
        args.order_type = "LIMIT".into();
        args.price_limit = None;
        let error = run(
            &mut client,
            &PredictionCommand::Trade {
                command: TradeCommand::Quote(args),
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("priceLimit"), "{error}");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "invalid request reached the network"
        );
    }

    #[test]
    fn only_the_three_writing_trade_leaves_are_mutations() {
        let leaves = vec![
            (
                PredictionCommand::Category {
                    command: CategoryCommand::List,
                },
                false,
            ),
            (
                PredictionCommand::Market {
                    command: MarketCommand::List(MarketListArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Market {
                    command: MarketCommand::Detail(MarketDetailArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Market {
                    command: MarketCommand::Search(MarketSearchArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Market {
                    command: MarketCommand::OrderBook(OrderBookArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Market {
                    command: MarketCommand::LastTradePrice(LastTradePriceArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Position {
                    command: PositionCommand::List(PositionListArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Position {
                    command: PositionCommand::Token(PositionTokenArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Position {
                    command: PositionCommand::SettledHistory(SettledHistoryArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Position {
                    command: PositionCommand::Pnl(PnlArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Position {
                    command: PositionCommand::Portfolio,
                },
                false,
            ),
            (
                PredictionCommand::Order {
                    command: OrderCommand::History(OrderHistoryArgs::default()),
                },
                false,
            ),
            (
                PredictionCommand::Trade {
                    command: TradeCommand::Quote(quote_args()),
                },
                false,
            ),
            (
                PredictionCommand::Trade {
                    command: TradeCommand::PlaceOrder(PlaceOrderArgs {
                        quote_id: "quote_abc123".into(),
                        order_type: Some("LIMIT".into()),
                        slippage_bps: "1000".into(),
                        price_limit: Some("0.6".into()),
                    }),
                },
                true,
            ),
            (
                PredictionCommand::Trade {
                    command: TradeCommand::Cancel(CancelArgs {
                        order_ids: "order_123".into(),
                    }),
                },
                true,
            ),
            (
                PredictionCommand::Trade {
                    command: TradeCommand::Redeem(RedeemArgs {
                        token_ids: "token_123".into(),
                        binance_chain_id: None,
                    }),
                },
                true,
            ),
        ];
        assert_eq!(leaves.len(), 16);
        let mut mutations = 0;
        for (command, expected) in leaves {
            assert_eq!(is_mutation(&command), expected, "{command:?}");
            let payload = mutation_payload(&command);
            if !expected {
                assert!(payload.is_err(), "{command:?}");
                continue;
            }
            mutations += 1;
            let payload = payload.unwrap();
            let encoded = payload.to_string();
            assert!(!encoded.contains("session") && !encoded.contains("cookie"));
            assert_eq!(
                payload["operation"],
                match &command {
                    PredictionCommand::Trade {
                        command: TradeCommand::PlaceOrder(_),
                    } => "prediction.place-order",
                    PredictionCommand::Trade {
                        command: TradeCommand::Cancel(_),
                    } => "prediction.cancel",
                    _ => "prediction.redeem",
                }
            );
            assert_eq!(
                payload,
                mutation_payload(&command).unwrap(),
                "payload must be deterministic"
            );
        }
        assert_eq!(mutations, 3);
    }

    #[test]
    fn fixed_point_conversions_match_bundle_helpers() {
        assert_eq!(to_fixed_18("10"), "10000000000000000000");
        assert_eq!(to_fixed_18("0.5"), "500000000000000000");
        assert_eq!(to_fixed_18("0.1234567890123456789"), "123456789012345678");
        assert_eq!(to_fixed_18("1"), "1000000000000000000");
        assert_eq!(decimal_from_fixed("10000000000000000000"), "10");
        assert_eq!(decimal_from_fixed("500000000000000000"), "0.5");
        assert_eq!(decimal_from_fixed("0"), "0");
        assert_eq!(from_fixed_18(None), json!("0"));
        assert_eq!(from_fixed_18(Some(&json!(7))), json!(7));
        assert_eq!(formatted_time(Some(&json!(0))), Value::Null);
        assert_eq!(formatted_time(Some(&json!("1767225600000"))), Value::Null);
    }

    fn quote_args() -> TradeQuoteArgs {
        TradeQuoteArgs {
            binance_chain_id: "56".into(),
            token_id: "123456789".into(),
            market_topic_id: "100".into(),
            side: "BUY".into(),
            amount: "10".into(),
            order_type: "MARKET".into(),
            slippage_bps: Some("1000".into()),
            price_limit: None,
        }
    }
}
