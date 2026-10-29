//! DeFi: protocols, opportunities, positions, deposit/redeem/LP/claim. Mirrors `baw defi`.
//!
//! Leaf grammar, defaults and validation messages come from the installed CLI's own
//! `--help` (docs/JS-CLI-PARITY.md) and from the bundle's commander actions; the endpoints
//! and body field names come from `@binance/agentic-wallet/dist/index.js`.
use crate::client::{checksum_address, Client, API};
use bigdecimal::BigDecimal;
use clap::Args;
use reqwest::Method;
use serde_json::{json, Map, Value};
use std::str::FromStr;

const GAS_LEVELS: [&str; 3] = ["LOW", "MEDIUM", "HIGH"];
const CLAIM_TYPES: [&str; 4] = [
    "REWARD_PROTOCOL",
    "REWARD_INVESTMENT",
    "LP_FEE",
    "REDEMPTION",
];
const PREVIEW_ACTIONS: [&str; 5] = ["deposit", "redeem", "lp-add", "lp-remove", "claim"];

#[derive(clap::Subcommand, Debug)]
pub enum DefiCommand {
    /// List DeFi protocols with TVL and APY
    ProtocolList(ProtocolListArgs),
    /// Get detailed information about a DeFi protocol
    ProtocolInfo(ProtocolInfoArgs),
    /// List DeFi investment opportunities
    InvestmentList(InvestmentListArgs),
    /// Get detailed information about a DeFi investment
    InvestmentInfo(InvestmentInfoArgs),
    /// Query your DeFi positions across protocols
    Position(PositionArgs),
    /// Deposit / stake / supply assets to a DeFi protocol
    Deposit(DepositArgs),
    /// Redeem / unstake assets from a DeFi protocol
    Redeem(RedeemArgs),
    /// Add liquidity to an LP position (create new or top up existing)
    LpAdd(LpAddArgs),
    /// Remove liquidity from an LP position
    LpRemove(LpRemoveArgs),
    /// Claim LP fees, protocol/investment rewards, or matured redemptions
    Claim(ClaimArgs),
    /// Preview a DeFi transaction (does not broadcast)
    Preview(PreviewArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ProtocolListArgs {
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long = "investType", value_name = "investType")]
    pub invest_type: Option<String>,
    #[arg(long = "sortField", value_name = "sortField")]
    pub sort_field: Option<String>,
    #[arg(long = "sortDirection", value_name = "sortDirection")]
    pub sort_direction: Option<String>,
    #[arg(long, value_name = "page")]
    pub page: Option<u32>,
    #[arg(long, value_name = "size")]
    pub size: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct ProtocolInfoArgs {
    #[arg(long = "defiProtocolId", value_name = "defiProtocolId")]
    pub defi_protocol_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct InvestmentListArgs {
    #[arg(long = "investType", value_name = "investType")]
    pub invest_type: String,
    #[arg(long = "defiProtocolId", value_name = "defiProtocolId")]
    pub defi_protocol_id: Option<String>,
    #[arg(long = "contractAddresses", value_name = "addresses")]
    pub contract_addresses: Option<String>,
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long = "sortField", value_name = "sortField")]
    pub sort_field: Option<String>,
    #[arg(long = "sortDirection", value_name = "sortDirection")]
    pub sort_direction: Option<String>,
    #[arg(long, value_name = "page")]
    pub page: Option<u32>,
    #[arg(long, value_name = "size")]
    pub size: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct InvestmentInfoArgs {
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct PositionArgs {
    #[arg(long, value_name = "address")]
    pub address: Option<String>,
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long = "defiProtocolId", value_name = "defiProtocolId")]
    pub defi_protocol_id: Option<String>,
    /// Force-refresh positions (skip cache, rate-limited)
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Args, Debug, Clone)]
pub struct DepositArgs {
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: String,
    #[arg(long = "tokenAddress", value_name = "tokenAddress")]
    pub token_address: String,
    #[arg(long, value_name = "amount")]
    pub amount: String,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct RedeemArgs {
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: String,
    #[arg(long = "tokenAddress", value_name = "tokenAddress")]
    pub token_address: String,
    /// Redeem amount (human-readable); mutually exclusive with --ratio
    #[arg(long, value_name = "amount")]
    pub amount: Option<String>,
    /// Redeem ratio (0, 1]; mutually exclusive with --amount
    #[arg(long, value_name = "ratio")]
    pub ratio: Option<String>,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct LpAddArgs {
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: String,
    #[arg(long = "tokenAddress", value_name = "tokenAddress")]
    pub token_address: String,
    #[arg(long, value_name = "amount")]
    pub amount: String,
    /// V3/V4 LP NFT tokenId; use to top up an existing position
    #[arg(long = "nftId", value_name = "nftId")]
    pub nft_id: Option<String>,
    /// Lower tick of the price range (raw int24, aligned to pool tickSpacing)
    #[arg(
        long = "tickLower",
        value_name = "tickLower",
        allow_hyphen_values = true
    )]
    pub tick_lower: Option<String>,
    /// Upper tick of the price range (raw int24, aligned to pool tickSpacing)
    #[arg(
        long = "tickUpper",
        value_name = "tickUpper",
        allow_hyphen_values = true
    )]
    pub tick_upper: Option<String>,
    /// Price range as a percentage of the current pool price (e.g. "5" for ±5%)
    #[arg(long = "priceRange", value_name = "percent")]
    pub price_range: Option<String>,
    #[arg(
        long = "slippageBps",
        value_name = "slippageBps",
        default_value = "auto"
    )]
    pub slippage_bps: String,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct LpRemoveArgs {
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: String,
    #[arg(long = "nftId", value_name = "nftId")]
    pub nft_id: String,
    /// Removal ratio (0, 1]; "1" removes the full position
    #[arg(long, value_name = "ratio")]
    pub ratio: String,
    #[arg(
        long = "slippageBps",
        value_name = "slippageBps",
        default_value = "auto"
    )]
    pub slippage_bps: String,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct ClaimArgs {
    /// Claim type: REWARD_PROTOCOL / REWARD_INVESTMENT / LP_FEE / REDEMPTION
    #[arg(long = "claimType", value_name = "claimType")]
    pub claim_type: String,
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: Option<String>,
    /// DeFi protocol ID (required for REWARD_PROTOCOL)
    #[arg(long = "defiProtocolId", value_name = "defiProtocolId")]
    pub defi_protocol_id: Option<String>,
    /// LP NFT tokenId (required for LP_FEE)
    #[arg(long = "nftId", value_name = "nftId")]
    pub nft_id: Option<String>,
    /// Redemption record ID (required for REDEMPTION)
    #[arg(long = "redemptionId", value_name = "redemptionId")]
    pub redemption_id: Option<String>,
    /// Optional token address to limit the claim
    #[arg(long = "tokenAddress", value_name = "tokenAddress")]
    pub token_address: Option<String>,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

#[derive(Args, Debug, Clone)]
pub struct PreviewArgs {
    /// Action: deposit / redeem / lp-add / lp-remove / claim
    #[arg(long = "action", value_name = "action")]
    pub action: String,
    #[arg(long = "investmentId", value_name = "investmentId")]
    pub investment_id: Option<String>,
    #[arg(long = "tokenAddress", value_name = "tokenAddress")]
    pub token_address: Option<String>,
    #[arg(long, value_name = "amount")]
    pub amount: Option<String>,
    /// Ratio (0, 1] (redeem / lp-remove)
    #[arg(long, value_name = "ratio")]
    pub ratio: Option<String>,
    #[arg(long = "nftId", value_name = "nftId")]
    pub nft_id: Option<String>,
    /// Lower tick of LP price range (lp-add only)
    #[arg(
        long = "tickLower",
        value_name = "tickLower",
        allow_hyphen_values = true
    )]
    pub tick_lower: Option<String>,
    /// Upper tick of LP price range (lp-add only)
    #[arg(
        long = "tickUpper",
        value_name = "tickUpper",
        allow_hyphen_values = true
    )]
    pub tick_upper: Option<String>,
    /// LP price range percentage, e.g. "5" for ±5%
    #[arg(long = "priceRange", value_name = "percent")]
    pub price_range: Option<String>,
    /// Slippage in basis points: "auto" or 1-4999
    #[arg(long = "slippageBps", value_name = "slippageBps")]
    pub slippage_bps: Option<String>,
    /// Claim type (claim only)
    #[arg(long = "claimType", value_name = "claimType")]
    pub claim_type: Option<String>,
    /// DeFi protocol ID (claim REWARD_PROTOCOL)
    #[arg(long = "defiProtocolId", value_name = "defiProtocolId")]
    pub defi_protocol_id: Option<String>,
    /// Redemption record ID (claim REDEMPTION)
    #[arg(long = "redemptionId", value_name = "redemptionId")]
    pub redemption_id: Option<String>,
    #[arg(long = "gasLevel", value_name = "gasLevel", default_value = "MEDIUM")]
    pub gas_level: String,
    #[arg(
        long = "binanceChainId",
        value_name = "binanceChainId",
        default_value = "56"
    )]
    pub binance_chain_id: String,
}

