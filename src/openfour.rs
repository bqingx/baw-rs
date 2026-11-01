//! In-process quote for four.meme / Open Four Agent ID exclusive bonding-curve buys.
//!
//! The official reference (`agent-id-exclusive-token-launches.mjs quote-buy`) is a Node
//! script: it starts a process, reads the curve through several JSON-RPC calls and then
//! simulates the transaction. No production latency is inferred from this implementation.
//!
//! This module reproduces only what is needed to build a valid buy: read the curve, ask
//! `Tools.estimateBuyByBudget` for the size, apply the slippage floor and encode
//! `Core.buyByBudget`. It does **not** simulate — `contract-call preview` simulates the
//! exact transaction on the backend afterwards, and a revert shows up there before anything
//! is signed.
//!
//! Everything here is a read: no credentials, no wallet session, no signing.
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub const PRESET_ID: u128 = 1778027615735;
const CHAIN_ID: &str = "56";

/// Reference endpoints, same list the official script uses, tried in order.
const DEFAULT_RPCS: [&str; 7] = [
    "https://bsc-dataseed.bnbchain.org",
    "https://bsc-dataseed-public.bnbchain.org",
    "https://bsc-dataseed.nariox.org",
    "https://bsc-dataseed.defibit.io",
    "https://bsc-dataseed.ninicoin.io",
    "https://bsc.nodereal.io",
    "https://rpc-bnb.blockmachine.io",
];

mod sel {
    pub const REGISTRY_CORE: &str = "0x7ba9413c";
    pub const REGISTRY_TOOLS: &str = "0xe75dfb3c";
    pub const TOKENS: &str = "0xe4860339";
    pub const IS_PRESET_ACTIVE: &str = "0x3c3ef33c";
    pub const TOTAL_RAISED: &str = "0xc5c4744c";
    pub const REMAINING_FOR_SALE: &str = "0x13e4930d";
    pub const PHASE: &str = "0xb1c9fe6e";
    pub const ESTIMATE_BUY_BY_BUDGET: &str = "0x1f8fcbf6";
    pub const BUY_BY_BUDGET: &str = "0x995ee6da";
}

const BPS: u128 = 10_000;

/// Parsed on its own, before the wallet CLI: a curve quote needs no wallet directory,
/// no instance identity and no session.
#[derive(clap::Parser, Debug, Clone)]
#[command(
    name = "openfour",
    about = "Open Four bonding-curve reads (no session needed)"
)]
pub struct Args {
    /// Accepted for symmetry with the rest of the CLI; this command always prints JSON.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: OpenFourCommand,
}

pub const NAMES: [&str; 2] = ["openfour", "four"];

#[derive(Subcommand, Debug, Clone)]
pub enum OpenFourCommand {
    /// Read the curve and build a `buyByBudget` transaction (no session, no signing).
    QuoteBuy(QuoteBuyArgs),
    /// Offline BNB/WBNB budget encoding, options=0; no quote or RPC.
    EncodeBuy(Buy),
}

/// Offline `buyByBudget` intent. No quote, no registry discovery and no session.
#[derive(clap::Args, Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Buy {
    #[arg(long)]
    pub core: String,
    #[arg(long)]
    pub token: String,
    #[arg(long = "wallet")]
    pub from: String,
    #[arg(long = "amount-wei")]
    pub budget: String,
    #[arg(long = "min-out-raw")]
    #[serde(rename = "minAmountOut")]
    pub min_amount_out: String,
    #[arg(long = "allow-zero-min-out")]
    #[serde(default, rename = "allowZeroMinOut")]
    pub allow_zero_min_out: bool,
}

/// Encode the exact `Core.buyByBudget` call for a fixed BNB/WBNB budget.
pub fn build(a: &Buy) -> Result<Value, String> {
    for address in [&a.core, &a.token, &a.from] {
        crate::signing::address("56", address)?;
        if address[2..].bytes().all(|b| b == b'0') {
            return Err("zero address is forbidden".into());
        }
    }
    let budget = crate::uint::uint256(&a.budget)?;
    let min = crate::uint::uint256(&a.min_amount_out)?;
    if min.bits() == 0 && !a.allow_zero_min_out {
        return Err("zero minAmountOut requires explicit allowZeroMinOut=true".into());
    }
    if budget.bits() == 0 {
        return Err("budget must be positive".into());
    }
    let input = encode_budget(&a.token, &budget, &min)?;
    Ok(
        json!({"binanceChainId":"56","from":a.from,"to":a.core,"value":budget.to_str_radix(10),"inputData":input}),
    )
}

