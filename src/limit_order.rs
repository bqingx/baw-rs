//! Limit orders: buy, sell, list, cancel. Wire protocol mirrored from `baw` 1.12.0.
//!
//! Endpoints and payload fields are taken verbatim from the installed bundle
//! `@binance/agentic-wallet/dist/index.js`; each call site quotes the matched
//! string. The JavaScript CLI never converts RWA share amounts when it *places*
//! a limit order (`limitOrder()` forwards `amount` unchanged), so buy/sell pass
//! `--fromTokenQty` through byte for byte. Only *listing* rewrites quantities
//! and trigger prices with the RWA multiplier, exactly like the JS record mapper.
use crate::client::{checksum_address, Client};
use crate::rwa::{self, Registry};
use bigdecimal::BigDecimal;
use clap::Args;
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::str::FromStr;

/// `static PLACE_LIMIT_ORDER_ENDPOINT="/bapi/defi/v1/public/wallet-direct/web-dex/agent/place-limit-order"`
const PLACE_LIMIT_ORDER: &str =
    "/bapi/defi/v1/public/wallet-direct/web-dex/agent/place-limit-order";
/// `static BATCH_QUERY_LIMIT_ORDERS_ENDPOINT="/bapi/defi/v1/public/wallet-direct/web-dex/agent/batch-query-limit-orders"`
const BATCH_QUERY_LIMIT_ORDERS: &str =
    "/bapi/defi/v1/public/wallet-direct/web-dex/agent/batch-query-limit-orders";
/// `static CANCEL_LIMIT_ORDER_ENDPOINT="/bapi/defi/v1/public/wallet-direct/web-dex/agent/cancel-limit-order"`
const CANCEL_LIMIT_ORDER: &str =
    "/bapi/defi/v1/public/wallet-direct/web-dex/agent/cancel-limit-order";

/// `var sr=["PENDING","WORKING","TRIGGERED","FINISHED","FAILED","EXPIRED","CANCELED"]`
const LIMIT_STATUSES: [&str; 7] = [
    "PENDING",
    "WORKING",
    "TRIGGERED",
    "FINISHED",
    "FAILED",
    "EXPIRED",
    "CANCELED",
];
/// `var Wa=new Set(["1","56","8453","CT_501","4663"])`
const CHAINS: [&str; 5] = ["1", "56", "8453", "CT_501", "4663"];
/// `function ir(t,e){return fd.has(e)?t??!0:!1}` with `fd=new Set(["1","56","CT_501"])`
const MEV_CHAINS: [&str; 3] = ["1", "56", "CT_501"];
/// `var Qa=1e18` — ceiling inside `function se(t)`.
const MAX_AMOUNT: &str = "1000000000000000000";
/// `var za=1e15` — ceiling inside `function nn(t)`.
const MAX_TRIGGER_PRICE: &str = "1000000000000000";
/// `var Ga=15778368e5` — 2020-01-01T00:00:00Z in milliseconds.
const MIN_TIME_MS: i64 = 1_577_836_800_000;

const INVALID_DECIMAL: &str = "expected an exact unsigned decimal string";
const INVALID_TRIGGER_PRICE: &str =
    "triggerPrice must be a positive decimal no greater than 1e15 (e.g. $100 or 100)";
const INVALID_SLIPPAGE: &str = "slippage must be auto or a number between 0 and 100";

#[derive(clap::Subcommand, Debug)]
pub enum LimitOrderCommand {
    #[command(about = "Create a limit buy order (buy token at trigger price)")]
    Buy(LimitBuyArgs),
    #[command(about = "Create a limit sell order (sell token at trigger price)")]
    Sell(LimitSellArgs),
    #[command(about = "List and query limit orders with filters")]
    List(LimitListArgs),
    #[command(
        about = "Cancel a pending limit order by its strategy ID. Cannot cancel executed orders"
    )]
    Cancel(LimitCancelArgs),
}

// Options shared by `buy` and `sell`; the JS CLI declares the identical option
// set on both leaves and only differs in the `side` sent on the wire.
#[derive(Clone, Debug, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitCreateArgs {
    /// Trigger price in USD (e.g. $100 or 100)
    #[arg(long = "triggerPrice", value_name = "triggerPrice")]
    pub trigger_price: String,
    /// Amount to spend (`buy`) or amount of tokens to sell (`sell`)
    #[arg(long = "fromTokenQty", value_name = "fromTokenQty")]
    pub from_token_qty: String,
    /// Source token address (`buy`) or token to sell (`sell`)
    #[arg(long = "fromToken", value_name = "fromToken")]
    pub from_token: String,
    /// Token to buy (address) (`buy`) or destination token address (`sell`)
    #[arg(long = "toToken", value_name = "toToken")]
    pub to_token: String,
    /// Slippage tolerance: "auto" (default) or 0-100
    #[arg(long, value_name = "slippage", default_value = "auto")]
    pub slippage: String,
    /// Gas level: LOW, MEDIUM (default), HIGH
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    /// MEV protection: true (default) or false
    #[arg(long, value_name = "mev", default_value = "true")]
    pub mev: String,
    /// Binance chain ID
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: String,
}

#[derive(Debug, Args)]
pub struct LimitBuyArgs {
    #[command(flatten)]
    pub order: LimitCreateArgs,
}

#[derive(Debug, Args)]
pub struct LimitSellArgs {
    #[command(flatten)]
    pub order: LimitCreateArgs,
}