pub fn is_mutation(command: &DefiCommand) -> bool {
    matches!(
        command,
        DefiCommand::Deposit(_)
            | DefiCommand::Redeem(_)
            | DefiCommand::LpAdd(_)
            | DefiCommand::LpRemove(_)
            | DefiCommand::Claim(_)
    )
}

/// Deterministic intent digest input for the durable journal. Carries the operation
/// discriminator plus the identifiers/amounts that are about to be sent; never a timestamp,
/// backend value or display text.
pub fn mutation_payload(command: &DefiCommand) -> Result<Value, String> {
    if !is_mutation(command) {
        return Err("not a mutation".into());
    }
    let planned = plan(command)?;
    Ok(json!({
        "operation": format!("defi.{}", planned.leaf),
        "args": planned.body,
    }))
}

struct Planned {
    leaf: &'static str,
    path: String,
    body: Value,
}

impl Planned {
    fn open(leaf: &'static str, tail: &str, body: Value) -> Self {
        Self {
            leaf,
            path: format!("{API}/defi/open/{tail}"),
            body,
        }
    }
    fn trade(leaf: &'static str, tail: &str, body: Value) -> Self {
        Self {
            leaf,
            path: format!("{API}/defi/{tail}"),
            body,
        }
    }
}

/// Builds the exact path and body the JS CLI sends. Validation runs first so a rejected
/// leaf never touches the network. `undefined` body members are dropped, matching
/// `JSON.stringify` on the commander result object.
fn plan(command: &DefiCommand) -> Result<Planned, String> {
    Ok(match command {
        // async protocolList(e){return this.request(`${t.OPEN_PREFIX}/protocol/list`,{method:"POST",body:e??{}})}
        DefiCommand::ProtocolList(a) => Planned::open(
            "protocol-list",
            "protocol/list",
            object(&[
                ("binanceChainId", text(&a.binance_chain_id)),
                ("investType", text(&a.invest_type)),
                ("sortField", text(&a.sort_field)),
                ("sortDirection", text(&a.sort_direction)),
                ("page", a.page.map(|value| json!(value))),
                ("size", a.size.map(|value| json!(value))),
            ]),
        ),
        // async protocolInfo(e){return this.request(`${t.OPEN_PREFIX}/protocol/info`,{method:"POST",body:{defiProtocolId:e}})}
        DefiCommand::ProtocolInfo(a) => Planned::open(
            "protocol-info",
            "protocol/info",
            object(&[("defiProtocolId", Some(json!(a.defi_protocol_id)))]),
        ),
        // async investmentList(e){return this.request(`${t.OPEN_PREFIX}/investment/list`,{method:"POST",body:e})}
        DefiCommand::InvestmentList(a) => Planned::open(
            "investment-list",
            "investment/list",
            object(&[
                ("investType", Some(json!(a.invest_type))),
                ("defiProtocolId", text(&a.defi_protocol_id)),
                ("contractAddresses", text(&a.contract_addresses)),
                ("binanceChainId", text(&a.binance_chain_id)),
                ("sortField", text(&a.sort_field)),
                ("sortDirection", text(&a.sort_direction)),
                ("page", a.page.map(|value| json!(value))),
                ("size", a.size.map(|value| json!(value))),
            ]),
        ),
        // async investmentInfo(e){return this.request(`${t.OPEN_PREFIX}/investment/info`,{method:"POST",body:{investmentId:e}})}
        DefiCommand::InvestmentInfo(a) => Planned::open(
            "investment-info",
            "investment/info",
            object(&[("investmentId", Some(json!(a.investment_id)))]),
        ),
        // position(e){...`${t.OPEN_PREFIX}/position`...} positionRefresh(e){...`${t.OPEN_PREFIX}/position/refresh`...}
        DefiCommand::Position(a) => {
            check_address(
                a.address.as_deref(),
                a.binance_chain_id.as_deref(),
                "address",
            )?;
            let body = object(&[
                ("address", text(&a.address)),
                ("binanceChainId", text(&a.binance_chain_id)),
                ("defiProtocolId", text(&a.defi_protocol_id)),
            ]);
            if a.refresh {
                Planned::open("position", "position/refresh", body)
            } else {
                Planned::open("position", "position", body)
            }
        }
        // async deposit(e){return this.request(`${t.TRADE_PREFIX}/deposit`,{method:"POST",body:e})}
        DefiCommand::Deposit(a) => {
            valid_amount(&a.amount)?;
            check_address(
                Some(&a.token_address),
                Some(&a.binance_chain_id),
                "tokenAddress",
            )?;
            let gas = gas_level(&a.gas_level)?;
            Planned::trade(
                "deposit",
                "deposit",
                object(&[
                    ("binanceChainId", Some(json!(a.binance_chain_id))),
                    ("investmentId", Some(json!(a.investment_id))),
                    ("tokenAddress", Some(json!(a.token_address))),
                    ("amount", Some(json!(a.amount))),
                    ("gasLevel", Some(json!(gas))),
                ]),
            )
        }
        // async redeem(e){return this.request(`${t.TRADE_PREFIX}/redeem`,{method:"POST",body:e})}
        DefiCommand::Redeem(a) => {
            if a.amount.is_some() == a.ratio.is_some() {
                return Err("Please specify either --amount or --ratio, but not both.".into());
            }
            check_address(
                Some(&a.token_address),
                Some(&a.binance_chain_id),
                "tokenAddress",
            )?;
            let mut amount = None;
            let mut ratio = None;
            if let Some(value) = &a.amount {
                valid_amount(value)?;
                amount = Some(json!(value));
            }
            if let Some(value) = &a.ratio {
                valid_ratio(value)?;
                ratio = Some(json!(value));
            }
            let gas = gas_level(&a.gas_level)?;
            Planned::trade(
                "redeem",
                "redeem",
                object(&[
                    ("binanceChainId", Some(json!(a.binance_chain_id))),
                    ("investmentId", Some(json!(a.investment_id))),
                    ("tokenAddress", Some(json!(a.token_address))),
                    ("amount", amount),
                    ("ratio", ratio),
                    ("gasLevel", Some(json!(gas))),
                ]),
            )
        }
        // async lpAdd(e){return this.request(`${t.TRADE_PREFIX}/lp-add`,{method:"POST",body:e})}
        DefiCommand::LpAdd(a) => {
            valid_amount(&a.amount)?;
            check_address(
                Some(&a.token_address),
                Some(&a.binance_chain_id),
                "tokenAddress",
            )?;
            let slippage = slippage_bps(&a.slippage_bps)?;
            let gas = gas_level(&a.gas_level)?;
            let source = lp_source(
                a.nft_id.as_deref(),
                a.price_range.as_deref(),
                a.tick_lower.as_deref(),
                a.tick_upper.as_deref(),
            )?;
            validate_lp_source(&source, &a.price_range, &a.tick_lower, &a.tick_upper)?;
            Planned::trade(
                "lp-add",
                "lp-add",
                object(&[
                    ("binanceChainId", Some(json!(a.binance_chain_id))),
                    ("investmentId", Some(json!(a.investment_id))),
                    ("tokenAddress", Some(json!(a.token_address))),
                    ("amount", Some(json!(a.amount))),
                    ("nftId", text(&a.nft_id)),
                    ("tickLower", text(&a.tick_lower)),
                    ("tickUpper", text(&a.tick_upper)),
                    ("priceRange", text(&a.price_range)),
                    ("slippageBps", Some(json!(slippage))),
                    ("gasLevel", Some(json!(gas))),
                ]),
            )
        }
        // async lpRemove(e){return this.request(`${t.TRADE_PREFIX}/lp-remove`,{method:"POST",body:e})}
        DefiCommand::LpRemove(a) => {
            valid_ratio(&a.ratio)?;
            let slippage = slippage_bps(&a.slippage_bps)?;
            let gas = gas_level(&a.gas_level)?;
            Planned::trade(
                "lp-remove",
                "lp-remove",
                object(&[
                    ("binanceChainId", Some(json!(a.binance_chain_id))),
                    ("investmentId", Some(json!(a.investment_id))),
                    ("nftId", Some(json!(a.nft_id))),
                    ("ratio", Some(json!(a.ratio))),
                    ("slippageBps", Some(json!(slippage))),
                    ("gasLevel", Some(json!(gas))),
                ]),
            )
        }
        // async claim(e){return this.request(`${t.TRADE_PREFIX}/claim`,{method:"POST",body:e})}
        // var dl={REWARD_PROTOCOL:["defiProtocolId"],REWARD_INVESTMENT:["investmentId"],
        //         LP_FEE:["investmentId","nftId"],REDEMPTION:["investmentId","redemptionId"]}
        DefiCommand::Claim(a) => {
            let claim_type = claim_type(&a.claim_type)?;
            let fields = [
                ("investmentId", &a.investment_id),
                ("defiProtocolId", &a.defi_protocol_id),
                ("nftId", &a.nft_id),
                ("redemptionId", &a.redemption_id),
                ("tokenAddress", &a.token_address),
            ];
            if let Some(message) = missing_claim_params(&claim_type, &fields) {
                return Err(message);
            }
            check_address(
                a.token_address.as_deref(),
                Some(&a.binance_chain_id),
                "tokenAddress",
            )?;
            let gas = gas_level(&a.gas_level)?;
            Planned::trade(
                "claim",
                "claim",
                object(&[
                    ("binanceChainId", Some(json!(a.binance_chain_id))),
                    ("claimType", Some(json!(claim_type))),
                    ("investmentId", text(&a.investment_id)),
                    ("defiProtocolId", text(&a.defi_protocol_id)),
                    ("nftId", text(&a.nft_id)),
                    ("redemptionId", text(&a.redemption_id)),
                    ("tokenAddress", text(&a.token_address)),
                    ("gasLevel", Some(json!(gas))),
                ]),
            )
        }
        // async preview(e){return this.request(`${t.TRADE_PREFIX}/preview`,{method:"POST",body:e})}
        DefiCommand::Preview(a) => Planned::trade("preview", "preview", preview_body(a)?),
    })
}