#[derive(clap::Args, Debug, Clone)]
pub struct QuoteBuyArgs {
    /// Token contract address on BSC.
    #[arg(long)]
    pub token: String,
    /// Wallet address that will send the transaction.
    #[arg(long)]
    pub wallet: String,
    /// Budget in BNB, for example 0.0001.
    #[arg(long)]
    pub amount: String,
    /// Slippage tolerance in basis points (2000 = 20%).
    #[arg(long = "slippage-bps", default_value_t = 2000)]
    pub slippage_bps: u128,
    /// BSC JSON-RPC endpoint; defaults to the reference endpoint list.
    #[arg(long)]
    pub rpc: Option<String>,
}

/// Entry point used by the dispatcher.
pub async fn run(command: &OpenFourCommand) -> Result<Value, String> {
    match command {
        OpenFourCommand::EncodeBuy(a) => build(a),
        OpenFourCommand::QuoteBuy(a) => {
            let amount_wei = parse_units(&a.amount, 18)?;
            quote_buy(&QuoteBuy {
                token: a.token.clone(),
                wallet: a.wallet.clone(),
                amount_wei,
                slippage_bps: a.slippage_bps,
                rpc: a.rpc.clone().or_else(|| std::env::var("BSC_RPC_URL").ok()),
            })
            .await
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuoteBuy {
    pub token: String,
    pub wallet: String,
    /// Budget in wei (the most BNB that may be spent).
    pub amount_wei: u128,
    pub slippage_bps: u128,
    pub rpc: Option<String>,
}

#[derive(Serialize)]
pub struct Tx {
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "inputData")]
    pub input_data: String,
    #[serde(rename = "binanceChainId")]
    pub binance_chain_id: String,
}

/// Runs the quote and returns the same JSON envelope the rest of the CLI emits.
pub async fn quote_buy(args: &QuoteBuy) -> Result<Value, String> {
    let token = normalise_address(&args.token)?;
    let wallet = normalise_address(&args.wallet)?;
    if args.amount_wei == 0 {
        return Err("--amount-wei must be greater than zero".into());
    }
    if args.slippage_bps >= BPS {
        return Err("--slippage-bps must be below 10000".into());
    }
    let rpc = Rpc::connect(args.rpc.as_deref()).await?;

    // Registry first: Core and Tools are read from it so a governance upgrade is followed
    // instead of pinned here.
    let core = rpc.address_call(REGISTRY, sel::REGISTRY_CORE).await?;
    let tools = rpc.address_call(REGISTRY, sel::REGISTRY_TOOLS).await?;

    // One call for the token's Core record: preset, supply, quote asset, vault, paused.
    let config_hex = rpc
        .eth_call(&core, &format!("{}{}", sel::TOKENS, word_address(&token)))
        .await?;
    let config = words(&config_hex)?;
    if config.len() < 19 {
        return Err("Core.tokens returned an unexpected shape".into());
    }
    if config[17] != 1 {
        return Err("token is not managed by this Core".into());
    }
    let preset_id = config[2];
    let quote_asset = address_at(&config_hex, 9)?;
    let vault = address_at(&config_hex, 10)?;
    let paused = config[18] != 0;

    // The remaining reads are independent: one round trip.
    let preset_call = format!("{}{}", sel::IS_PRESET_ACTIVE, word_uint(preset_id));
    let (preset_hex, raised_hex, remaining_hex, phase_hex) = tokio::join!(
        rpc.eth_call(REGISTRY, &preset_call),
        rpc.eth_call(&vault, sel::TOTAL_RAISED),
        rpc.eth_call(&vault, sel::REMAINING_FOR_SALE),
        rpc.eth_call(&vault, sel::PHASE),
    );
    let preset_active = first_word(&preset_hex?)? != 0;
    let total_raised = first_word(&raised_hex?)?;
    let remaining = first_word(&remaining_hex?)?;
    let phase = first_word(&phase_hex?)?;

    if preset_id != PRESET_ID {
        return Err(format!(
            "preset {preset_id} is not the Agent ID exclusive preset ({PRESET_ID})"
        ));
    }
    if paused {
        return Err("token curve is paused".into());
    }
    if !preset_active {
        return Err("preset is inactive".into());
    }
    if phase != 1 {
        return Err(format!(
            "token is not in the Trading phase (phase {phase}); use market-order after migration"
        ));
    }
    if quote_asset.to_lowercase() != WBNB {
        return Err(format!(
            "quote asset {quote_asset} is not WBNB; only native BNB buys are supported here"
        ));
    }

    // Size the buy. estimateBuyByBudget is a view on Tools, not Core.
    let estimate_hex = rpc
        .eth_call(
            &tools,
            &format!(
                "{}{}",
                sel::ESTIMATE_BUY_BY_BUDGET,
                abi_args(
                    &[
                        Word::Address(&token),
                        Word::Address(&wallet),
                        Word::Uint(args.amount_wei),
                        Word::Uint(0)
                    ],
                    true
                )
            ),
        )
        .await?;
    let estimate = words(&estimate_hex)?;
    if estimate.len() < 5 {
        return Err("estimate returned an unexpected shape".into());
    }
    let (curve_quote, total_fee, user_pays, token_amount) =
        (estimate[0], estimate[1], estimate[2], estimate[4]);
    if token_amount == 0 {
        return Err("estimate returned zero tokens: gate, hold cap or budget too small".into());
    }

    let min_token_out = slip_down(token_amount, args.slippage_bps);
    let input_data = encode_budget(&token, &args.amount_wei.into(), &min_token_out.into())?;

    Ok(json!({
        "route": "contract-call",
        "mode": "budget",
        "token": token,
        "wallet": wallet,
        "contracts": {"core": core, "tools": tools, "vault": vault},
        "curve": {
            "phase": phase,
            "paused": paused,
            "presetActive": preset_active,
            "presetId": preset_id.to_string(),
            "totalRaisedRaw": total_raised.to_string(),
            "remainingForSaleRaw": remaining.to_string(),
        },
        "estimate": {
            "tokenAmountRaw": token_amount.to_string(),
            "curveQuoteRaw": curve_quote.to_string(),
            "totalFeeRaw": total_fee.to_string(),
            "userPaysRaw": user_pays.to_string(),
        },
        "slippageBps": args.slippage_bps,
        "minTokenOutRaw": min_token_out.to_string(),
        "tx": Tx {
            from: wallet.clone(),
            to: core.clone(),
            value: args.amount_wei.to_string(),
            input_data: input_data.clone(),
            binance_chain_id: CHAIN_ID.into(),
        },
        "previewCommand": format!(
            "baw-rs contract-call preview --binanceChainId 56 --from {wallet} --to {core} --value {} --inputData {input_data} --json",
            args.amount_wei
        ),
    }))
}

/// Shared, offline encoder for the known BNB/WBNB template: options=0, empty proof.
pub(crate) fn encode_budget(
    token: &str,
    budget: &num_bigint::BigUint,
    minimum: &num_bigint::BigUint,
) -> Result<String, String> {
    crate::signing::address(CHAIN_ID, token)?;
    if budget.bits() > 256 || minimum.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(format!(
        "{}{:0>64}{:0>64}{:0>64}{:064x}{:064x}{:064x}",
        sel::BUY_BY_BUDGET,
        token[2..].to_ascii_lowercase(),
        budget.to_str_radix(16),
        minimum.to_str_radix(16),
        0,
        160,
        0
    ))
}

const REGISTRY: &str = "0x912cef0c3ae9ab6eb3ec87cab69371cfb317ab94";
const WBNB: &str = "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c";

/// `x * (BPS - bps) / BPS`, floored, exactly as the reference script computes the floor.
fn slip_down(amount: u128, bps: u128) -> u128 {
    amount / BPS * (BPS - bps) + amount % BPS * (BPS - bps) / BPS
}

struct Rpc {
    http: reqwest::Client,
    url: String,
}

impl Rpc {
    async fn connect(url: Option<&str>) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "failed to create RPC client")?;
        let candidates: Vec<String> = match url {
            Some(u) => vec![u.to_string()],
            None => DEFAULT_RPCS.iter().map(|u| u.to_string()).collect(),
        };
        let mut last = String::from("no endpoint tried");
        for candidate in candidates {
            let probe = Self {
                http: http.clone(),
                url: candidate.clone(),
            };
            match probe.call("eth_blockNumber", json!([])).await {
                Ok(_) => return Ok(probe),
                Err(e) => last = format!("{candidate}: {e}"),
            }
        }
        Err(format!("no BSC RPC endpoint responded ({last})"))
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let response = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("{method} failed: {e}"))?;
        let value: Value = response
            .json()
            .await
            .map_err(|_| format!("{method} returned a non-JSON response"))?;
        if let Some(error) = value.get("error") {
            return Err(format!("{method} rejected: {error}"));
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("{method} returned no result"))
    }

    async fn eth_call(&self, to: &str, data: &str) -> Result<String, String> {
        let result = self
            .call("eth_call", json!([{"to": to, "data": data}, "latest"]))
            .await?;
        result
            .as_str()
            .map(|s| s.to_owned())
            .ok_or_else(|| "eth_call returned a non-string result".into())
    }

    async fn address_call(&self, to: &str, data: &str) -> Result<String, String> {
        let hex = self.eth_call(to, data).await?;
        address_at(&hex, 0)
    }
}