#[derive(Debug, Args)]
pub struct LimitListArgs {
    /// Query specific limit order by strategy ID (ignores other filters)
    #[arg(long = "strategyId", value_name = "id")]
    pub strategy_id: Option<String>,
    /// Filter by binanceChainId
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    /// Filter by order status: PENDING, WORKING, TRIGGERED, FINISHED, FAILED, EXPIRED, or CANCELED
    #[arg(long, value_name = "status")]
    pub status: Option<String>,
    /// Filter by source token contract address
    #[arg(long = "fromToken", value_name = "address")]
    pub from_token: Option<String>,
    /// Filter by target token contract address
    #[arg(long = "toToken", value_name = "address")]
    pub to_token: Option<String>,
    /// Filter orders created after this time (milliseconds timestamp)
    #[arg(long = "startTime", value_name = "timestamp")]
    pub start_time: Option<String>,
    /// Filter orders created before this time (milliseconds timestamp)
    #[arg(long = "endTime", value_name = "timestamp")]
    pub end_time: Option<String>,
    /// Page number for pagination
    #[arg(long, value_name = "page", default_value_t = 1)]
    pub page: u32,
    /// Number of orders per page (max: 100)
    #[arg(long = "pageSize", value_name = "size", default_value_t = 20)]
    pub page_size: u32,
}

#[derive(Debug, Args)]
pub struct LimitCancelArgs {
    /// Strategy ID of the limit order to cancel
    #[arg(long = "strategyId", value_name = "strategyId")]
    pub strategy_id: String,
}

pub fn is_mutation(command: &LimitOrderCommand) -> bool {
    matches!(
        command,
        LimitOrderCommand::Buy(_) | LimitOrderCommand::Sell(_) | LimitOrderCommand::Cancel(_)
    )
}

/// Durable original intent: identifiers and amounts only, never credentials or
/// derived wire values (no slippage fraction, no forced `mev` flag).
pub fn mutation_payload(command: &LimitOrderCommand) -> Result<Value, String> {
    match command {
        LimitOrderCommand::Buy(a) => {
            validate_create(&a.order)?;
            Ok(json!({"operation":"limit-order.buy","args":a.order}))
        }
        LimitOrderCommand::Sell(a) => {
            validate_create(&a.order)?;
            Ok(json!({"operation":"limit-order.sell","args":a.order}))
        }
        LimitOrderCommand::Cancel(a) => {
            let id = cancel_body(a)?;
            Ok(json!({"operation":"limit-order.cancel","args":id}))
        }
        LimitOrderCommand::List(_) => Err("not a mutation".into()),
    }
}

pub async fn run(client: &mut Client, command: &LimitOrderCommand) -> Result<Value, String> {
    match command {
        LimitOrderCommand::Buy(a) => create(client, &a.order, "BUY").await,
        LimitOrderCommand::Sell(a) => create(client, &a.order, "SELL").await,
        LimitOrderCommand::List(a) => list(client, a).await,
        LimitOrderCommand::Cancel(a) => cancel(client, a).await,
    }
}

fn session(c: &Client) -> Result<(), String> {
    if c.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}

fn chain(s: &str) -> Result<(), String> {
    if CHAINS.contains(&s) {
        Ok(())
    } else {
        Err("unsupported binanceChainId".into())
    }
}

fn address(chain_id: &str, s: &str) -> Result<(), String> {
    chain(chain_id)?;
    if chain_id == "CT_501" {
        if (32..=44).contains(&s.len())
            && s.bytes()
                .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
        {
            return Ok(());
        }
    } else if let Some(h) = s.strip_prefix("0x") {
        if h.len() == 40
            && h.bytes().all(|b| b.is_ascii_hexdigit())
            && (h == h.to_ascii_lowercase() || checksum_address(s) == s)
        {
            return Ok(());
        }
    }
    Err("invalid chain-specific address or EIP55 checksum".into())
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `function se(t)`, `He(t)` and `nn(t)` all gate on `/^\d+(\.\d+)?$/`.
fn decimal(s: &str) -> Result<BigDecimal, String> {
    let parts: Vec<&str> = s.split('.').collect();
    if s.is_empty() || s.len() > 512 || parts.len() > 2 || parts.iter().any(|p| !digits(p)) {
        return Err(INVALID_DECIMAL.into());
    }
    BigDecimal::from_str(s).map_err(|_| INVALID_DECIMAL.into())
}

/// `function se(t){...return e>ja&&e<=Qa}` with `ja=0`, `Qa=1e18`.
fn amount(s: &str) -> Result<(), String> {
    let n = decimal(s).map_err(|_| "amount must be greater than zero and at most 1e18")?;
    let max = BigDecimal::from_str(MAX_AMOUNT).map_err(|_| "invalid amount bound")?;
    if n <= 0 || n > max {
        return Err("amount must be greater than zero and at most 1e18".into());
    }
    Ok(())
}

/// `function nn(t){...let e=t.startsWith("$")?t.slice(1):t,n=ve(e,30);...}`
/// — strips one leading `$`, trims, caps at 30 chars, requires a plain decimal
/// in `(0, 1e15]`, and forwards the caller's spelling unchanged.
fn trigger_price(s: &str) -> Result<String, String> {
    let v = s.strip_prefix('$').unwrap_or(s).trim();
    if v.len() > 30 {
        return Err(INVALID_TRIGGER_PRICE.into());
    }
    let n = decimal(v).map_err(|_| INVALID_TRIGGER_PRICE.to_string())?;
    let max = BigDecimal::from_str(MAX_TRIGGER_PRICE).map_err(|_| "invalid price bound")?;
    if n <= 0 || n > max {
        return Err(INVALID_TRIGGER_PRICE.into());
    }
    Ok(v.to_string())
}

/// `function He(t){if(t===void 0||t==="auto")return!0;...e>=0&&e<=100}` and
/// `e.slippage===void 0||e.slippage==="auto"?n="auto":n=((parseFloat(s))/100).toString()`
fn slippage(s: &str) -> Result<String, String> {
    if s == "auto" {
        return Ok("auto".into());
    }
    let n = decimal(s).map_err(|_| INVALID_SLIPPAGE.to_string())?;
    if n > 100 {
        return Err(INVALID_SLIPPAGE.into());
    }
    Ok((n / BigDecimal::from(100)).normalized().to_plain_string())
}

fn gas(s: &str) -> Result<String, String> {
    let v = s.to_ascii_uppercase();
    if matches!(v.as_str(), "LOW" | "MEDIUM" | "HIGH") {
        Ok(v)
    } else {
        Err("gasLevel must be LOW, MEDIUM, or HIGH".into())
    }
}

/// `function ir(t,e){return fd.has(e)?t??!0:!1}`: `--mev` defaults to true, and
/// outside chains 1/56/CT_501 the JS CLI forces `false` regardless of the flag.
fn mev(flag: &str, chain_id: &str) -> Result<bool, String> {
    let requested = match flag {
        "true" => true,
        "false" => false,
        _ => return Err("mev must be true or false".into()),
    };
    Ok(requested && MEV_CHAINS.contains(&chain_id))
}

fn validate_create(a: &LimitCreateArgs) -> Result<(), String> {
    chain(&a.binance_chain_id)?;
    address(&a.binance_chain_id, &a.from_token)?;
    address(&a.binance_chain_id, &a.to_token)?;
    amount(&a.from_token_qty)?;
    trigger_price(&a.trigger_price)?;
    slippage(&a.slippage)?;
    gas(&a.gas_level)?;
    mev(&a.mev, &a.binance_chain_id)?;
    Ok(())
}

/// `function Yt(t){...let e=ve(t,so);return!e||!qa.test(e)?null:e}` with
/// `so=64`, `qa=/^[a-zA-Z0-9-]+$/` — the strategyId gate used by list and cancel.
fn strategy_ref(s: &str) -> Result<&str, String> {
    let v = s.trim();
    if v.is_empty() || v.len() > 64 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("invalid strategyId format; must be alphanumeric with optional hyphens".into());
    }
    Ok(v)
}