fn preview_body(a: &PreviewArgs) -> Result<Value, String> {
    let action = preview_action(&a.action)?;
    let gas = gas_level(&a.gas_level)?;
    let chain = a.binance_chain_id.as_str();
    let mut pairs: Vec<(&str, Option<Value>)> = vec![
        ("action", Some(json!(action))),
        ("binanceChainId", Some(json!(chain))),
        ("gasLevel", Some(json!(gas))),
    ];
    match action.as_str() {
        "deposit" => {
            let investment = preview_required(&a.investment_id, "investmentId", &action)?;
            let token = preview_required(&a.token_address, "tokenAddress", &action)?;
            let amount = preview_required(&a.amount, "amount", &action)?;
            valid_amount(amount)?;
            check_address(Some(token), Some(chain), "tokenAddress")?;
            pairs.push(("investmentId", Some(json!(investment))));
            pairs.push(("tokenAddress", Some(json!(token))));
            pairs.push(("amount", Some(json!(amount))));
        }
        "redeem" => {
            let investment = preview_required(&a.investment_id, "investmentId", &action)?;
            let token = preview_required(&a.token_address, "tokenAddress", &action)?;
            check_address(Some(token), Some(chain), "tokenAddress")?;
            if a.amount.is_some() == a.ratio.is_some() {
                return Err("Please specify either --amount or --ratio, but not both.".into());
            }
            pairs.push(("investmentId", Some(json!(investment))));
            pairs.push(("tokenAddress", Some(json!(token))));
            if let Some(amount) = &a.amount {
                valid_amount(amount)?;
                pairs.push(("amount", Some(json!(amount))));
            }
            if let Some(ratio) = &a.ratio {
                valid_ratio(ratio)?;
                pairs.push(("ratio", Some(json!(ratio))));
            }
        }
        "lp-add" => {
            let investment = preview_required(&a.investment_id, "investmentId", &action)?;
            let token = preview_required(&a.token_address, "tokenAddress", &action)?;
            let amount = preview_required(&a.amount, "amount", &action)?;
            valid_amount(amount)?;
            check_address(Some(token), Some(chain), "tokenAddress")?;
            pairs.push(("investmentId", Some(json!(investment))));
            pairs.push(("tokenAddress", Some(json!(token))));
            pairs.push(("amount", Some(json!(amount))));
            if let Some(slippage) = &a.slippage_bps {
                pairs.push(("slippageBps", Some(json!(slippage_bps(slippage)?))));
            }
            if let Some(nft) = &a.nft_id {
                pairs.push(("nftId", Some(json!(nft))));
            }
            if let Some(lower) = &a.tick_lower {
                pairs.push(("tickLower", Some(json!(lower))));
            }
            if let Some(upper) = &a.tick_upper {
                pairs.push(("tickUpper", Some(json!(upper))));
            }
            if let Some(range) = &a.price_range {
                pairs.push(("priceRange", Some(json!(range))));
            }
            let source = lp_source(
                a.nft_id.as_deref(),
                a.price_range.as_deref(),
                a.tick_lower.as_deref(),
                a.tick_upper.as_deref(),
            )?;
            validate_lp_source(&source, &a.price_range, &a.tick_lower, &a.tick_upper)?;
        }
        "lp-remove" => {
            let investment = preview_required(&a.investment_id, "investmentId", &action)?;
            let nft = preview_required(&a.nft_id, "nftId", &action)?;
            let ratio = preview_required(&a.ratio, "ratio", &action)?;
            valid_ratio(ratio)?;
            pairs.push(("investmentId", Some(json!(investment))));
            pairs.push(("nftId", Some(json!(nft))));
            pairs.push(("ratio", Some(json!(ratio))));
            if let Some(slippage) = &a.slippage_bps {
                pairs.push(("slippageBps", Some(json!(slippage_bps(slippage)?))));
            }
        }
        "claim" => {
            let claim_type = match &a.claim_type {
                Some(value) => claim_type(value)?,
                None => return Err("claimType is required.".into()),
            };
            pairs.push(("claimType", Some(json!(claim_type))));
            for (key, value) in [
                ("investmentId", &a.investment_id),
                ("defiProtocolId", &a.defi_protocol_id),
                ("nftId", &a.nft_id),
                ("redemptionId", &a.redemption_id),
                ("tokenAddress", &a.token_address),
            ] {
                if let Some(value) = value {
                    pairs.push((key, Some(json!(value))));
                }
            }
            check_address(a.token_address.as_deref(), Some(chain), "tokenAddress")?;
            let fields = [
                ("investmentId", &a.investment_id),
                ("defiProtocolId", &a.defi_protocol_id),
                ("nftId", &a.nft_id),
                ("redemptionId", &a.redemption_id),
                ("tokenAddress", &a.token_address),
            ];
            if let Some(message) = missing_claim_params(&claim_type, &fields) {
                return Err(message);
            }
        }
        _ => {
            return Err(
                "Invalid action; allowed: deposit, redeem, lp-add, lp-remove, claim.".into(),
            )
        }
    }
    Ok(object(&pairs))
}

