//! OKX-only market command grammar and Binance wallet transfers.
//! Binance is the wallet backend, never a market quote/swap provider.
pub mod approvals;
pub(crate) mod intent;
pub(crate) mod okx;
pub mod route_action;
pub mod wallet_ops;
use crate::core::client::{checksum_address, Client};
use crate::query::rwa;
use bigdecimal::BigDecimal;
use clap::{Args, Subcommand, ValueEnum};
pub(crate) use intent::intent_summary;
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};
use std::str::FromStr;

/// Pure local CLI entry point; no wallet, configuration, or authorization access.
pub fn local_summary(command: &MarketCommand) -> Option<Result<Value, String>> {
    match command {
        MarketCommand::Intent(args) => Some(intent_summary(args)),
        _ => None,
    }
}

const A: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet";

fn session(c: &Client) -> Result<(), String> {
    if c.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}
pub(crate) async fn safeguards(c: &mut Client, chain: &str) -> Result<(), String> {
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
    sell_amount(
        shares,
        multiplier,
        row["balance"]
            .as_str()
            .ok_or("missing exact wallet balance")?,
        row["decimals"]
            .as_i64()
            .ok_or("missing wallet token decimals")?,
    )
}
#[derive(Debug, Clone, Subcommand)]
pub enum MarketCommand {
    Quote(QuoteArgs),
    Swap(SwapArgs),
    /// Local intent only: no quote, simulation, session, or execution authorization.
    Intent(SwapArgs),
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Okx,
}
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteArgs {
    /// OKX is the only trade provider. No fallback.
    #[arg(long, value_enum, default_value = "okx")]
    pub provider: Provider,
    /// Private OKX profile (0600).
    #[arg(long)]
    pub okx_config: Option<std::path::PathBuf>,
    #[arg(long = "fromTokenDecimals")]
    pub from_token_decimals: Option<u8>,
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long = "fromToken")]
    pub from_token: String,
    #[arg(long = "toToken")]
    pub to_token: String,
    #[arg(long = "fromTokenQty")]
    pub from_token_qty: String,
    /// Explicit slippage percentage; auto is not supported. Never increased for token tax.
    #[arg(long)]
    pub slippage: String,
    /// Explicit input transfer-tax metadata cap in percent. Zero keeps tax tokens disabled.
    #[arg(long, default_value = "0")]
    pub max_from_tax_percent: String,
    /// Explicit output transfer-tax metadata cap in percent. Net wallet minimum still applies.
    #[arg(long, default_value = "0")]
    pub max_to_tax_percent: String,
}
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapArgs {
    #[command(flatten)]
    #[serde(flatten)]
    pub quote: QuoteArgs,
    /// Must be explicit. Protected submission is not yet supported by the signing backend.
    #[arg(long)]
    pub mev: String,
    #[arg(long = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(long)]
    pub acknowledge_token_risk: bool,
    /// Optional fixed net-output floor in base units. Omit to use fresh OKX/slippage minima.
    #[arg(long)]
    pub okx_min_output_units: Option<String>,
    #[arg(long)]
    pub auto_approve: bool,
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
        Err(e) => Ok(
            json!({"status":"UNCERTAIN","retrySafe":false,"cause":e,"message":"transfer outcome unknown; reconcile transaction history; never automatically resubmit"}),
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
fn gas(s: &str) -> Result<String, String> {
    let v = s.to_ascii_uppercase();
    if matches!(v.as_str(), "LOW" | "MEDIUM" | "HIGH") {
        Ok(v)
    } else {
        Err("gasLevel must be LOW, MEDIUM, or HIGH".into())
    }
}
pub fn is_mutating(c: &MarketCommand) -> bool {
    matches!(c, MarketCommand::Swap(_))
}
pub fn mutation_payload(c: &MarketCommand) -> Result<Value, String> {
    match c {
        MarketCommand::Swap(a) => {
            crate::trade::okx::validate_quote_options(&a.quote)?;
            crate::trade::okx::validate_swap_options(a)?;
            gas(&a.gas_level)?;
            if !matches!(a.mev.as_str(), "true" | "false") {
                return Err("mev must be true or false".into());
            }
            Ok(
                json!({"operation":"market-order.swap","args":a,"calldataPolicy":crate::trade::route_action::CALLDATA_POLICY}),
            )
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
#[cfg(test)]
mod transfer_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimals_and_transfer_rwa_rounding_remain_lossless() {
        let value: Value = serde_json::from_str("9007199254740993.123456789123456789").unwrap();
        assert_eq!(
            backend_decimal(&value).unwrap().to_plain_string(),
            "9007199254740993.123456789123456789"
        );
        assert_eq!(sell_amount("1", "3", "1", 2).unwrap(), "0.34");
        assert!(sell_amount("4", "3", "1", 2).is_err());
        assert_eq!(sell_amount("1", "1", "1", 18).unwrap(), "1");
    }
    #[test]
    fn transfer_payload_rejects_ambiguous_amount_and_invalid_gas() {
        let mut a = SendArgs {
            binance_chain_id: "56".into(),
            amount: Some("1".into()),
            max: false,
            recipient: "0x1111111111111111111111111111111111111111".into(),
            token_address: "0x0000000000000000000000000000000000000000".into(),
            gas_level: "HIGH".into(),
        };
        assert!(send_payload(&a).is_ok());
        a.max = true;
        assert!(send_payload(&a).is_err());
        a.max = false;
        a.gas_level = "FASTEST".into();
        assert!(send_payload(&a).is_err());
        assert_eq!(Provider::default(), Provider::Okx);
    }
}
