//! Native market trades and transfers; mutation authorization/journaling is parent-owned.
use crate::client::{checksum_address, Client};
use crate::rwa::{self, Registry};
use bigdecimal::BigDecimal;
use clap::{Args, Subcommand};
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};
use std::str::FromStr;
const A: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet";
const D: &str = "/bapi/defi/v1/public/wallet-direct/web-dex/agent";

fn session(c: &Client) -> Result<(), String> {
    if c.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}
pub async fn run(c: &mut Client, command: &MarketCommand) -> Result<Value, String> {
    match command {
        MarketCommand::Quote(a) => {
            validate_quote(a)?;
            session(c)?;
            let r = rwa::fetch(c).await?;
            quote(c, a, &r).await
        }
        MarketCommand::Swap(a) => {
            validate_swap(a)?;
            session(c)?;
            let r = rwa::fetch(c).await?;
            swap(c, a, &r).await
        }
        MarketCommand::List(a) => {
            let body = list_body(a)?;
            session(c)?;
            let r = rwa::fetch(c).await?;
            let data = c
                .request(
                    Method::POST,
                    &format!("{D}/batch-query-market-orders"),
                    Some(body),
                )
                .await?;
            let rows = data["rows"].as_array().ok_or("invalid market order rows")?;
            let list = rows
                .iter()
                .map(|row| map_order(row, &r))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(
                json!({"total":data["total"],"page":if a.order_id.is_some(){1}else{a.page},"pageSize":if a.order_id.is_some(){1}else{a.page_size},"list":list}),
            )
        }
    }
}
async fn quote(c: &mut Client, a: &QuoteArgs, r: &Registry) -> Result<Value, String> {
    let from = r.token(&a.binance_chain_id, &a.from_token);
    let to = r.token(&a.binance_chain_id, &a.to_token);
    let token = from.or(to);
    let sell = from.is_some();
    let ondo = token.is_some_and(|t| t.kind == "ondo");
    let qty = if let Some(t) = from {
        rwa::divide_up(&a.from_token_qty, &t.multiplier, 18)?
    } else {
        a.from_token_qty.clone()
    };
    let mut body = json!({"binanceChainId":a.binance_chain_id,"fromToken":a.from_token,"toToken":a.to_token,"slippage":slippage(&a.slippage)?});
    body[if ondo { "fromAmount" } else { "amount" }] = json!(qty);
    if token.is_some() {
        body["fromTokenShare"] = if sell {
            json!(a.from_token_qty)
        } else {
            Value::Null
        };
        body["fromMultiplier"] = from.map(|t| json!(t.multiplier)).unwrap_or(Value::Null);
    }
    let endpoint = if ondo { "ondo/quote" } else { "quote" };
    let mut out = c
        .request(Method::POST, &format!("{D}/{endpoint}"), Some(body))
        .await?;
    if !out.is_object() {
        return Err("invalid quote response".into());
    }
    if out["quoteId"].as_str().unwrap_or("").is_empty() {
        out["quoteId"] = out.get("uniQuoteId").cloned().unwrap_or(json!(""));
    }
    for key in [
        "fromCoinSymbol",
        "fromCoinAmount",
        "toCoinSymbol",
        "toCoinAmount",
    ] {
        if out.get(key).is_none_or(|v| v.is_null()) {
            out[key] = json!("");
        }
    }
    if sell {
        out["fromCoinAmount"] = json!(a.from_token_qty);
    } else if let Some(t) = to {
        if ondo && out["toTokenShare"].as_str().is_some_and(|s| !s.is_empty()) {
            out["toCoinAmount"] = out["toTokenShare"].clone();
        } else if let Some(qty) = out["toCoinAmount"].as_str().filter(|s| !s.is_empty()) {
            let m = if ondo {
                out["toMultiplier"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&t.multiplier)
            } else {
                &t.multiplier
            };
            out["toCoinAmount"] = json!(rwa::multiply_down(qty, m, 18)?);
        }
    }
    Ok(out)
}