/// Fixed order, `None` members dropped: the bundle passes a JS object whose `undefined`
/// members never reach the wire.
fn object(pairs: &[(&str, Option<Value>)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        if let Some(value) = value {
            map.insert((*key).into(), value.clone());
        }
    }
    Value::Object(map)
}

fn text(value: &Option<String>) -> Option<Value> {
    value.as_deref().map(|value| json!(value))
}

fn present(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|value| !value.is_empty())
}

/// Plain non-negative decimal (`^\d+(\.\d+)?$` in the bundle), parsed exactly.
fn plain_decimal(value: &str) -> Option<BigDecimal> {
    if value.is_empty() || value.len() > 4096 {
        return None;
    }
    let mut parts = value.split('.');
    let integer = parts.next()?;
    let fraction = parts.next();
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    BigDecimal::from_str(value).ok()
}

fn valid_amount(value: &str) -> Result<(), String> {
    let max = BigDecimal::from(1_000_000_000_000_000_000u64);
    match plain_decimal(value) {
        Some(amount) if amount > 0 && amount <= max => Ok(()),
        _ => Err("Amount must be greater than 0.".into()),
    }
}

fn valid_ratio(value: &str) -> Result<(), String> {
    match plain_decimal(value) {
        Some(ratio) if ratio > 0 && ratio <= 1 => Ok(()),
        _ => Err("Ratio must be between 0 (exclusive) and 1 (inclusive).".into()),
    }
}

fn valid_price_range(value: &str) -> Result<(), String> {
    match plain_decimal(value) {
        Some(range) if range > 0 && range <= 50 => Ok(()),
        _ => Err("priceRange must be a number in (0, 50].".into()),
    }
}

fn slippage_bps(value: &str) -> Result<String, String> {
    if value == "auto" {
        return Ok("auto".into());
    }
    let valid = !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && value
            .parse::<u64>()
            .is_ok_and(|bps| (1..=4999).contains(&bps));
    if valid {
        Ok(value.into())
    } else {
        Err("slippageBps must be \"auto\" or an integer between 1 and 4999.".into())
    }
}

fn gas_level(value: &str) -> Result<String, String> {
    let level = value.to_ascii_uppercase();
    if GAS_LEVELS.contains(&level.as_str()) {
        Ok(level)
    } else {
        Err("gasLevel must be LOW, MEDIUM, or HIGH.".into())
    }
}

fn claim_type(value: &str) -> Result<String, String> {
    let normalized = value.to_ascii_uppercase();
    if CLAIM_TYPES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(
            "Invalid claimType; allowed: REWARD_PROTOCOL, REWARD_INVESTMENT, LP_FEE, REDEMPTION."
                .into(),
        )
    }
}

fn preview_action(value: &str) -> Result<String, String> {
    let normalized = value.to_ascii_lowercase();
    if PREVIEW_ACTIONS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err("Invalid action; allowed: deposit, redeem, lp-add, lp-remove, claim.".into())
    }
}

/// function Zd(t){return!!t&&/^\d+$/.test(t)}
fn chain_is_numeric(chain: &str) -> bool {
    !chain.is_empty() && chain.bytes().all(|b| b.is_ascii_digit())
}