/// `this.tradeApi.placeLimitOrder({binanceChainId:e.binanceChainId,fromToken:e.fromToken,toToken:e.toToken,amount:e.amount,slippage:n,side:e.side,triggerPrice:e.triggerPrice,mev:ir(e.mev,e.binanceChainId),gasLevel:e.gasLevel})`
fn create_body(a: &LimitCreateArgs, side: &str) -> Result<Value, String> {
    validate_create(a)?;
    Ok(json!({
        "binanceChainId": a.binance_chain_id,
        "fromToken": a.from_token,
        "toToken": a.to_token,
        "amount": a.from_token_qty,
        "slippage": slippage(&a.slippage)?,
        "side": side,
        "triggerPrice": trigger_price(&a.trigger_price)?,
        "mev": mev(&a.mev, &a.binance_chain_id)?,
        "gasLevel": gas(&a.gas_level)?,
    }))
}

/// `this.tradeApi.cancelLimitOrder({strategyId:n})` with `n=parseInt(e,10)`.
fn cancel_body(a: &LimitCancelArgs) -> Result<Value, String> {
    let id = strategy_ref(&a.strategy_id)?;
    let number = id
        .parse::<u64>()
        .map_err(|_| "strategyId must be a number".to_string())?;
    Ok(json!({"strategyId": number}))
}