enum Word<'a> {
    Address(&'a str),
    Uint(u128),
}

/// Static arguments followed by a trailing empty `bytes` (the `proof` field).
fn abi_args(words_in: &[Word<'_>], empty_tail: bool) -> String {
    let mut out = String::new();
    for word in words_in {
        match word {
            Word::Address(a) => out.push_str(&word_address(a)),
            Word::Uint(u) => out.push_str(&word_uint(*u)),
        }
    }
    if empty_tail {
        // Offset to the empty bytes blob: 32 * (n + 1), then its zero length.
        out.push_str(&word_uint(32 * (words_in.len() as u128 + 1)));
        out.push_str(&word_uint(0));
    }
    out
}

fn word_address(a: &str) -> String {
    let bare = a.trim_start_matches("0x").to_lowercase();
    format!("{:0>64}", bare)
}

fn word_uint(u: u128) -> String {
    format!("{u:064x}")
}

/// Reads the 20-byte address stored in slot `index` of an ABI payload.
///
/// Skipping this and reusing the numeric parser would silently drop the top four bytes of
/// every address, because only the low 128 bits survive a `u128` parse.
fn address_at(hex: &str, index: usize) -> Result<String, String> {
    let bare = hex.strip_prefix("0x").unwrap_or(hex);
    let start = index * 64;
    let end = start + 64;
    if bare.len() < end {
        return Err("malformed ABI payload".into());
    }
    Ok(format!("0x{}", &bare[start + 24..end]))
}

fn words(hex: &str) -> Result<Vec<u128>, String> {
    let bare = hex.strip_prefix("0x").unwrap_or(hex);
    if bare.is_empty() || !bare.len().is_multiple_of(64) {
        return Err("malformed ABI payload".into());
    }
    bare.as_bytes()
        .chunks(64)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).map_err(|_| "malformed ABI payload")?;
            // Values wider than u128 cannot appear in the fields this module reads; keep the
            // low 128 bits rather than failing the whole quote.
            let start = text.len().saturating_sub(32);
            u128::from_str_radix(&text[start..], 16).map_err(|_| "malformed ABI word".into())
        })
        .collect()
}