/// function Xr(t){return/^0x[a-fA-F0-9]{40}$/.test(t)} plus EIP-55 for mixed-case input.
fn valid_evm_address(address: &str) -> bool {
    let Some(hex) = address.strip_prefix("0x") else {
        return false;
    };
    if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    if hex.bytes().any(|b| b.is_ascii_uppercase()) {
        return checksum_address(address) == address;
    }
    true
}

/// function oe(t,e,n){t!==void 0&&(e!==void 0&&!Zd(e)||Xd(t,n))}: EVM addresses are checked
/// only for numeric chains, so base58 chain addresses are forwarded untouched.
fn check_address(address: Option<&str>, chain: Option<&str>, name: &str) -> Result<(), String> {
    let Some(address) = address else {
        return Ok(());
    };
    if chain.is_some_and(|chain| !chain_is_numeric(chain)) {
        return Ok(());
    }
    if valid_evm_address(address) {
        Ok(())
    } else {
        Err(format!("Invalid {name}"))
    }
}

enum LpSource {
    Nft,
    PriceRange,
    Ticks,
}

/// function On(t): `--nftId` wins and suppresses range/tick checks; otherwise exactly one
/// of `--priceRange` or the tick pair is required.
fn lp_source(
    nft: Option<&str>,
    price_range: Option<&str>,
    tick_lower: Option<&str>,
    tick_upper: Option<&str>,
) -> Result<LpSource, String> {
    if nft.is_some_and(|value| !value.is_empty()) {
        return Ok(LpSource::Nft);
    }
    let has_price_range = price_range.is_some_and(|value| !value.is_empty());
    let lower = tick_lower.is_some_and(|value| !value.is_empty());
    let upper = tick_upper.is_some_and(|value| !value.is_empty());
    if lower != upper {
        return Err("--tickLower and --tickUpper must be specified together.".into());
    }
    if !has_price_range && !lower {
        return Err(
            "Specify --priceRange or (--tickLower & --tickUpper) for a new LP position, or --nftId to top up an existing one."
                .into(),
        );
    }
    if has_price_range && lower {
        return Err(
            "--priceRange and explicit ticks are mutually exclusive when --nftId is absent; specify exactly one."
                .into(),
        );
    }
    Ok(if has_price_range {
        LpSource::PriceRange
    } else {
        LpSource::Ticks
    })
}

/// function Ln(t) for the range and function Nn(t,e) for the tick pair.
fn validate_lp_source(
    source: &LpSource,
    price_range: &Option<String>,
    tick_lower: &Option<String>,
    tick_upper: &Option<String>,
) -> Result<(), String> {
    match source {
        LpSource::Nft => Ok(()),
        LpSource::PriceRange => match price_range.as_deref() {
            Some(range) => valid_price_range(range),
            None => Err("priceRange must be a number in (0, 50].".into()),
        },
        LpSource::Ticks => {
            let lower = tick_lower
                .as_deref()
                .unwrap_or_default()
                .trim()
                .parse::<i64>()
                .map_err(|_| "tickLower and tickUpper must be integers.".to_string())?;
            let upper = tick_upper
                .as_deref()
                .unwrap_or_default()
                .trim()
                .parse::<i64>()
                .map_err(|_| "tickLower and tickUpper must be integers.".to_string())?;
            if lower >= upper {
                return Err("tickLower must be less than tickUpper.".into());
            }
            Ok(())
        }
    }
}

fn claim_required(claim_type: &str) -> &'static [&'static str] {
    match claim_type {
        "REWARD_PROTOCOL" => &["defiProtocolId"],
        "REWARD_INVESTMENT" => &["investmentId"],
        "LP_FEE" => &["investmentId", "nftId"],
        "REDEMPTION" => &["investmentId", "redemptionId"],
        _ => &[],
    }
}

fn missing_claim_params(claim_type: &str, fields: &[(&str, &Option<String>)]) -> Option<String> {
    let missing: Vec<String> = claim_required(claim_type)
        .iter()
        .filter(|name| {
            !fields
                .iter()
                .any(|(key, value)| key == *name && present(value))
        })
        .map(|name| format!("--{name}"))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "Missing required parameters for claim type \"{claim_type}\": {}",
        missing.join(", ")
    ))
}

fn preview_required<'a>(
    value: &'a Option<String>,
    name: &str,
    action: &str,
) -> Result<&'a str, String> {
    value
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("--{name} is required for preview --action {action}"))
}