/// The JS mapper raises `LIMIT_ORDER_API_ERROR` with the backend message
/// interpolated. This port keeps the code/orderId/orderExpireTime envelope and
/// replaces the free-form text with a static, sanitized string.
fn api_code_error(data: &Value) -> Option<String> {
    let code = data
        .get("code")
        .filter(|c| !c.is_null() && c.as_str() != Some("") && c.as_str() != Some("000000"))?;
    let safe_code = code
        .as_str()
        .filter(|s| s.len() <= 32 && s.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("UNKNOWN");
    let mut out = Map::new();
    out.insert("error".into(), json!("LIMIT_ORDER_API_ERROR"));
    out.insert("code".into(), json!(safe_code));
    out.insert(
        "message".into(),
        json!("limit order rejected; reconcile open orders before resubmitting"),
    );
    if let Some(order_id) = data["orderId"].as_str().filter(|s| !s.is_empty()) {
        out.insert("orderId".into(), json!(order_id));
    }
    if let Some(expire) = data.get("orderExpireTime").filter(|v| !v.is_null()) {
        out.insert("orderExpireTime".into(), expire.clone());
    }
    Some(Value::Object(out).to_string())
}

/// `return{strategyId:r.data.strategyId.toString(),clientStrategyId:r.data.clientStrategyId}`
fn strategy_id(data: &Value) -> Result<String, String> {
    match &data["strategyId"] {
        Value::String(s) if !s.trim().is_empty() => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err("limit order response is missing strategyId".into()),
    }
}

async fn create(client: &mut Client, a: &LimitCreateArgs, side: &str) -> Result<Value, String> {
    let body = create_body(a, side)?;
    session(client)?;
    let data = client
        .request(Method::POST, PLACE_LIMIT_ORDER, Some(body))
        .await?;
    if let Some(error) = api_code_error(&data) {
        return Err(error);
    }
    // `p()?g({strategyId:v.strategyId})` — the JS `--json` output for this leaf.
    Ok(json!({"strategyId": strategy_id(&data)?}))
}

async fn cancel(client: &mut Client, a: &LimitCancelArgs) -> Result<Value, String> {
    let body = cancel_body(a)?;
    session(client)?;
    let data = client
        .request(Method::POST, CANCEL_LIMIT_ORDER, Some(body))
        .await?;
    if let Some(error) = api_code_error(&data) {
        return Err(error);
    }
    // `return await this.tradeApi.cancelLimitOrder({strategyId:n}),{strategyId:e,status:"CANCELED"}`
    Ok(json!({"strategyId": a.strategy_id, "status": "CANCELED"}))
}

enum ListQuery {
    /// `if(isNaN(a))return{total:0,page:1,pageSize:1,list:[]}` — no request at all.
    Empty,
    Strategy(i64),
    Filter(Value),
}

fn timestamp(s: &str) -> Result<i64, String> {
    let v = s.trim();
    if !digits(v) || v.len() > 20 {
        return Err("time must be an integer millisecond timestamp".into());
    }
    let n = v
        .parse::<i64>()
        .map_err(|_| "time must be an integer millisecond timestamp".to_string())?;
    if n < MIN_TIME_MS
        || n > chrono::Utc::now().timestamp_millis() + 100 * 365 * 24 * 60 * 60 * 1000
    {
        return Err("time out of range (2020 through now + 100 years)".into());
    }
    Ok(n)
}

/// Filter tokens are chain-checked against `--binanceChainId` when given; without
/// it the address family decides, the same way the market-order list does.
fn token_chain<'a>(a: &'a LimitListArgs, token: &'a str) -> &'a str {
    a.binance_chain_id
        .as_deref()
        .unwrap_or(if token.starts_with("0x") {
            "1"
        } else {
            "CT_501"
        })
}

/// `this.tradeApi.batchQueryLimitOrders({binanceChainId:s,statusList:i,fromTokenAddress:e.fromToken,toTokenAddress:e.toToken,startTime:e.startTime,endTime:e.endTime,page:r,pageSize:o,sort:"DESC"})`
/// and the strategy form `batchQueryLimitOrders({strategyId:a,page:1,pageSize:1})`.
fn list_query(a: &LimitListArgs) -> Result<ListQuery, String> {
    if let Some(id) = &a.strategy_id {
        let id = strategy_ref(id)?;
        return Ok(match id.parse::<i64>() {
            Ok(number) => ListQuery::Strategy(number),
            Err(_) => ListQuery::Empty,
        });
    }
    if !(1..=10_000).contains(&a.page) || !(1..=100).contains(&a.page_size) {
        return Err("page must be 1..10000, pageSize 1..100".into());
    }
    if let Some(status) = &a.status {
        if !LIMIT_STATUSES.contains(&status.as_str()) {
            return Err(
                "status must be PENDING, WORKING, TRIGGERED, FINISHED, FAILED, EXPIRED, or CANCELED"
                    .into(),
            );
        }
    }
    if let Some(chain_id) = &a.binance_chain_id {
        chain(chain_id)?;
    }
    let mut body = Map::new();
    if let Some(chain_id) = &a.binance_chain_id {
        body.insert("binanceChainId".into(), json!(chain_id));
    }
    if let Some(status) = &a.status {
        body.insert("statusList".into(), json!([status]));
    }
    for (key, token) in [
        ("fromTokenAddress", &a.from_token),
        ("toTokenAddress", &a.to_token),
    ] {
        if let Some(token) = token {
            address(token_chain(a, token), token)?;
            body.insert(key.into(), json!(token));
        }
    }
    let start = a.start_time.as_deref().map(timestamp).transpose()?;
    let end = a.end_time.as_deref().map(timestamp).transpose()?;
    if start.zip(end).is_some_and(|(s, e)| s > e) {
        return Err("startTime exceeds endTime".into());
    }
    if let Some(start) = start {
        body.insert("startTime".into(), json!(start));
    }
    if let Some(end) = end {
        body.insert("endTime".into(), json!(end));
    }
    body.insert("page".into(), json!(a.page));
    body.insert("pageSize".into(), json!(a.page_size));
    body.insert("sort".into(), json!("DESC"));
    Ok(ListQuery::Filter(Value::Object(body)))
}

async fn list(client: &mut Client, a: &LimitListArgs) -> Result<Value, String> {
    let query = list_query(a)?;
    session(client)?;
    let (body, page, page_size) = match query {
        ListQuery::Empty => {
            return Ok(json!({"total":0,"page":1,"pageSize":1,"list":[]}));
        }
        ListQuery::Strategy(id) => (json!({"strategyId": id, "page": 1, "pageSize": 1}), 1, 1),
        ListQuery::Filter(body) => (body, a.page, a.page_size),
    };
    let registry = rwa::fetch(client).await?;
    let data = client
        .request(Method::POST, BATCH_QUERY_LIMIT_ORDERS, Some(body))
        .await?;
    if let Some(error) = api_code_error(&data) {
        return Err(error);
    }
    let rows = data["rows"].as_array().ok_or("invalid limit order rows")?;
    let orders = rows
        .iter()
        .map(|row| order_record(row, &registry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"total": data["total"], "page": page, "pageSize": page_size, "list": orders}))
}

fn nonempty<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row[key].as_str().filter(|s| !s.is_empty())
}

fn str_field<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

fn decimal_text(v: &Value) -> Result<String, String> {
    if let Some(s) = v.as_str() {
        Ok(s.to_string())
    } else if v.is_number() {
        Ok(v.to_string())
    } else {
        Err("invalid backend decimal value".into())
    }
}