fn native_token(chain: &str, token: &str) -> bool {
    if chain == "CT_501" {
        token == "So11111111111111111111111111111111111111112"
    } else {
        matches!(
            token.to_ascii_lowercase().as_str(),
            "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                | "0x0000000000000000000000000000000000000000"
        )
    }
}
async fn safeguards(c: &mut Client, chain: &str) -> Result<(), String> {
    let settings = c
        .request(Method::GET, &format!("{A}/settings/query"), None)
        .await?;
    let limits = c
        .request(Method::GET, &format!("{A}/trading-limit/query"), None)
        .await?;
    let limit = backend_decimal(&settings["dailyLimit"])?;
    let used = backend_decimal(&limits["quotaUsed"])?;
    if limit <= used {
        return Err("daily trading quota exhausted".into());
    }
    if settings["devMode"]["balanceExceeded"] == true {
        return Err("developer mode balance exceeded".into());
    }
    if settings["devMode"]["enabled"] == true {
        let expiry = settings["devMode"]["expiresAt"]
            .as_i64()
            .or_else(|| {
                settings["devMode"]["expiresAt"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
            })
            .ok_or("developer mode expiry missing")?;
        if expiry <= chrono::Utc::now().timestamp() {
            return Err("developer mode expired".into());
        }
    }
    let lock = c
        .request(
            Method::POST,
            &format!("{A}/order/tx-lock-status"),
            Some(json!({"binanceChainId":chain})),
        )
        .await?;
    if lock["status"] != "UNLOCKED" {
        return Err("chain transaction lock is locked or unknown".into());
    }
    Ok(())
}
fn backend_decimal(v: &Value) -> Result<BigDecimal, String> {
    if let Some(s) = v.as_str() {
        decimal(s)
    } else if v.is_number() {
        decimal(&v.to_string())
    } else {
        Err("missing backend decimal value".into())
    }
}
fn check_quote(a: &QuoteArgs, q: &Value) -> Result<(), String> {
    if q.get("code")
        .is_some_and(|v| !v.is_null() && v.as_str() != Some("000000") && v.as_str() != Some(""))
    {
        return Err("native quote rejected".into());
    }
    if q["quoteId"].as_str().is_none_or(|s| s.is_empty())
        || q["toCoinAmount"]
            .as_str()
            .and_then(|s| decimal(s).ok())
            .is_none_or(|n| n <= 0)
    {
        return Err("native quote missing quoteId or positive output amount".into());
    }
    if a.slippage != "auto" {
        let actual =
            backend_decimal(&q["slippage"]).map_err(|_| "native quote missing slippage")?;
        let requested = decimal(&slippage(&a.slippage)?)?;
        if actual > requested {
            return Err("native quote slippage exceeds explicitly requested limit".into());
        }
    }
    Ok(())
}
async fn swap(c: &mut Client, a: &SwapArgs, r: &Registry) -> Result<Value, String> {
    let q = &a.quote;
    if !native_token(&q.binance_chain_id, &q.to_token) && !a.acknowledge_token_risk {
        return Err(
            "target token is not locally audited; require --acknowledge-token-risk (not an audit)"
                .into(),
        );
    }
    if a.mev == "true" && !matches!(q.binance_chain_id.as_str(), "1" | "56" | "CT_501") {
        return Err(
            "MEV protection unsupported on this chain; explicitly choose --mev false".into(),
        );
    }
    let from = r.token(&q.binance_chain_id, &q.from_token);
    let token = from.or_else(|| r.token(&q.binance_chain_id, &q.to_token));
    let ondo = token.is_some_and(|t| t.kind == "ondo");
    if ondo && (a.mev != "false" || gas(&a.gas_level)? != "MEDIUM") {
        return Err("Ondo endpoint supports neither MEV nor gas-level selection; require --mev false and default MEDIUM gasLevel".into());
    }
    safeguards(c, &q.binance_chain_id).await?;
    let quoted = quote(c, q, r).await?;
    check_quote(q, &quoted)?;
    let qty = if let Some(t) = from {
        wallet_sell_amount(
            c,
            &q.binance_chain_id,
            &q.from_token,
            &q.from_token_qty,
            &t.multiplier,
        )
        .await?
    } else {
        q.from_token_qty.clone()
    };
    let mut body = json!({"binanceChainId":q.binance_chain_id,"fromToken":q.from_token,"toToken":q.to_token,"amount":qty,"slippage":slippage(&q.slippage)?});
    if !ondo {
        body["mev"] = json!(a.mev == "true");
        body["gasLevel"] = json!(gas(&a.gas_level)?);
    }
    if token.is_some() {
        body["tokenShare"] = if from.is_some() {
            json!(q.from_token_qty)
        } else {
            Value::Null
        };
        body["multiplier"] = from.map(|t| json!(t.multiplier)).unwrap_or(Value::Null);
    }
    let endpoint = if ondo {
        "ondo/place-order"
    } else {
        "place-order"
    };
    let result = c
        .request(Method::POST, &format!("{D}/{endpoint}"), Some(body))
        .await;
    match result {
        Err(_) => Ok(
            json!({"status":"UNCERTAIN","retrySafe":false,"message":"submission outcome unknown; reconcile order history; never automatically resubmit","quote":quoted}),
        ),
        Ok(data) => {
            let mut out = order_result(data, &q.from_token)?;
            out["quote"] = quoted;
            Ok(out)
        }
    }
}
fn sell_amount(
    shares: &str,
    multiplier: &str,
    balance: &str,
    scale: i64,
) -> Result<String, String> {
    let s = decimal(shares)?;
    let m = decimal(multiplier)?;
    let b = decimal(balance)?;
    if m <= 0 || !(0..=255).contains(&scale) {
        return Err("invalid RWA multiplier or decimals".into());
    }
    // Cross multiplication tests the exact rational quotient before rounding/clamping.
    if s > &b * &m {
        return Err("insufficient balance: requested RWA shares exceed wallet balance".into());
    }
    if m == 1 {
        return Ok(shares.into());
    }
    let rounded = rwa::divide_up(shares, multiplier, scale)?;
    if decimal(&rounded)? < b {
        Ok(rounded)
    } else {
        Ok(balance.into())
    }
}
async fn wallet_sell_amount(
    c: &mut Client,
    chain: &str,
    token: &str,
    shares: &str,
    multiplier: &str,
) -> Result<String, String> {
    let wallet = c
        .request(Method::POST, &format!("{A}/mpc-wallet/token/list"), None)
        .await?;
    let row = wallet["tokenList"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|t| {
                t["binanceChainId"] == chain
                    && t["contractAddress"].as_str().is_some_and(|a| {
                        if chain == "CT_501" {
                            a == token
                        } else {
                            a.eq_ignore_ascii_case(token)
                        }
                    })
            })
        })
        .ok_or("RWA token not found in wallet token list")?;
    let balance = row["balance"]
        .as_str()
        .ok_or("missing exact wallet balance")?;
    let scale = row["decimals"]
        .as_i64()
        .ok_or("missing wallet token decimals")?;
    sell_amount(shares, multiplier, balance, scale)
}
fn order_result(data: Value, _from: &str) -> Result<Value, String> {
    if let Some(code) = data
        .get("code")
        .filter(|c| !c.is_null() && c.as_str() != Some("") && c.as_str() != Some("000000"))
    {
        let safe_code = code
            .as_str()
            .filter(|s| s.len() <= 32 && s.bytes().all(|b| b.is_ascii_digit()))
            .unwrap_or("UNKNOWN");
        return Err(json!({"error":"ORDER_API_ERROR","code":safe_code,"message":"Order rejected; reconcile history before any new intent","orderId":data["orderId"],"orderExpireTime":data["orderExpireTime"]}).to_string());
    }
    if data["orderId"].as_str().is_none_or(|s| s.is_empty()) {
        return Ok(
            json!({"status":"UNCERTAIN","retrySafe":false,"message":"missing orderId; reconcile history"}),
        );
    }
    let mut out = data;
    out["status"] = json!("SUBMITTED");
    out["retrySafe"] = json!(false);
    Ok(out)
}
#[derive(Debug, Clone, Subcommand)]
pub enum MarketCommand {
    Quote(QuoteArgs),
    Swap(SwapArgs),
    List(ListArgs),
}
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteArgs {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long = "fromToken")]
    pub from_token: String,
    #[arg(long = "toToken")]
    pub to_token: String,
    #[arg(long = "fromTokenQty")]
    pub from_token_qty: String,
    #[arg(long, default_value = "auto")]
    pub slippage: String,
}
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapArgs {
    #[command(flatten)]
    #[serde(flatten)]
    pub quote: QuoteArgs,
    #[arg(long, default_value = "true")]
    pub mev: String,
    #[arg(long = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    /// Explicit risk acceptance; this does not represent a token audit.
    #[arg(long)]
    pub acknowledge_token_risk: bool,
}
#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long = "orderId")]
    pub order_id: Option<String>,
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long = "fromToken")]
    pub from_token: Option<String>,
    #[arg(long = "toToken")]
    pub to_token: Option<String>,
    #[arg(long = "startTime")]
    pub start_time: Option<String>,
    #[arg(long = "endTime")]
    pub end_time: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub page: u32,
    #[arg(long = "pageSize", default_value_t = 20)]
    pub page_size: u32,
}
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
#[command(group(clap::ArgGroup::new("quantity").required(true).multiple(false).args(["amount","max"])))]
pub struct SendArgs {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub amount: Option<String>,
    #[arg(long)]
    pub max: bool,
    #[arg(long)]
    pub recipient: String,
    #[arg(long = "tokenAddress")]
    pub token_address: String,
    #[arg(long = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
}
fn validate_send(a: &SendArgs) -> Result<(), String> {
    if a.amount.is_some() == a.max {
        return Err("specify exactly one of --amount or --max".into());
    }
    address(&a.binance_chain_id, &a.recipient)?;
    address(&a.binance_chain_id, &a.token_address)?;
    gas(&a.gas_level)?;
    if let Some(v) = &a.amount {
        amount(v)?;
    }
    Ok(())
}
pub fn send_payload(a: &SendArgs) -> Result<Value, String> {
    validate_send(a)?;
    Ok(json!({"operation":"wallet.send","args":a}))
}
/// One backend transfer only. Backend address-book/confirmation policy remains authoritative.
pub async fn send(c: &mut Client, a: &SendArgs) -> Result<Value, String> {
    validate_send(a)?;
    session(c)?;
    let mut body = json!({"binanceChainId":a.binance_chain_id,"recipientAddress":a.recipient,"tokenAddress":a.token_address,"feeLevel":gas(&a.gas_level)?});
    if let Some(qty) = &a.amount {
        let r = rwa::fetch(c).await?;
        let qty = if let Some(t) = r.token(&a.binance_chain_id, &a.token_address) {
            wallet_sell_amount(c, &a.binance_chain_id, &a.token_address, qty, &t.multiplier).await?
        } else {
            qty.clone()
        };
        body["amount"] = json!(qty);
    } else {
        body["isMax"] = json!(true);
    }
    match c
        .request(Method::POST, &format!("{A}/transfer"), Some(body))
        .await
    {
        Err(_) => Ok(
            json!({"status":"UNCERTAIN","retrySafe":false,"message":"transfer outcome unknown; reconcile transaction history; never automatically resubmit"}),
        ),
        Ok(data) => {
            if data.get("code").is_some_and(|code| {
                !code.is_null() && code.as_str() != Some("") && code.as_str() != Some("000000")
            }) {
                return Err(
                    "transfer rejected by internal API code; do not automatically resubmit".into(),
                );
            }
            if data["status"] == "PENDING_CONFIRMATION" {
                return Ok(
                    json!({"status":"PENDING_CONFIRMATION","retrySafe":false,"message":"APP_CONFIRMATION_REQUIRED; use the Binance app, do not resubmit","backendData":data}),
                );
            }
            if matches!(data["status"].as_str(), Some("SUBMIT_FAIL" | "FAILED")) {
                return Err("transfer rejected; do not automatically resubmit".into());
            }
            let status = if data["txHash"].as_str().is_some_and(|s| !s.is_empty()) {
                "SUBMITTED"
            } else {
                "UNCERTAIN"
            };
            Ok(
                json!({"status":status,"retrySafe":false,"txHash":data["txHash"],"backendData":data}),
            )
        }
    }
}
impl Default for ListArgs {
    fn default() -> Self {
        Self {
            order_id: None,
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
}
fn timestamp(s: &str) -> Result<i64, String> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err("time must be an integer millisecond timestamp".into());
    }
    let n = s.parse::<i64>().map_err(|_| "invalid time")?;
    if n < 1_577_836_800_000
        || n > chrono::Utc::now().timestamp_millis() + 100 * 365 * 24 * 60 * 60 * 1000
    {
        return Err("time out of range (2020 through now + 100 years)".into());
    }
    Ok(n)
}
fn list_body(a: &ListArgs) -> Result<Value, String> {
    if !(1..=10000).contains(&a.page) || !(1..=100).contains(&a.page_size) {
        return Err("page must be 1..10000, pageSize 1..100".into());
    }
    if a.status
        .as_deref()
        .is_some_and(|s| !matches!(s, "PENDING" | "FINISHED" | "FAILED"))
    {
        return Err("status must be PENDING, FINISHED, or FAILED".into());
    }
    if let Some(c) = &a.binance_chain_id {
        chain(c)?;
    }
    for t in [&a.from_token, &a.to_token].into_iter().flatten() {
        address(
            a.binance_chain_id
                .as_deref()
                .unwrap_or(if t.starts_with("0x") { "1" } else { "CT_501" }),
            t,
        )?;
    }
    let start = a.start_time.as_deref().map(timestamp).transpose()?;
    let end = a.end_time.as_deref().map(timestamp).transpose()?;
    if start.zip(end).is_some_and(|(s, e)| s > e) {
        return Err("startTime exceeds endTime".into());
    }
    if let Some(id) = &a.order_id {
        if id.is_empty()
            || id.len() > 64
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid orderId".into());
        }
        return Ok(json!({"orderId":id,"page":1,"pageSize":1}));
    }
    let mut body = json!({"page":a.page,"pageSize":a.page_size,"sort":"DESC"});
    for (k, v) in [
        ("binanceChainId", &a.binance_chain_id),
        ("fromTokenAddress", &a.from_token),
        ("toTokenAddress", &a.to_token),
    ] {
        if let Some(v) = v {
            body[k] = json!(v);
        }
    }
    if let Some(v) = &a.status {
        body["statusList"] = json!([v]);
    }
    if let Some(v) = start {
        body["startTime"] = json!(v);
    }
    if let Some(v) = end {
        body["endTime"] = json!(v);
    }
    Ok(body)
}
fn str_field<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn date(v: &Value) -> Value {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .filter(|n| *n != 0)
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|t| {
            json!(t
                .with_timezone(&chrono::Local)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, false))
        })
        .unwrap_or(Value::Null)
}
fn converted(v: &Value, r: &Registry, chain: &str, token: &str) -> Result<Value, String> {
    if v.is_null() || v.as_str() == Some("") {
        return Ok(v.clone());
    }
    if let Some(t) = r.token(chain, token) {
        Ok(json!(rwa::multiply_down(
            v.as_str().ok_or("RWA quantity must be a decimal string")?,
            &t.multiplier,
            18
        )?))
    } else {
        Ok(v.clone())
    }
}
fn map_order(row: &Value, r: &Registry) -> Result<Value, String> {
    let chain = if !str_field(row, "walletChainId").is_empty() {
        str_field(row, "walletChainId")
    } else {
        str_field(row, "fromTokenChainId")
    };
    let from_chain = if !str_field(row, "fromTokenChainId").is_empty() {
        str_field(row, "fromTokenChainId")
    } else {
        chain
    };
    let to_chain = if !str_field(row, "toTokenChainId").is_empty() {
        str_field(row, "toTokenChainId")
    } else {
        chain
    };
    let from = str_field(row, "fromTokenAddress");
    let to = str_field(row, "toTokenAddress");
    let mut out = json!({"orderType":"market","orderId":if str_field(row,"orderId").is_empty(){str_field(row,"id")}else{str_field(row,"orderId")},"chain":chain,"fromToken":checksum_address(from),"fromTokenName":str_field(row,"fromTokenName"),"fromTokenQty":converted(&json!(str_field(row,"fromTokenQty")),r,from_chain,from)?,"toToken":checksum_address(to),"toTokenName":str_field(row,"toTokenName"),"status":str_field(row,"status"),"bookTime":date(&row["bookTime"]),"updatedTime":date(&row["updatedTime"])});
    if row.get("toTokenActualQty").is_some() {
        out["toTokenActualQty"] = converted(&row["toTokenActualQty"], r, to_chain, to)?;
    }
    for (src, dst) in [("slippage", "slippage"), ("orderTxId", "txHash")] {
        if let Some(v) = row.get(src) {
            out[dst] = v.clone();
        }
    }
    if let Some(extend) = row["extend"]
        .as_str()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
    {
        if let Some(refund) = extend["refund"].as_array().filter(|r| !r.is_empty()) {
            let mut refunds = Vec::new();
            for v in refund {
                let mut item = json!({});
                for k in ["tokenAddress", "symbol"] {
                    if let Some(v) = v.get(k) {
                        item[k] = v.clone();
                    }
                }
                if v.get("amount").is_some() {
                    item["amount"] =
                        converted(&v["amount"], r, chain, str_field(v, "tokenAddress"))?;
                }
                refunds.push(item);
            }
            out["refund"] = json!(refunds);
        }
    }
    Ok(out)
}
fn chain(s: &str) -> Result<(), String> {
    if matches!(s, "1" | "56" | "8453" | "CT_501" | "4663") {
        Ok(())
    } else {
        Err("unsupported binanceChainId".into())
    }
}
fn address(c: &str, s: &str) -> Result<(), String> {
    chain(c)?;
    if c == "CT_501" {
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
fn validate_quote(a: &QuoteArgs) -> Result<(), String> {
    chain(&a.binance_chain_id)?;
    address(&a.binance_chain_id, &a.from_token)?;
    address(&a.binance_chain_id, &a.to_token)?;
    amount(&a.from_token_qty)?;
    slippage(&a.slippage)?;
    Ok(())
}
fn gas(s: &str) -> Result<String, String> {
    let v = s.to_ascii_uppercase();
    if matches!(v.as_str(), "LOW" | "MEDIUM" | "HIGH") {
        Ok(v)
    } else {
        Err("gasLevel must be LOW, MEDIUM, or HIGH".into())
    }
}
fn validate_swap(a: &SwapArgs) -> Result<(), String> {
    validate_quote(&a.quote)?;
    gas(&a.gas_level)?;
    if !matches!(a.mev.as_str(), "true" | "false") {
        return Err("mev must be true or false".into());
    }
    Ok(())
}
pub fn is_mutating(c: &MarketCommand) -> bool {
    matches!(c, MarketCommand::Swap(_))
}
/// Durable original intent: no RWA conversion or requested-protection rewriting.
pub fn mutation_payload(c: &MarketCommand) -> Result<Value, String> {
    match c {
        MarketCommand::Swap(a) => {
            validate_swap(a)?;
            Ok(json!({"operation":"market-order.swap","args":a}))
        }
        _ => Err("not a mutation".into()),
    }
}

fn decimal(s: &str) -> Result<BigDecimal, String> {
    if s.is_empty()
        || s.len() > 512
        || !s
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        || s.bytes().filter(|b| *b == b'.').count() > 1
    {
        return Err("expected an exact unsigned decimal string".into());
    }
    BigDecimal::from_str(s).map_err(|_| "invalid decimal".into())
}
fn amount(s: &str) -> Result<BigDecimal, String> {
    let n = decimal(s)?;
    if n <= 0 || n > BigDecimal::from_str("1000000000000000000").unwrap() {
        return Err("amount must be greater than zero and at most 1e18".into());
    }
    Ok(n)
}
fn slippage(s: &str) -> Result<String, String> {
    if s == "auto" {
        return Ok(s.into());
    }
    let n = decimal(s)?;
    if n > 100 {
        return Err("slippage must be auto or 0..100 percent".into());
    }
    Ok((n / BigDecimal::from(100)).normalized().to_plain_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn server(
        data: Vec<Value>,
    ) -> (
        crate::client::Client,
        std::sync::mpsc::Receiver<(String, Value)>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || {
            for d in data {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut b = [0; 4096];
                    let n = s.read(&mut b).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&b[..n]);
                    if let Some(e) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..e]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|n| n.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= e + 4 + len {
                            break;
                        }
                    }
                }
                let req = String::from_utf8(bytes).unwrap();
                let (head, body) = req.split_once("\r\n\r\n").unwrap();
                tx.send((
                    head.lines().next().unwrap().into(),
                    if body.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str(body).unwrap()
                    },
                ))
                .unwrap();
                if let Some(status) = d["__http_status"].as_u64() {
                    s.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                    drop(s);
                    listener.set_nonblocking(true).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    assert!(
                        listener.accept().is_err(),
                        "must not retry an uncertain write"
                    );
                    continue;
                }
                if d["__disconnect"] == true {
                    drop(s);
                    listener.set_nonblocking(true).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    assert!(
                        listener.accept().is_err(),
                        "must not retry disconnected writes"
                    );
                    continue;
                }
                let body = json!({"code":"000000","data":d}).to_string();
                s.write_all(
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
        let creds = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        (
            crate::client::Client::test_client(creds, &origin).unwrap(),
            rx,
            h,
        )
    }
    #[test]
    fn backend_numeric_decimals_preserve_full_json_precision() {
        let value: Value = serde_json::from_str("9007199254740993.123456789123456789").unwrap();
        assert_eq!(
            backend_decimal(&value).unwrap().to_plain_string(),
            "9007199254740993.123456789123456789"
        );
    }
    #[tokio::test]
    async fn failed_order_code_never_echoes_backend_secrets() {
        let data = json!({"code":"123","message":"synthetic-secret-session","orderId":"recover-order","orderExpireTime":42});
        let error = order_result(data, &quote_args().from_token).unwrap_err();
        assert!(error.contains("ORDER_API_ERROR") && error.contains("recover-order"));
        assert!(
            !error.contains("synthetic-secret-session"),
            "backend message leaked"
        );
    }
    #[tokio::test]
    async fn normal_quote_tcp_preserves_metadata_without_float_rounding() {
        let (mut c, rx, h) = server(vec![
            json!([]),
            json!({"uniQuoteId":"q","fromCoinAmount":"1","toCoinAmount":"2","feeDetail":{"rate":"0.1"},"gasDetails":[{"x":1}],"newMetadata":true}),
        ]);
        let out = run(&mut c, &MarketCommand::Quote(quote_args()))
            .await
            .unwrap();
        assert_eq!(out["quoteId"], "q");
        assert_eq!(out["feeDetail"]["rate"], "0.1");
        assert_eq!(out["newMetadata"], true);
        assert!(rx.recv().unwrap().0.contains("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list "));
        let (head, body) = rx.recv().unwrap();
        assert!(head.starts_with("POST /bapi/defi/v1/public/wallet-direct/web-dex/agent/quote "));
        assert_eq!(
            body,
            json!({"binanceChainId":"56","fromToken":quote_args().from_token,"toToken":quote_args().to_token,"amount":"0001.00","slippage":"auto"})
        );
        h.join().unwrap();
    }
    #[tokio::test]
    async fn rwa_quote_routes_and_converts_sell_up_and_buy_down() {
        for (kind, suffix) in [(1, "ondo/quote"), (3, "quote"), (99, "quote")] {
            for sell in [true, false] {
                let mut a = quote_args();
                a.from_token_qty = "1".into();
                let addr = if sell { &a.from_token } else { &a.to_token };
                let (mut c, rx, h) = server(vec![
                    json!([{"chainId":"56","contractAddress":addr,"multiplier":"3","type":kind}]),
                    json!({"quoteId":"q","toCoinAmount":"0.3333333333333333339","toMultiplier":"6","toTokenShare":if kind==1 {json!("7")}else{Value::Null}}),
                ]);
                let out = run(&mut c, &MarketCommand::Quote(a.clone())).await.unwrap();
                rx.recv().unwrap();
                let (head, body) = rx.recv().unwrap();
                assert!(head.starts_with(&format!("POST {D}/{suffix} ")));
                let key = if kind == 1 { "fromAmount" } else { "amount" };
                assert_eq!(body[key], if sell { "0.333333333333333334" } else { "1" });
                assert_eq!(
                    body["fromTokenShare"],
                    if sell { json!("1") } else { Value::Null }
                );
                assert_eq!(
                    body["fromMultiplier"],
                    if sell { json!("3") } else { Value::Null }
                );
                if sell {
                    assert_eq!(out["fromCoinAmount"], "1");
                } else {
                    assert_eq!(
                        out["toCoinAmount"],
                        if kind == 1 {
                            "7"
                        } else {
                            "1.000000000000000001"
                        }
                    );
                }
                h.join().unwrap();
            }
        }
    }
    fn swap_args() -> SwapArgs {
        SwapArgs {
            quote: quote_args(),
            mev: "true".into(),
            gas_level: "MEDIUM".into(),
            acknowledge_token_risk: true,
        }
    }
    #[tokio::test]
    async fn swap_requires_preflight_quote_and_preserves_submitted_not_filled() {
        for result in [
            json!({"orderId":"o","clientOrderId":"c"}),
            json!({"code":"123","message":"bad {{0}}","orderId":"o","orderExpireTime":42}),
        ] {
            let (mut c, rx, h) = server(vec![
                json!([]),
                json!({"dailyLimit":"100","tradeAllTokens":true}),
                json!({"quotaUsed":"1"}),
                json!({"status":"UNLOCKED"}),
                json!({"quoteId":"q","toCoinAmount":"2","slippage":"0.01"}),
                result.clone(),
            ]);
            let out = run(&mut c, &MarketCommand::Swap(swap_args())).await;
            if result.get("code").is_some() {
                let e = out.unwrap_err();
                assert!(e.contains("ORDER_API_ERROR") && e.contains("o") && e.contains("42"));
            } else {
                let o = out.unwrap();
                assert_eq!(o["status"], "SUBMITTED");
                assert_eq!(o["orderId"], "o");
                assert_eq!(o["quote"]["quoteId"], "q");
                assert_ne!(o["status"], "FILLED");
            }
            let calls: Vec<_> = (0..6).map(|_| rx.recv().unwrap()).collect();
            assert!(calls[1].0.starts_with(&format!("GET {A}/settings/query ")));
            assert!(calls[2]
                .0
                .starts_with(&format!("GET {A}/trading-limit/query ")));
            assert_eq!(calls[3].1, json!({"binanceChainId":"56"}));
            assert!(calls[4].0.starts_with(&format!("POST {D}/quote ")));
            assert!(calls[5].0.starts_with(&format!("POST {D}/place-order ")));
            assert_eq!(calls[5].1["mev"], true);
            assert_eq!(calls[5].1["gasLevel"], "MEDIUM");
            h.join().unwrap();
        }
    }
    #[test]
    fn rwa_sell_rounding_clamp_and_oversell_are_exact() {
        assert_eq!(
            sell_amount("1", "3", "0.3333333333333333335", 18).unwrap(),
            "0.3333333333333333335"
        );
        assert_eq!(
            sell_amount("1", "3", "1", 18).unwrap(),
            "0.333333333333333334"
        );
        assert_eq!(sell_amount("1", "3", "1", 6).unwrap(), "0.333334");
        assert_eq!(sell_amount("1.00", "1", "2", 6).unwrap(), "1.00");
        assert!(sell_amount("1", "3", "0.333333333333333333", 18).is_err());
        assert!(sell_amount("3", "1", "2", 18).is_err());
    }
    #[tokio::test]
    async fn rwa_swap_buy_and_sell_use_original_routes_and_null_share_fields() {
        for kind in [1, 3, 99] {
            for sell in [true, false] {
                let mut a = swap_args();
                a.quote.from_token_qty = "1".into();
                a.mev = if kind == 1 { "false" } else { "true" }.into();
                let token = if sell {
                    a.quote.from_token.clone()
                } else {
                    a.quote.to_token.clone()
                };
                let mut replies = vec![
                    json!([{"chainId":"56","contractAddress":token,"multiplier":"3","type":kind}]),
                    json!({"dailyLimit":"10"}),
                    json!({"quotaUsed":"0"}),
                    json!({"status":"UNLOCKED"}),
                    json!({"quoteId":"q","toCoinAmount":"1"}),
                ];
                if sell {
                    replies.push(json!({"tokenList":[{"binanceChainId":"56","contractAddress":token,"balance":"0.3333333333333333335","decimals":18}]}));
                }
                replies.push(json!({"orderId":"o"}));
                let (mut c, rx, h) = server(replies);
                let out = run(&mut c, &MarketCommand::Swap(a)).await.unwrap();
                assert_eq!(out["status"], "SUBMITTED");
                let calls: Vec<_> = rx.iter().take(if sell { 7 } else { 6 }).collect();
                let (head, body) = calls.last().unwrap();
                assert!(head.starts_with(&format!(
                    "POST {D}/{}place-order ",
                    if kind == 1 { "ondo/" } else { "" }
                )));
                assert_eq!(
                    body["amount"],
                    if sell { "0.3333333333333333335" } else { "1" }
                );
                assert_eq!(
                    body["tokenShare"],
                    if sell { json!("1") } else { Value::Null }
                );
                assert_eq!(
                    body["multiplier"],
                    if sell { json!("3") } else { Value::Null }
                );
                if kind == 1 {
                    assert!(body.get("gasLevel").is_none() && body.get("mev").is_none());
                }
                h.join().unwrap();
            }
        }
    }
    #[tokio::test]
    async fn list_validates_filters_and_maps_actual_quantities_refunds_and_status() {
        let a = ListArgs {
            binance_chain_id: Some("56".into()),
            status: Some("PENDING".into()),
            start_time: Some("1577836800000".into()),
            page: 2,
            page_size: 5,
            ..Default::default()
        };
        let row = json!({"id":"o","walletChainId":"56","fromTokenAddress":quote_args().from_token,"fromTokenQty":"2","toTokenAddress":quote_args().to_token,"toTokenActualQty":"0.1","status":"SUBMITTED","orderTxId":"tx","bookTime":1577836800000_i64,"updatedTime":1577836801000_i64,"extend":json!({"refund":[{"tokenAddress":quote_args().from_token,"amount":"0.25","symbol":"R"}]}).to_string()});
        let (mut c, rx, h) = server(vec![
            json!([{"chainId":"56","contractAddress":quote_args().from_token,"multiplier":"3","type":1},{"chainId":"56","contractAddress":quote_args().to_token,"multiplier":"2","type":3}]),
            json!({"total":1,"rows":[row]}),
        ]);
        let out = run(&mut c, &MarketCommand::List(a)).await.unwrap();
        assert_eq!(out["page"], 2);
        assert_eq!(out["list"][0]["fromTokenQty"], "6");
        assert_eq!(out["list"][0]["toTokenActualQty"], "0.2");
        assert_eq!(out["list"][0]["refund"][0]["amount"], "0.75");
        assert_eq!(out["list"][0]["status"], "SUBMITTED");
        assert_eq!(out["list"][0]["txHash"], "tx");
        assert!(out["list"][0]["bookTime"]
            .as_str()
            .unwrap()
            .starts_with("2020-01-01"));
        rx.recv().unwrap();
        let (head, body) = rx.recv().unwrap();
        assert!(head.starts_with(&format!("POST {D}/batch-query-market-orders ")));
        assert_eq!(
            body,
            json!({"binanceChainId":"56","statusList":["PENDING"],"startTime":1577836800000_i64,"page":2,"pageSize":5,"sort":"DESC"})
        );
        h.join().unwrap();
        let a = ListArgs {
            order_id: Some("order-1".into()),
            ..Default::default()
        };
        assert_eq!(
            list_body(&a).unwrap(),
            json!({"orderId":"order-1","page":1,"pageSize":1})
        );
        for a in [
            ListArgs {
                page: 0,
                ..Default::default()
            },
            ListArgs {
                page_size: 101,
                ..Default::default()
            },
            ListArgs {
                status: Some("SUBMITTED".into()),
                ..Default::default()
            },
            ListArgs {
                start_time: Some("1577836800000junk".into()),
                ..Default::default()
            },
            ListArgs {
                start_time: Some("1577836800000".into()),
                end_time: Some("1577836799999".into()),
                ..Default::default()
            },
            ListArgs {
                order_id: Some("../../".into()),
                ..Default::default()
            },
        ] {
            assert!(list_body(&a).is_err());
        }
    }
    #[tokio::test]
    async fn transfer_internal_failure_code_cannot_be_reported_as_submitted() {
        let (mut client, rx, thread) = server(vec![
            json!([]),
            json!({"code":"123","message":"private-backend-message","txHash":"not-proof"}),
        ]);
        let result = send(&mut client, &send_args()).await;
        thread.join().unwrap();
        assert_eq!(rx.try_iter().count(), 2);
        let error = result.unwrap_err();
        assert!(error.contains("transfer rejected"));
        assert!(!error.contains("private-backend-message"));
    }
    fn send_args() -> SendArgs {
        SendArgs {
            binance_chain_id: "56".into(),
            amount: Some("1.00".into()),
            max: false,
            recipient: quote_args().to_token,
            token_address: quote_args().from_token,
            gas_level: "HIGH".into(),
        }
    }
    #[test]
    fn send_intent_requires_exactly_amount_or_max_without_changing_input() {
        let mut a = send_args();
        assert_eq!(send_payload(&a).unwrap()["args"]["amount"], "1.00");
        a.max = true;
        assert!(send_payload(&a).is_err());
        a.amount = None;
        assert!(send_payload(&a).is_ok());
        a.max = false;
        assert!(send_payload(&a).is_err());
    }
    #[tokio::test]
    async fn send_tcp_fee_level_recipient_policy_and_rwa_share_handling() {
        for mode in [0, 1, 2] {
            let mut a = send_args();
            let mut replies = Vec::new();
            if mode == 2 {
                a.max = true;
                a.amount = None;
            } else {
                replies.push(if mode==1{json!([{"chainId":"56","contractAddress":a.token_address,"multiplier":"3","type":1}])}else{json!([])});
                if mode == 1 {
                    replies.push(json!({"tokenList":[{"binanceChainId":"56","contractAddress":a.token_address,"balance":"1","decimals":6}]}));
                }
            }
            replies.push(json!({"txHash":"tx","status":"PROCESSING"}));
            let count = replies.len();
            let (mut c, rx, h) = server(replies);
            let out = send(&mut c, &a).await.unwrap();
            assert_eq!(out["status"], "SUBMITTED");
            assert_eq!(out["txHash"], "tx");
            let reqs: Vec<_> = (0..count).map(|_| rx.recv().unwrap()).collect();
            let (head, body) = reqs.last().unwrap();
            assert!(head.starts_with(&format!("POST {A}/transfer ")));
            assert_eq!(body["feeLevel"], "HIGH");
            assert!(body.get("gasLevel").is_none());
            assert_eq!(body["recipientAddress"], a.recipient);
            if mode == 2 {
                assert_eq!(body["isMax"], true);
                assert!(body.get("amount").is_none());
            } else {
                assert_eq!(body["amount"], if mode == 1 { "0.333334" } else { "1.00" });
            }
            h.join().unwrap();
        }
    }
    #[tokio::test]
    async fn explicit_slippage_mismatch_blocks_before_any_write() {
        let mut a = swap_args();
        a.quote.slippage = "1".into();
        let (mut c, rx, h) = server(vec![
            json!([]),
            json!({"dailyLimit":100}),
            json!({"quotaUsed":0}),
            json!({"status":"UNLOCKED"}),
            json!({"quoteId":"q","toCoinAmount":"2","slippage":"0.02"}),
        ]);
        let e = run(&mut c, &MarketCommand::Swap(a)).await.unwrap_err();
        assert!(e.contains("slippage"), "{e}");
        for _ in 0..5 {
            rx.recv().unwrap();
        }
        h.join().unwrap();
    }
    #[tokio::test]
    async fn uncertain_writes_never_retry_timeout_http_or_disconnected_socket() {
        for failure in [
            json!({"__http_status":408}),
            json!({"__http_status":504}),
            json!({"__disconnect":true}),
        ] {
            let mut a = send_args();
            a.max = true;
            a.amount = None;
            let (mut c, rx, h) = server(vec![failure.clone()]);
            let out = send(&mut c, &a).await.unwrap();
            assert_eq!(out["status"], "UNCERTAIN");
            assert_eq!(out["retrySafe"], false);
            assert!(rx.recv().unwrap().0.contains("/transfer "));
            h.join().unwrap();
            assert!(rx.try_recv().is_err());
            let (mut c, rx, h) = server(vec![
                json!([]),
                json!({"dailyLimit":"10"}),
                json!({"quotaUsed":"0"}),
                json!({"status":"UNLOCKED"}),
                json!({"quoteId":"q","toCoinAmount":"1"}),
                failure,
            ]);
            let out = run(&mut c, &MarketCommand::Swap(swap_args()))
                .await
                .unwrap();
            assert_eq!(out["status"], "UNCERTAIN");
            assert_eq!(out["retrySafe"], false);
            for _ in 0..6 {
                rx.recv().unwrap();
            }
            h.join().unwrap();
            assert!(rx.try_recv().is_err());
        }
    }
    #[tokio::test]
    async fn backend_recipient_confirmation_is_not_reported_as_sent() {
        let mut a = send_args();
        a.max = true;
        a.amount = None;
        let (mut c, rx, h) = server(vec![json!({"status":"PENDING_CONFIRMATION","orderId":"x"})]);
        let out = send(&mut c, &a).await.unwrap();
        assert_eq!(out["status"], "PENDING_CONFIRMATION");
        assert!(out["message"]
            .as_str()
            .unwrap()
            .contains("APP_CONFIRMATION_REQUIRED"));
        assert!(out.get("txHash").is_none());
        rx.recv().unwrap();
        h.join().unwrap();
    }
    #[tokio::test]
    async fn locked_expired_exhausted_or_unknown_settings_block_submission() {
        for (settings, quota, lock, error) in [
            (
                json!({"dailyLimit":"1"}),
                json!({"quotaUsed":"1"}),
                None,
                "quota",
            ),
            (
                json!({"dailyLimit":"10","devMode":{"enabled":true,"expiresAt":1}}),
                json!({"quotaUsed":"0"}),
                None,
                "expired",
            ),
            (
                json!({"dailyLimit":"10","devMode":{"balanceExceeded":true}}),
                json!({"quotaUsed":"0"}),
                None,
                "balance exceeded",
            ),
            (
                json!({"dailyLimit":"10"}),
                json!({"quotaUsed":"0"}),
                Some(json!({"status":"LOCKED"})),
                "lock",
            ),
            (
                json!({"dailyLimit":"10"}),
                json!({"quotaUsed":"0"}),
                Some(json!({})),
                "lock",
            ),
            (json!({}), json!({"quotaUsed":"0"}), None, "decimal"),
        ] {
            let mut replies = vec![json!([]), settings, quota];
            if let Some(l) = lock {
                replies.push(l);
            }
            let n = replies.len();
            let (mut c, rx, h) = server(replies);
            let e = run(&mut c, &MarketCommand::Swap(swap_args()))
                .await
                .unwrap_err();
            assert!(e.contains(error), "{e}");
            for _ in 0..n {
                let (head, _) = rx.recv().unwrap();
                assert!(!head.contains("/place-order ") && !head.contains("/quote "));
            }
            h.join().unwrap();
        }
    }
    #[tokio::test]
    async fn unacknowledged_target_and_unsupported_protections_fail_closed() {
        for (a, registry, error) in {
            let mut risk = swap_args();
            risk.acknowledge_token_risk = false;
            let mut chain = swap_args();
            chain.quote.binance_chain_id = "8453".into();
            let ondo = swap_args();
            let mut gas = swap_args();
            gas.mev = "false".into();
            gas.gas_level = "HIGH".into();
            vec![
                (risk, json!([]), "acknowledge"),
                (chain, json!([]), "MEV"),
                (
                    ondo,
                    json!([{"chainId":"56","contractAddress":quote_args().to_token,"multiplier":"3","type":1}]),
                    "Ondo",
                ),
                (
                    gas,
                    json!([{"chainId":"56","contractAddress":quote_args().to_token,"multiplier":"3","type":1}]),
                    "gas-level",
                ),
            ]
        } {
            let (mut c, rx, h) = server(vec![registry]);
            let e = run(&mut c, &MarketCommand::Swap(a)).await.unwrap_err();
            assert!(e.contains(error), "{e}");
            rx.recv().unwrap();
            h.join().unwrap();
        }
    }
    #[derive(clap::Parser)]
    struct Parser {
        #[command(subcommand)]
        command: MarketCommand,
    }
    fn quote_args() -> QuoteArgs {
        QuoteArgs {
            binance_chain_id: "56".into(),
            from_token: "0x1111111111111111111111111111111111111111".into(),
            to_token: "0x2222222222222222222222222222222222222222".into(),
            from_token_qty: "0001.00".into(),
            slippage: "auto".into(),
        }
    }
    #[test]
    fn clap_defaults_intent_and_chain_addresses() {
        use clap::Parser as _;
        let p = Parser::try_parse_from([
            "x",
            "swap",
            "--binanceChainId",
            "56",
            "--fromToken",
            "0x1111111111111111111111111111111111111111",
            "--toToken",
            "0x2222222222222222222222222222222222222222",
            "--fromTokenQty",
            "0001.00",
            "--acknowledge-token-risk",
        ])
        .unwrap();
        assert!(is_mutating(&p.command));
        let v = mutation_payload(&p.command).unwrap();
        assert_eq!(v["args"]["fromTokenQty"], "0001.00");
        assert_eq!(v["args"]["mev"], "true");
        assert_eq!(v["args"]["gasLevel"], "MEDIUM");
        assert_eq!(v["args"]["slippage"], "auto");
        assert!(address("56", "So11111111111111111111111111111111111111112").is_err());
        assert!(address("CT_501", "So11111111111111111111111111111111111111112").is_ok());
        assert!(address("1", "0x52908400098527886E0F7030069857D2E4169EE7").is_ok());
        assert!(address("1", "0x52908400098527886e0F7030069857D2E4169EE7").is_err());
        assert!(mutation_payload(&MarketCommand::Swap(SwapArgs {
            quote: quote_args(),
            mev: "junk".into(),
            gas_level: "LOW".into(),
            acknowledge_token_risk: true
        }))
        .is_err());
    }
    #[test]
    fn exact_decimal_and_slippage_validation() {
        for v in [
            "",
            "0",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1e2",
            ".1",
            "1.",
            "NaN",
            "1000000000000000000.000000000000000001",
        ] {
            assert!(amount(v).is_err(), "{v}");
        }
        for v in [
            "1",
            "0001.000",
            "0.00000000000000000000001",
            "1000000000000000000",
        ] {
            assert!(amount(v).is_ok(), "{v}");
        }
        assert_eq!(slippage("auto").unwrap(), "auto");
        assert_eq!(
            slippage("1.234567891234567891").unwrap(),
            "0.01234567891234567891"
        );
        assert_eq!(slippage("100").unwrap(), "1");
        assert_eq!(slippage("0").unwrap(), "0");
        assert!(slippage("100.000000000000000001").is_err());
        assert!(slippage("1junk").is_err());
    }
}