fn first_word(hex: &str) -> Result<u128, String> {
    words(hex)?
        .into_iter()
        .next()
        .ok_or_else(|| "empty ABI payload".to_string())
}

fn normalise_address(value: &str) -> Result<String, String> {
    let bare = value.strip_prefix("0x").unwrap_or(value);
    if bare.len() != 40 || !bare.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected a 20-byte hex address".into());
    }
    Ok(format!("0x{}", bare.to_lowercase()))
}

/// Decimal string in whole units to base units, without floating point.
pub fn parse_units(value: &str, decimals: u32) -> Result<u128, String> {
    let (whole, frac) = match value.split_once('.') {
        Some((w, f)) => (w, f),
        None => (value, ""),
    };
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err("amount must be a decimal number".into());
    }
    if frac.len() > decimals as usize {
        return Err(format!("amount has more than {decimals} decimals"));
    }
    let scale = 10u128
        .checked_pow(decimals)
        .ok_or("decimal scale overflow")?;
    let whole: u128 = whole.parse().map_err(|_| "amount is too large")?;
    let mut frac_padded = frac.to_string();
    while frac_padded.len() < decimals as usize {
        frac_padded.push('0');
    }
    let frac: u128 = if frac_padded.is_empty() {
        0
    } else {
        frac_padded.parse().map_err(|_| "amount is too large")?
    };
    whole
        .checked_mul(scale)
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(|| "amount is too large".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from the reference script on BSC mainnet (0.001 BNB budget buy).
    const REFERENCE_INPUT: &str = "0x995ee6da0000000000000000000000006ff638d7bc89cac3567b99cafeaf13f18c51ffff00000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000234fb471955f0cd7ab2c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000";

    fn buy() -> Buy {
        serde_json::from_value(json!({"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"1","minAmountOut":"0","allowZeroMinOut":true})).unwrap()
    }

    #[test]
    fn zero_minimum_requires_separate_explicit_opt_in() {
        let mut v = serde_json::to_value(buy()).unwrap();
        v.as_object_mut().unwrap().remove("allowZeroMinOut");
        let a: Buy = serde_json::from_value(v.clone()).unwrap();
        assert!(
            build(&a).is_err(),
            "zero minimum must not silently disable price protection"
        );
        v["allowZeroMinOut"] = json!(true);
        assert!(build(&serde_json::from_value(v).unwrap()).is_ok());
    }

    #[test]
    fn full_width_boundaries_and_strict_schema() {
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        let mut a = buy();
        a.budget = max.into();
        a.min_amount_out = format!("0x{}", "f".repeat(64));
        let tx = build(&a).unwrap();
        let data = tx["inputData"].as_str().unwrap();
        assert_eq!(&data[74..138], "f".repeat(64));
        assert_eq!(&data[138..202], "f".repeat(64));
        assert_eq!(&data[202..266], "0".repeat(64));
        assert_eq!(&data[266..330], format!("{:064x}", 160));
        assert_eq!(&data[330..394], "0".repeat(64));
        for bad in [
            "",
            "-1",
            "+1",
            " 1",
            "1.0",
            "1e18",
            "0x",
            "0X1",
            "１２",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            a.min_amount_out = bad.into();
            assert!(build(&a).is_err(), "{bad}");
        }
        a = buy();
        a.budget = "0".into();
        assert!(build(&a).is_err());
        for bad in [
            "0x0000000000000000000000000000000000000000",
            "0x111",
            "0xZZ11111111111111111111111111111111111111",
            " 0x1111111111111111111111111111111111111111",
        ] {
            a = buy();
            a.core = bad.into();
            assert!(build(&a).is_err());
        }
        let mut v = serde_json::to_value(buy()).unwrap();
        v.as_object_mut().unwrap().remove("minAmountOut");
        assert!(serde_json::from_value::<Buy>(v).is_err());
    }

    #[test]
    fn encodes_buy_by_budget_exactly_like_the_reference() {
        let encoded = format!(
            "{}{}",
            sel::BUY_BY_BUDGET,
            abi_args(
                &[
                    Word::Address("0x6ff638d7bc89cac3567b99cafeaf13f18c51ffff"),
                    Word::Uint(1_000_000_000_000_000),
                    Word::Uint(166_753_122_020_095_302_282_028),
                    Word::Uint(0),
                ],
                true
            )
        );
        assert_eq!(encoded, REFERENCE_INPUT);
    }

    #[test]
    fn slippage_floor_matches_the_reference_numbers() {
        // Reference: 208441.402525119127852535 tokens with 20% slippage
        // -> minTokenOut 166753.122020095302282028.
        assert_eq!(
            slip_down(208_441_402_525_119_127_852_535, 2000),
            166_753_122_020_095_302_282_028
        );
        // No slippage keeps the amount; full slippage floors to zero.
        assert_eq!(slip_down(12345, 0), 12345);
        assert_eq!(slip_down(12345, 10_000 - 1), 1);
    }

    #[test]
    fn parses_decimal_amounts_without_floating_point() {
        assert_eq!(parse_units("0.0001", 18).unwrap(), 100_000_000_000_000);
        assert_eq!(parse_units("1", 18).unwrap(), 1_000_000_000_000_000_000);
        assert_eq!(parse_units("0.5", 6).unwrap(), 500_000);
        assert!(parse_units("0.0000000000000000001", 18).is_err());
        assert!(parse_units("abc", 18).is_err());
    }

    #[test]
    fn reads_abi_words_and_addresses() {
        let hex = format!(
            "{}{}",
            word_uint(7),
            word_address("0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c")
        );
        let parsed = words(&hex).unwrap();
        assert_eq!(parsed[0], 7);
        // The address must survive intact, including its top four bytes (0xbb4cdb9c).
        assert_eq!(
            address_at(&hex, 1).unwrap(),
            "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c"
        );
        assert!(words("0x").is_err());
        assert!(words("0x12").is_err());
    }
}