/// `function X(t){...return`${a}-${l}-${c}T${u}:${h}:${y}${i}`}` — local time,
/// second precision, numeric offset.
fn date(v: &Value) -> Value {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .filter(|n| *n != 0)
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|t| {
            json!(t
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%dT%H:%M:%S%:z")
                .to_string())
        })
        .unwrap_or(Value::Null)
}

fn refunds(row: &Value, registry: &Registry, chain_id: &str) -> Result<Value, String> {
    let Some(extend) = row["extend"]
        .as_str()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
    else {
        return Ok(Value::Null);
    };
    let Some(items) = extend["refund"].as_array().filter(|rows| !rows.is_empty()) else {
        return Ok(Value::Null);
    };
    let mut out = Vec::new();
    for item in items {
        let token = str_field(item, "tokenAddress");
        let multiplier = registry
            .token(chain_id, token)
            .map(|t| t.multiplier.as_str());
        let mut entry = Map::new();
        if let Some(value) = item.get("tokenAddress") {
            entry.insert("tokenAddress".into(), value.clone());
        }
        match (multiplier, item["amount"].as_str()) {
            (Some(m), Some(amount)) if !amount.is_empty() => {
                entry.insert("amount".into(), json!(rwa::multiply_down(amount, m, 18)?));
            }
            _ => {
                if let Some(value) = item.get("amount") {
                    entry.insert("amount".into(), value.clone());
                }
            }
        }
        if let Some(value) = item.get("symbol") {
            entry.insert("symbol".into(), value.clone());
        }
        out.push(Value::Object(entry));
    }
    Ok(json!(out))
}