pub async fn run(client: &mut Client, command: &DefiCommand) -> Result<Value, String> {
    let planned = plan(command)?;
    if client.credentials().session_id.is_empty() {
        return Err("wallet session is missing".into());
    }
    // Bundle endpoint table (class Pn): every DeFi leaf is one POST with a JSON body, using
    //   static OPEN_PREFIX="/bapi/defi/v1/public/wallet-direct/agent-wallet/defi/open";
    //   static TRADE_PREFIX="/bapi/defi/v1/public/wallet-direct/agent-wallet/defi";
    // and the per-leaf quoted call recorded above each arm of `plan`.
    client
        .request(Method::POST, &planned.path, Some(planned.body))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{channel, Receiver};
    use std::thread::JoinHandle;
    use std::time::Duration;

    const OPEN: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet/defi/open";
    const TRADE: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet/defi";
    const ADDRESS: &str = "0x66FAa5Ded68CC83a71BC172Fb03C17a6Bcb32f7a";

    struct Harness {
        client: Client,
        seen: Receiver<(String, Value)>,
        handle: JoinHandle<()>,
    }

    fn harness(responses: Vec<String>) -> Harness {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, seen) = channel();
        let handle = std::thread::spawn(move || {
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0u8; 4096];
                    let read = socket.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
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
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            response.len(),
                            response
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });
        Harness {
            client: client_at(&origin),
            seen,
            handle,
        }
    }

    fn client_at(origin: &str) -> Client {
        let credentials = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        Client::test_client(credentials, origin).unwrap()
    }

    fn ok(data: Value) -> String {
        json!({"code":"000000","data":data}).to_string()
    }

    fn protocol_list() -> ProtocolListArgs {
        ProtocolListArgs {
            binance_chain_id: Some("56".into()),
            invest_type: Some("Earn".into()),
            sort_field: Some("tvl".into()),
            sort_direction: Some("DESC".into()),
            page: Some(2),
            size: Some(10),
        }
    }

    fn deposit() -> DepositArgs {
        DepositArgs {
            investment_id: "venus_usdt".into(),
            token_address: "0x55d398326f99059fF775485246999027B3197955".into(),
            amount: "100".into(),
            gas_level: "HIGH".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn redeem() -> RedeemArgs {
        RedeemArgs {
            investment_id: "venus_usdt".into(),
            token_address: "0x55d398326f99059fF775485246999027B3197955".into(),
            amount: Some("50".into()),
            ratio: None,
            gas_level: "MEDIUM".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn lp_add() -> LpAddArgs {
        LpAddArgs {
            investment_id: "pancake_v3_xxx".into(),
            token_address: "0x55d398326f99059fF775485246999027B3197955".into(),
            amount: "1".into(),
            nft_id: None,
            tick_lower: None,
            tick_upper: None,
            price_range: Some("5".into()),
            slippage_bps: "auto".into(),
            gas_level: "MEDIUM".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn lp_remove() -> LpRemoveArgs {
        LpRemoveArgs {
            investment_id: "pancake_v3_xxx".into(),
            nft_id: "12345".into(),
            ratio: "0.5".into(),
            slippage_bps: "100".into(),
            gas_level: "MEDIUM".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn claim() -> ClaimArgs {
        ClaimArgs {
            claim_type: "lp_fee".into(),
            investment_id: Some("pancake_v3_xxx".into()),
            defi_protocol_id: None,
            nft_id: Some("12345".into()),
            redemption_id: None,
            token_address: None,
            gas_level: "MEDIUM".into(),
            binance_chain_id: "56".into(),
        }
    }

    fn preview(action: &str) -> PreviewArgs {
        PreviewArgs {
            action: action.into(),
            investment_id: Some("X".into()),
            token_address: Some("0x55d398326f99059fF775485246999027B3197955".into()),
            amount: Some("100".into()),
            ratio: None,
            nft_id: None,
            tick_lower: None,
            tick_upper: None,
            price_range: None,
            slippage_bps: None,
            claim_type: None,
            defi_protocol_id: None,
            redemption_id: None,
            gas_level: "MEDIUM".into(),
            binance_chain_id: "56".into(),
        }
    }

    #[tokio::test]
    async fn protocol_list_sends_only_provided_filters() {
        let mut h = harness(vec![
            ok(json!({"total":1,"list":[]})),
            ok(json!({"total":0})),
        ]);
        let data = run(&mut h.client, &DefiCommand::ProtocolList(protocol_list()))
            .await
            .unwrap();
        assert_eq!(data["total"], 1);
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/protocol/list HTTP/1.1"));
        assert_eq!(
            body,
            json!({"binanceChainId":"56","investType":"Earn","sortField":"tvl","sortDirection":"DESC","page":2,"size":10})
        );
        // The bundle declares no commander defaults here, so an unset filter is omitted.
        run(
            &mut h.client,
            &DefiCommand::ProtocolList(ProtocolListArgs {
                binance_chain_id: None,
                invest_type: None,
                sort_field: None,
                sort_direction: None,
                page: None,
                size: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(h.seen.recv().unwrap().1, json!({}));
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn protocol_info_sends_defi_protocol_id() {
        let mut h = harness(vec![ok(json!({"protocolName":"Venus"}))]);
        let data = run(
            &mut h.client,
            &DefiCommand::ProtocolInfo(ProtocolInfoArgs {
                defi_protocol_id: "venus".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(data["protocolName"], "Venus");
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/protocol/info HTTP/1.1"));
        assert_eq!(body, json!({"defiProtocolId":"venus"}));
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn investment_list_sends_filters_and_required_type() {
        let mut h = harness(vec![ok(json!({"total":0,"list":[]}))]);
        run(
            &mut h.client,
            &DefiCommand::InvestmentList(InvestmentListArgs {
                invest_type: "Earn".into(),
                defi_protocol_id: Some("venus".into()),
                contract_addresses: Some("0xaaa,0xbbb".into()),
                binance_chain_id: Some("56".into()),
                sort_field: Some("apy".into()),
                sort_direction: Some("ASC".into()),
                page: Some(1),
                size: Some(20),
            }),
        )
        .await
        .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/investment/list HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "investType":"Earn",
                "defiProtocolId":"venus",
                "contractAddresses":"0xaaa,0xbbb",
                "binanceChainId":"56",
                "sortField":"apy",
                "sortDirection":"ASC",
                "page":1,
                "size":20
            })
        );
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn investment_info_sends_investment_id() {
        let mut h = harness(vec![ok(json!({"investmentName":"USDT"}))]);
        run(
            &mut h.client,
            &DefiCommand::InvestmentInfo(InvestmentInfoArgs {
                investment_id: "venus-bsc-usdt-supply".into(),
            }),
        )
        .await
        .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/investment/info HTTP/1.1"));
        assert_eq!(body, json!({"investmentId":"venus-bsc-usdt-supply"}));
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn position_routes_refresh_to_its_own_path() {
        let mut h = harness(vec![ok(json!({"deFiTotalValue":"1"})), ok(json!({}))]);
        run(
            &mut h.client,
            &DefiCommand::Position(PositionArgs {
                address: Some(ADDRESS.into()),
                binance_chain_id: Some("56".into()),
                defi_protocol_id: Some("venus".into()),
                refresh: false,
            }),
        )
        .await
        .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/position HTTP/1.1"));
        assert_eq!(
            body,
            json!({"address":ADDRESS,"binanceChainId":"56","defiProtocolId":"venus"})
        );
        run(
            &mut h.client,
            &DefiCommand::Position(PositionArgs {
                address: None,
                binance_chain_id: None,
                defi_protocol_id: None,
                refresh: true,
            }),
        )
        .await
        .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {OPEN}/position/refresh HTTP/1.1"));
        assert_eq!(body, json!({}));
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn deposit_sends_identifiers_amount_and_normalized_gas() {
        let mut h = harness(vec![ok(json!({"txHash":"0xfeed"}))]);
        let data = run(&mut h.client, &DefiCommand::Deposit(deposit()))
            .await
            .unwrap();
        assert_eq!(data["txHash"], "0xfeed");
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/deposit HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "investmentId":"venus_usdt",
                "tokenAddress":"0x55d398326f99059fF775485246999027B3197955",
                "amount":"100",
                "gasLevel":"HIGH"
            })
        );
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn redeem_sends_exactly_one_of_amount_or_ratio() {
        let mut h = harness(vec![ok(json!({})), ok(json!({"redeemDelayDays":[1,3]}))]);
        run(&mut h.client, &DefiCommand::Redeem(redeem()))
            .await
            .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/redeem HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "investmentId":"venus_usdt",
                "tokenAddress":"0x55d398326f99059fF775485246999027B3197955",
                "amount":"50",
                "gasLevel":"MEDIUM"
            })
        );
        let mut by_ratio = redeem();
        by_ratio.amount = None;
        by_ratio.ratio = Some("1".into());
        run(&mut h.client, &DefiCommand::Redeem(by_ratio))
            .await
            .unwrap();
        let (_, body) = h.seen.recv().unwrap();
        assert_eq!(body["ratio"], "1");
        assert!(body.get("amount").is_none());
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn lp_add_sends_range_and_omits_absent_position_fields() {
        let mut h = harness(vec![ok(json!({"txHash":"0xaa"}))]);
        run(&mut h.client, &DefiCommand::LpAdd(lp_add()))
            .await
            .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/lp-add HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "investmentId":"pancake_v3_xxx",
                "tokenAddress":"0x55d398326f99059fF775485246999027B3197955",
                "amount":"1",
                "priceRange":"5",
                "slippageBps":"auto",
                "gasLevel":"MEDIUM"
            })
        );
        assert!(body.get("nftId").is_none() && body.get("tickLower").is_none());
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn lp_remove_sends_nft_ratio_and_slippage() {
        let mut h = harness(vec![ok(json!({"txHash":"0xbb"}))]);
        run(&mut h.client, &DefiCommand::LpRemove(lp_remove()))
            .await
            .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/lp-remove HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "investmentId":"pancake_v3_xxx",
                "nftId":"12345",
                "ratio":"0.5",
                "slippageBps":"100",
                "gasLevel":"MEDIUM"
            })
        );
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn claim_sends_uppercased_type_and_optional_identifiers() {
        let mut h = harness(vec![ok(json!({"txHash":"0xcc"}))]);
        run(&mut h.client, &DefiCommand::Claim(claim()))
            .await
            .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/claim HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "binanceChainId":"56",
                "claimType":"LP_FEE",
                "investmentId":"pancake_v3_xxx",
                "nftId":"12345",
                "gasLevel":"MEDIUM"
            })
        );
        h.handle.join().unwrap();
    }

    #[tokio::test]
    async fn preview_sends_action_scoped_body() {
        let mut h = harness(vec![
            ok(json!({"balanceChange":[]})),
            ok(json!({"balanceChange":[]})),
        ]);
        run(&mut h.client, &DefiCommand::Preview(preview("deposit")))
            .await
            .unwrap();
        let (head, body) = h.seen.recv().unwrap();
        assert_eq!(head, format!("POST {TRADE}/preview HTTP/1.1"));
        assert_eq!(
            body,
            json!({
                "action":"deposit",
                "binanceChainId":"56",
                "gasLevel":"MEDIUM",
                "investmentId":"X",
                "tokenAddress":"0x55d398326f99059fF775485246999027B3197955",
                "amount":"100"
            })
        );
        // lp-remove carries only identifiers/ratio and no token address, matching the bundle.
        let mut remove = preview("lp-remove");
        remove.amount = None;
        remove.nft_id = Some("12345".into());
        remove.ratio = Some("1".into());
        run(&mut h.client, &DefiCommand::Preview(remove))
            .await
            .unwrap();
        let (_, body) = h.seen.recv().unwrap();
        assert_eq!(
            body,
            json!({
                "action":"lp-remove",
                "binanceChainId":"56",
                "gasLevel":"MEDIUM",
                "investmentId":"X",
                "nftId":"12345",
                "ratio":"1"
            })
        );
        h.handle.join().unwrap();
    }

    #[test]
    fn only_writing_leaves_are_mutations() {
        let reads = [
            DefiCommand::ProtocolList(protocol_list()),
            DefiCommand::ProtocolInfo(ProtocolInfoArgs {
                defi_protocol_id: "venus".into(),
            }),
            DefiCommand::InvestmentList(InvestmentListArgs {
                invest_type: "Earn".into(),
                defi_protocol_id: None,
                contract_addresses: None,
                binance_chain_id: None,
                sort_field: None,
                sort_direction: None,
                page: None,
                size: None,
            }),
            DefiCommand::InvestmentInfo(InvestmentInfoArgs {
                investment_id: "venus-bsc-usdt-supply".into(),
            }),
            DefiCommand::Position(PositionArgs {
                address: None,
                binance_chain_id: None,
                defi_protocol_id: None,
                refresh: false,
            }),
            DefiCommand::Preview(preview("deposit")),
        ];
        for command in &reads {
            assert!(!is_mutation(command), "{command:?} must be a read");
            assert_eq!(mutation_payload(command).unwrap_err(), "not a mutation");
        }
        let writes = [
            DefiCommand::Deposit(deposit()),
            DefiCommand::Redeem(redeem()),
            DefiCommand::LpAdd(lp_add()),
            DefiCommand::LpRemove(lp_remove()),
            DefiCommand::Claim(claim()),
        ];
        let mut operations = Vec::new();
        for command in &writes {
            assert!(is_mutation(command), "{command:?} must be a mutation");
            let first = mutation_payload(command).unwrap();
            assert_eq!(first, mutation_payload(command).unwrap(), "must be stable");
            operations.push(first["operation"].as_str().unwrap_or_default().to_string());
            // Identifiers/amounts only: never a transaction hash or backend value.
            assert!(first["args"].get("txHash").is_none());
        }
        operations.sort();
        operations.dedup();
        assert_eq!(operations.len(), 5);
        assert_eq!(
            mutation_payload(&writes[0]).unwrap()["args"],
            json!({
                "binanceChainId":"56",
                "investmentId":"venus_usdt",
                "tokenAddress":"0x55d398326f99059fF775485246999027B3197955",
                "amount":"100",
                "gasLevel":"HIGH"
            })
        );
    }

    #[test]
    fn mutation_payloads_distinguish_leaves_with_identical_amounts() {
        let mut redeemed = redeem();
        redeemed.amount = Some("100".into());
        let deposited = mutation_payload(&DefiCommand::Deposit(deposit())).unwrap();
        let redeemed = mutation_payload(&DefiCommand::Redeem(redeemed)).unwrap();
        assert_eq!(deposited["args"]["amount"], redeemed["args"]["amount"]);
        assert_ne!(deposited["operation"], redeemed["operation"]);
        assert_ne!(deposited, redeemed);
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let mut h = harness(vec![
            json!({"code":"100500","message":"synthetic-secret-session","data":null}).to_string(),
        ]);
        let error = run(&mut h.client, &DefiCommand::Deposit(deposit()))
            .await
            .expect_err("must fail closed");
        assert_eq!(error, "Binance API code 100500");
        assert!(!error.contains("synthetic-secret"));
        h.handle.join().unwrap();
    }

    #[test]
    fn cli_surface_matches_documented_leaf_names_and_defaults() {
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            command: DefiCommand,
        }
        let parse = |args: &[&str]| TestCli::try_parse_from(args).unwrap().command;
        // Every leaf and option name exactly as the installed CLI prints them, including the
        // negative tick values its own examples use.
        let cases: [&[&str]; 11] = [
            &[
                "defi",
                "protocol-list",
                "--binanceChainId",
                "56",
                "--investType",
                "Earn",
                "--sortField",
                "tvl",
                "--sortDirection",
                "DESC",
                "--page",
                "1",
                "--size",
                "200",
            ],
            &["defi", "protocol-info", "--defiProtocolId", "venus"],
            &[
                "defi",
                "investment-list",
                "--investType",
                "Earn",
                "--defiProtocolId",
                "venus",
                "--contractAddresses",
                "0xa,0xb",
                "--binanceChainId",
                "56",
                "--sortField",
                "apy",
                "--sortDirection",
                "ASC",
                "--page",
                "1",
                "--size",
                "20",
            ],
            &[
                "defi",
                "investment-info",
                "--investmentId",
                "venus-bsc-usdt-supply",
            ],
            &[
                "defi",
                "position",
                "--address",
                ADDRESS,
                "--binanceChainId",
                "56",
                "--defiProtocolId",
                "venus",
                "--refresh",
            ],
            &[
                "defi",
                "deposit",
                "--investmentId",
                "i",
                "--tokenAddress",
                ADDRESS,
                "--amount",
                "1",
                "--gasLevel",
                "LOW",
                "--binanceChainId",
                "1",
            ],
            &[
                "defi",
                "redeem",
                "--investmentId",
                "i",
                "--tokenAddress",
                ADDRESS,
                "--ratio",
                "1",
                "--gasLevel",
                "HIGH",
            ],
            &[
                "defi",
                "lp-add",
                "--investmentId",
                "i",
                "--tokenAddress",
                ADDRESS,
                "--amount",
                "1",
                "--nftId",
                "1",
                "--tickLower",
                "-887220",
                "--tickUpper",
                "887220",
                "--priceRange",
                "5",
                "--slippageBps",
                "100",
            ],
            &[
                "defi",
                "lp-remove",
                "--investmentId",
                "i",
                "--nftId",
                "1",
                "--ratio",
                "1",
                "--slippageBps",
                "auto",
            ],
            &[
                "defi",
                "claim",
                "--claimType",
                "LP_FEE",
                "--investmentId",
                "i",
                "--defiProtocolId",
                "venus",
                "--nftId",
                "1",
                "--redemptionId",
                "r",
                "--tokenAddress",
                ADDRESS,
            ],
            &[
                "defi",
                "preview",
                "--action",
                "claim",
                "--investmentId",
                "i",
                "--tokenAddress",
                ADDRESS,
                "--amount",
                "1",
                "--ratio",
                "1",
                "--nftId",
                "1",
                "--tickLower",
                "-1",
                "--tickUpper",
                "1",
                "--priceRange",
                "5",
                "--slippageBps",
                "auto",
                "--claimType",
                "LP_FEE",
                "--defiProtocolId",
                "venus",
                "--redemptionId",
                "r",
            ],
        ];
        for args in cases {
            TestCli::try_parse_from(args).unwrap_or_else(|error| panic!("{args:?}: {error}"));
        }
        // Commander defaults, verbatim from --help.
        match parse(&[
            "defi",
            "deposit",
            "--investmentId",
            "i",
            "--tokenAddress",
            ADDRESS,
            "--amount",
            "1",
        ]) {
            DefiCommand::Deposit(a) => {
                assert_eq!(a.gas_level, "MEDIUM");
                assert_eq!(a.binance_chain_id, "56");
            }
            other => panic!("{other:?}"),
        }
        match parse(&[
            "defi",
            "lp-add",
            "--investmentId",
            "i",
            "--tokenAddress",
            ADDRESS,
            "--amount",
            "1",
            "--priceRange",
            "5",
        ]) {
            DefiCommand::LpAdd(a) => assert_eq!(a.slippage_bps, "auto"),
            other => panic!("{other:?}"),
        }
        // Reads carry no defaults: `defi position` alone is a valid empty-filter query.
        assert!(matches!(
            parse(&["defi", "position"]),
            DefiCommand::Position(_)
        ));
    }

    #[test]
    fn missing_required_option_is_rejected_before_any_call() {
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            command: DefiCommand,
        }
        for args in [
            vec!["defi", "deposit", "--investmentId", "venus_usdt"],
            vec!["defi", "claim"],
            vec!["defi", "preview", "--investmentId", "X"],
            vec!["defi", "protocol-info"],
            vec!["defi", "investment-list"],
            vec!["defi", "lp-remove", "--investmentId", "p", "--ratio", "1"],
        ] {
            let error = TestCli::try_parse_from(&args).expect_err("must require options");
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::MissingRequiredArgument,
                "{args:?}"
            );
        }
    }

    #[test]
    fn address_validation_matches_bundle_checksum_rules() {
        // Mixed case must be a valid EIP-55 checksum; all-lowercase is accepted as-is.
        assert!(valid_evm_address(
            "0x66FAa5Ded68CC83a71BC172Fb03C17a6Bcb32f7a"
        ));
        assert!(valid_evm_address(
            "0x66faa5ded68cc83a71bc172fb03c17a6bcb32f7a"
        ));
        assert!(!valid_evm_address(
            "0x66FAa5Ded68CC83a71BC172Fb03C17a6Bcb32f7b"
        ));
        assert!(!valid_evm_address("0x1234"));
        // Non-numeric chains use non-EVM address formats and are forwarded untouched.
        assert!(check_address(
            Some("So11111111111111111111111111111111111111112"),
            Some("CT_501"),
            "address"
        )
        .is_ok());
        assert!(check_address(Some("not-an-address"), None, "address").is_err());
        assert!(check_address(None, Some("56"), "address").is_ok());
    }

    #[tokio::test]
    async fn invalid_arguments_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = client_at(&origin);

        let mut zero_amount = deposit();
        zero_amount.amount = "0".into();
        let mut bad_checksum = deposit();
        bad_checksum.token_address = "0x66FAa5Ded68CC83a71BC172Fb03C17a6Bcb32f7b".into();
        let mut both = redeem();
        both.ratio = Some("1".into());
        let mut over_ratio = lp_remove();
        over_ratio.ratio = "2".into();
        let mut bad_slippage = lp_remove();
        bad_slippage.slippage_bps = "5000".into();
        let mut missing_nft = claim();
        missing_nft.nft_id = None;
        let mut mixed = lp_add();
        mixed.tick_lower = Some("-10".into());
        let mut big_range = lp_add();
        big_range.price_range = Some("51".into());
        let mut inverted = lp_add();
        inverted.price_range = None;
        inverted.tick_lower = Some("10".into());
        inverted.tick_upper = Some("-10".into());
        let mut bad_preview = preview("redeem");
        bad_preview.ratio = Some("1".into());

        for command in [
            DefiCommand::Deposit(zero_amount),
            DefiCommand::Deposit(bad_checksum),
            DefiCommand::Redeem(both),
            DefiCommand::LpRemove(over_ratio),
            DefiCommand::LpRemove(bad_slippage),
            DefiCommand::Claim(missing_nft),
            DefiCommand::LpAdd(mixed),
            DefiCommand::LpAdd(big_range),
            DefiCommand::LpAdd(inverted),
            DefiCommand::Preview(bad_preview),
            DefiCommand::Position(PositionArgs {
                address: Some("0xnot-an-address".into()),
                binance_chain_id: Some("56".into()),
                defi_protocol_id: None,
                refresh: false,
            }),
        ] {
            assert!(run(&mut client, &command).await.is_err(), "{command:?}");
        }
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            listener.accept().is_err(),
            "rejected arguments must never open a connection"
        );
    }
}