/// `mapToOrderRecord(e,"limit")` — key order and the `|| ""` defaults follow the
/// JS object literal so `--json` output matches field for field.
fn order_record(row: &Value, registry: &Registry) -> Result<Value, String> {
    let chain_id = nonempty(row, "walletChainId")
        .or_else(|| nonempty(row, "fromTokenChainId"))
        .unwrap_or("");
    let from_chain = nonempty(row, "fromTokenChainId").unwrap_or(chain_id);
    let to_chain = nonempty(row, "toTokenChainId").unwrap_or(chain_id);
    let from = str_field(row, "fromTokenAddress");
    let to = str_field(row, "toTokenAddress");
    let from_multiplier = registry
        .token(from_chain, from)
        .map(|t| t.multiplier.as_str());
    let to_multiplier = registry.token(to_chain, to).map(|t| t.multiplier.as_str());
    let price_multiplier = from_multiplier.or(to_multiplier);

    let mut out = Map::new();
    out.insert("orderType".into(), json!("limit"));
    out.insert(
        "orderId".into(),
        json!(nonempty(row, "orderId")
            .or_else(|| nonempty(row, "id"))
            .unwrap_or("")),
    );
    if let Some(value) = row.get("strategyId").filter(|v| !v.is_null()) {
        out.insert("strategyId".into(), value.clone());
    }
    out.insert("chain".into(), json!(chain_id));
    if let Some(value) = nonempty(row, "side") {
        out.insert("side".into(), json!(value));
    }
    out.insert("fromToken".into(), json!(checksum_address(from)));
    out.insert(
        "fromTokenName".into(),
        json!(nonempty(row, "fromTokenName").unwrap_or("")),
    );
    let quantity = str_field(row, "fromTokenQty");
    out.insert(
        "fromTokenQty".into(),
        match from_multiplier {
            Some(m) if !quantity.is_empty() => json!(rwa::multiply_down(quantity, m, 18)?),
            _ => json!(quantity),
        },
    );
    out.insert("toToken".into(), json!(checksum_address(to)));
    out.insert(
        "toTokenName".into(),
        json!(nonempty(row, "toTokenName").unwrap_or("")),
    );
    if let Some(value) = row.get("toTokenActualQty") {
        out.insert(
            "toTokenActualQty".into(),
            match (to_multiplier, value.as_str()) {
                (Some(m), Some(actual)) if !actual.is_empty() => {
                    json!(rwa::multiply_down(actual, m, 18)?)
                }
                _ => value.clone(),
            },
        );
    }
    if let Some(price) = row.get("price").filter(|v| truthy(v)) {
        out.insert(
            "triggerPrice".into(),
            match price_multiplier {
                Some(m) => json!(rwa::divide(&decimal_text(price)?, m, 18, false)?),
                None => price.clone(),
            },
        );
    }
    out.insert(
        "status".into(),
        json!(nonempty(row, "status").unwrap_or("")),
    );
    for (source, target) in [("slippage", "slippage"), ("orderTxId", "txHash")] {
        if let Some(value) = row.get(source) {
            out.insert(target.into(), value.clone());
        }
    }
    out.insert("bookTime".into(), date(&row["bookTime"]));
    out.insert("updatedTime".into(), date(&row["updatedTime"]));
    let refund = refunds(row, registry, chain_id)?;
    if !refund.is_null() {
        out.insert("refund".into(), refund);
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::Receiver;

    fn creds() -> crate::session::Credentials {
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    fn server(
        data: Vec<Value>,
    ) -> (
        Client,
        Receiver<(String, Value)>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            for reply in data {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|n| n.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                let (head, body) = request.split_once("\r\n\r\n").unwrap();
                tx.send((
                    head.lines().next().unwrap().to_string(),
                    if body.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str(body).unwrap()
                    },
                ))
                .unwrap();
                let body = match reply.get("__envelope_code") {
                    Some(code) => {
                        json!({"code":code,"message":"synthetic-secret-message","data":null})
                    }
                    None => json!({"code":"000000","data":reply}),
                }
                .to_string();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });
        (Client::test_client(creds(), &origin).unwrap(), rx, handle)
    }

    /// Any accepted connection is a failure: these leaves must fail before HTTP.
    fn silent() -> (Client, TcpListener) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        (Client::test_client(creds(), &origin).unwrap(), listener)
    }

    fn create_args() -> LimitCreateArgs {
        LimitCreateArgs {
            trigger_price: "$100".into(),
            from_token_qty: "10.10".into(),
            from_token: "0x1111111111111111111111111111111111111111".into(),
            to_token: "0x2222222222222222222222222222222222222222".into(),
            slippage: "1".into(),
            gas_level: "HIGH".into(),
            mev: "true".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn list_args() -> LimitListArgs {
        LimitListArgs {
            strategy_id: None,
            binance_chain_id: None,
            status: None,
            from_token: None,
            to_token: None,
            start_time: None,
            end_time: None,
            page: 1,
            page_size: 20,
        }
    }

    #[tokio::test]
    async fn buy_posts_limit_order_with_js_field_names_and_stripped_price() {
        let (mut client, requests, handle) = server(vec![json!({"strategyId":1856})]);
        let out = run(
            &mut client,
            &LimitOrderCommand::Buy(LimitBuyArgs {
                order: create_args(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(out, json!({"strategyId":"1856"}));
        let (line, body) = requests.recv().unwrap();
        assert_eq!(line, format!("POST {PLACE_LIMIT_ORDER} HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "fromToken":"0x1111111111111111111111111111111111111111",
                "toToken":"0x2222222222222222222222222222222222222222",
                "amount":"10.10",
                "slippage":"0.01",
                "side":"BUY",
                "triggerPrice":"100",
                "mev":true,
                "gasLevel":"HIGH"
            })
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn sell_uses_sell_side_and_forces_mev_off_on_unsupported_chains() {
        let mut args = create_args();
        args.binance_chain_id = "8453".into();
        args.slippage = "auto".into();
        args.gas_level = "medium".into();
        args.trigger_price = "0.000000000000000001".into();
        let (mut client, requests, handle) = server(vec![json!({"strategyId":"s-2"})]);
        let out = run(
            &mut client,
            &LimitOrderCommand::Sell(LimitSellArgs { order: args }),
        )
        .await
        .unwrap();
        assert_eq!(out["strategyId"], "s-2");
        let (line, body) = requests.recv().unwrap();
        assert!(line.starts_with(&format!("POST {PLACE_LIMIT_ORDER} ")));
        assert_eq!(body["side"], "SELL");
        assert_eq!(body["slippage"], "auto");
        assert_eq!(body["gasLevel"], "MEDIUM");
        assert_eq!(body["mev"], false);
        assert_eq!(body["triggerPrice"], "0.000000000000000001");
        assert_eq!(body["amount"], "10.10");
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn cancel_posts_numeric_strategy_id_and_reports_canceled() {
        let (mut client, requests, handle) = server(vec![json!({})]);
        let out = run(
            &mut client,
            &LimitOrderCommand::Cancel(LimitCancelArgs {
                strategy_id: "9876543210".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(out, json!({"strategyId":"9876543210","status":"CANCELED"}));
        let (line, body) = requests.recv().unwrap();
        assert!(line.starts_with(&format!("POST {CANCEL_LIMIT_ORDER} ")));
        assert_eq!(body, json!({"strategyId":9876543210_u64}));
        assert!(body["strategyId"].is_u64());
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn list_filters_routes_through_registry_and_maps_limit_fields() {
        let mut args = list_args();
        args.binance_chain_id = Some("56".into());
        args.status = Some("WORKING".into());
        args.start_time = Some("1577836800000".into());
        args.page = 2;
        args.page_size = 5;
        let registry = json!([
            {"chainId":"56","contractAddress":"0x1111111111111111111111111111111111111111","multiplier":"3","type":1}
        ]);
        let row = json!({
            "id":"order-1",
            "strategyId":42,
            "walletChainId":"56",
            "side":"BUY",
            "fromTokenAddress":"0x1111111111111111111111111111111111111111",
            "fromTokenName":"USDT",
            "fromTokenQty":"2",
            "toTokenAddress":"0x2222222222222222222222222222222222222222",
            "toTokenName":"RWA",
            "toTokenActualQty":"0.1",
            "price":"9",
            "status":"WORKING",
            "slippage":"0.01",
            "orderTxId":"tx-1",
            "bookTime":1577836800000_i64,
            "updatedTime":1577836801000_i64,
            "extend":json!({"refund":[{"tokenAddress":"0x1111111111111111111111111111111111111111","amount":"0.25","symbol":"USDT"}]}).to_string()
        });
        let (mut client, requests, handle) =
            server(vec![registry, json!({"total":1,"rows":[row]})]);
        let out = run(&mut client, &LimitOrderCommand::List(args))
            .await
            .unwrap();
        assert_eq!(out["total"], 1);
        assert_eq!(out["page"], 2);
        assert_eq!(out["pageSize"], 5);
        let record = &out["list"][0];
        assert_eq!(record["orderType"], "limit");
        assert_eq!(record["orderId"], "order-1");
        assert_eq!(record["strategyId"], 42);
        assert_eq!(record["chain"], "56");
        assert_eq!(record["side"], "BUY");
        assert_eq!(record["fromTokenQty"], "6");
        assert_eq!(record["toTokenActualQty"], "0.1");
        assert_eq!(record["triggerPrice"], "3");
        assert_eq!(record["txHash"], "tx-1");
        assert_eq!(record["slippage"], "0.01");
        assert_eq!(record["refund"][0]["amount"], "0.75");
        assert_eq!(record["refund"][0]["symbol"], "USDT");
        assert!(record["bookTime"]
            .as_str()
            .unwrap()
            .starts_with("2020-01-01T"));
        let (registry_line, _) = requests.recv().unwrap();
        assert!(registry_line.starts_with(
            "GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list "
        ));
        let (line, body) = requests.recv().unwrap();
        assert!(line.starts_with(&format!("POST {BATCH_QUERY_LIMIT_ORDERS} ")));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "statusList":["WORKING"],
                "startTime":1577836800000_i64,
                "page":2,
                "pageSize":5,
                "sort":"DESC"
            })
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn list_by_strategy_asks_for_one_row_and_non_numeric_never_calls_out() {
        let mut args = list_args();
        args.strategy_id = Some("9876543210".into());
        let (mut client, requests, handle) = server(vec![json!([]), json!({"total":0,"rows":[]})]);
        let out = run(&mut client, &LimitOrderCommand::List(args))
            .await
            .unwrap();
        assert_eq!(out, json!({"total":0,"page":1,"pageSize":1,"list":[]}));
        requests.recv().unwrap();
        let (line, body) = requests.recv().unwrap();
        assert!(line.starts_with(&format!("POST {BATCH_QUERY_LIMIT_ORDERS} ")));
        assert_eq!(
            body,
            json!({"strategyId":9876543210_i64,"page":1,"pageSize":1})
        );
        handle.join().unwrap();

        let mut args = list_args();
        args.strategy_id = Some("client-order-1".into());
        let (mut client, listener) = silent();
        let out = run(&mut client, &LimitOrderCommand::List(args))
            .await
            .unwrap();
        assert_eq!(out, json!({"total":0,"page":1,"pageSize":1,"list":[]}));
        assert!(listener.accept().is_err());
    }

    #[test]
    fn every_writing_leaf_is_a_mutation_and_list_is_not() {
        let buy = LimitOrderCommand::Buy(LimitBuyArgs {
            order: create_args(),
        });
        let sell = LimitOrderCommand::Sell(LimitSellArgs {
            order: create_args(),
        });
        let cancel = LimitOrderCommand::Cancel(LimitCancelArgs {
            strategy_id: "42".into(),
        });
        let list = LimitOrderCommand::List(list_args());
        for command in [&buy, &sell, &cancel] {
            assert!(is_mutation(command));
            assert!(mutation_payload(command).is_ok());
        }
        assert!(!is_mutation(&list));
        assert!(mutation_payload(&list).is_err());
        assert_eq!(
            mutation_payload(&buy).unwrap()["operation"],
            "limit-order.buy"
        );
        assert_eq!(
            mutation_payload(&sell).unwrap()["operation"],
            "limit-order.sell"
        );
        let intent = mutation_payload(&cancel).unwrap();
        assert_eq!(intent["operation"], "limit-order.cancel");
        assert_eq!(intent["args"]["strategyId"], 42);
        // The journal must never carry a session cookie or a derived wire value.
        for command in [&buy, &sell, &cancel] {
            let text = mutation_payload(command).unwrap().to_string();
            assert!(!text.contains("synthetic-session"));
            assert!(!text.contains(r#""slippage":"0.01""#));
        }
    }

    #[tokio::test]
    async fn data_level_code_becomes_sanitized_limit_order_error() {
        let (mut client, requests, handle) = server(vec![json!({
            "code":"12345",
            "message":"synthetic-secret-message",
            "orderId":"recover-1",
            "orderExpireTime":1700000000000_i64
        })]);
        let error = run(
            &mut client,
            &LimitOrderCommand::Buy(LimitBuyArgs {
                order: create_args(),
            }),
        )
        .await
        .unwrap_err();
        assert!(error.contains("LIMIT_ORDER_API_ERROR"), "{error}");
        assert!(error.contains("12345") && error.contains("recover-1"));
        assert!(!error.contains("synthetic-secret-message"));
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn envelope_failure_is_reported_without_backend_message() {
        let (mut client, requests, handle) = server(vec![json!({"__envelope_code":"123456"})]);
        let error = run(
            &mut client,
            &LimitOrderCommand::Cancel(LimitCancelArgs {
                strategy_id: "42".into(),
            }),
        )
        .await
        .unwrap_err();
        assert!(!error.contains("synthetic-secret-message"), "{error}");
        assert!(error.contains("123456"), "{error}");
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn invalid_required_arguments_never_reach_the_network() {
        let mut bad_price = create_args();
        bad_price.trigger_price = "$0".into();
        let mut bad_amount = create_args();
        bad_amount.from_token_qty = "1e2".into();
        let mut bad_mev = create_args();
        bad_mev.mev = "yes".into();
        let mut bad_slippage = create_args();
        bad_slippage.slippage = "101".into();
        let mut bad_gas = create_args();
        bad_gas.gas_level = "turbo".into();
        let mut bad_chain = create_args();
        bad_chain.binance_chain_id = "137".into();
        let mut bad_from = create_args();
        bad_from.from_token = "0x52908400098527886e0F7030069857D2E4169EE7".into();
        for args in [
            bad_price,
            bad_amount,
            bad_mev,
            bad_slippage,
            bad_gas,
            bad_chain,
            bad_from,
        ] {
            let (mut client, listener) = silent();
            let error = run(
                &mut client,
                &LimitOrderCommand::Buy(LimitBuyArgs {
                    order: args.clone(),
                }),
            )
            .await
            .unwrap_err();
            assert!(!error.is_empty());
            assert!(listener.accept().is_err(), "invalid args reached HTTP");
            assert!(
                mutation_payload(&LimitOrderCommand::Buy(LimitBuyArgs { order: args })).is_err()
            );
        }
        for args in [
            LimitListArgs {
                status: Some("SUBMITTED".into()),
                ..list_args()
            },
            LimitListArgs {
                page: 0,
                ..list_args()
            },
            LimitListArgs {
                page_size: 101,
                ..list_args()
            },
            LimitListArgs {
                strategy_id: Some("../../etc".into()),
                ..list_args()
            },
            LimitListArgs {
                end_time: Some("1577836800000junk".into()),
                ..list_args()
            },
        ] {
            let (mut client, listener) = silent();
            assert!(run(&mut client, &LimitOrderCommand::List(args))
                .await
                .is_err());
            assert!(listener.accept().is_err());
        }
        let (mut client, listener) = silent();
        assert!(run(
            &mut client,
            &LimitOrderCommand::Cancel(LimitCancelArgs {
                strategy_id: "not-a-number".into(),
            }),
        )
        .await
        .is_err());
        assert!(listener.accept().is_err());
    }

    #[tokio::test]
    async fn missing_session_fails_before_any_write() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(
            crate::session::Credentials {
                client_id: zeroize::Zeroizing::new("synthetic-client".into()),
                session_id: zeroize::Zeroizing::new(String::new()),
            },
            &origin,
        )
        .unwrap();
        let error = run(
            &mut client,
            &LimitOrderCommand::Cancel(LimitCancelArgs {
                strategy_id: "42".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "wallet session is missing");
        assert!(listener.accept().is_err());
    }

    #[derive(clap::Parser)]
    struct Parser {
        #[command(subcommand)]
        command: LimitOrderCommand,
    }

    #[test]
    fn clap_surface_matches_the_js_leaf_options_and_defaults() {
        use clap::Parser as _;
        let parsed = Parser::try_parse_from([
            "x",
            "buy",
            "--triggerPrice",
            "$100",
            "--fromTokenQty",
            "10.1",
            "--fromToken",
            "0x1111111111111111111111111111111111111111",
            "--toToken",
            "0x2222222222222222222222222222222222222222",
            "--binanceChainId",
            "56",
        ])
        .unwrap();
        let LimitOrderCommand::Buy(args) = parsed.command else {
            unreachable!("parsed a buy leaf")
        };
        assert_eq!(args.order.slippage, "auto");
        assert_eq!(args.order.gas_level, "MEDIUM");
        assert_eq!(args.order.mev, "true");
        let missing = Parser::try_parse_from(["x", "sell", "--binanceChainId", "56"]);
        assert!(missing.is_err());
        let list = Parser::try_parse_from(["x", "list"]).unwrap();
        let LimitOrderCommand::List(args) = list.command else {
            unreachable!("parsed a list leaf")
        };
        assert_eq!((args.page, args.page_size), (1, 20));
        assert!(Parser::try_parse_from(["x", "cancel"]).is_err());
    }

    #[test]
    fn validation_matches_js_regexes_and_bounds() {
        for value in ["", "0", "-1", "+1", ".5", "1.", "1e18", "1.2.3"] {
            assert!(amount(value).is_err(), "{value}");
        }
        for value in ["1", "0001.00", "1000000000000000000"] {
            assert!(amount(value).is_ok(), "{value}");
        }
        assert!(amount("1000000000000000000.000000000000000001").is_err());
        assert_eq!(trigger_price("$100").unwrap(), "100");
        assert_eq!(trigger_price(" 100.50 ").unwrap(), "100.50");
        assert_eq!(
            trigger_price("1000000000000000").unwrap(),
            "1000000000000000"
        );
        for value in ["$0", "-100", "1000000000000001", "1e5"] {
            assert!(trigger_price(value).is_err(), "{value}");
        }
        assert_eq!(slippage("auto").unwrap(), "auto");
        assert_eq!(slippage("0").unwrap(), "0");
        assert_eq!(slippage("1").unwrap(), "0.01");
        assert_eq!(slippage("100").unwrap(), "1");
        assert_eq!(slippage("30").unwrap(), "0.3");
        assert!(slippage("100.000000000000000001").is_err());
        assert!(slippage("1junk").is_err());
        assert!(mev("true", "56").unwrap());
        assert!(!mev("false", "56").unwrap());
        assert!(!mev("true", "8453").unwrap());
        assert!(mev("TRUE", "56").is_err());
        assert!(strategy_ref("42").is_ok());
        assert!(strategy_ref("a-b-1").is_ok());
        assert!(strategy_ref("").is_err());
        assert!(strategy_ref("4 2").is_err());
        assert!(strategy_ref(&"9".repeat(65)).is_err());
        assert!(address("CT_501", "So11111111111111111111111111111111111111112").is_ok());
        assert!(address("56", "So11111111111111111111111111111111111111112").is_err());
        assert!(address("1", "0x52908400098527886E0F7030069857D2E4169EE7").is_ok());
        assert!(address("1", "0x52908400098527886e0F7030069857D2E4169EE7").is_err());
    }

    #[test]
    fn timestamps_are_2020_bounded_and_ordered() {
        assert!(timestamp("1577836800000").is_ok());
        assert!(timestamp("1577836799999").is_err());
        assert!(timestamp("1577836800000junk").is_err());
        assert!(timestamp(&"9".repeat(21)).is_err());
        let mut args = list_args();
        args.start_time = Some("1577836800000".into());
        args.end_time = Some("1577836799999".into());
        assert!(list_query(&args).is_err());
    }
}
