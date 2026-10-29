//! Leaderboard, address analysis and alpha radar. Mirrors `baw leaderboard`.
//!
//! The option surface and defaults come from the installed CLI's own `--help`
//! (`docs/JS-CLI-PARITY.md`); every endpoint, HTTP method and body field name comes from
//! `@binance/agentic-wallet/dist/index.js`. The bundle fragment justifying each call is
//! quoted verbatim above it. These paths sit outside the `/agent-wallet` prefix owned by
//! `crate::client::API`, so they are sent as full paths.
use crate::client::Client;
use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashSet;

/// `var At="/bapi/defi/v1/public/wallet-direct",...bl=At+"/market/leaderboard/query"`
const QUERY: &str = "/bapi/defi/v1/public/wallet-direct/market/leaderboard/query";
/// `Tl=At+"/agent/market/alpharadar/query"`
const ALPHA_RADAR: &str = "/bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/query";
/// `Al=At+"/agent/market/leaderboard/preset/save"`
const PRESET_SAVE: &str = "/bapi/defi/v1/public/wallet-direct/agent/market/leaderboard/preset/save";
/// `Sl=At+"/agent/market/leaderboard/preset/list"`
const PRESET_LIST: &str = "/bapi/defi/v1/public/wallet-direct/agent/market/leaderboard/preset/list";
/// `wl=At+"/agent/market/alpharadar/config/save"`
const RADAR_SAVE: &str = "/bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/config/save";
/// `Cl=At+"/agent/market/alpharadar/config/list"`
const RADAR_LIST: &str = "/bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/config/list";

/// `var oc=["7d","30d","90d"];function Rr(t){...oc.includes(e)||T(...)}`
const PERIODS: [&str; 3] = ["7d", "30d", "90d"];
/// `var ic=["ALL","KOL","MPC"];function sc(t){let e=(t??"ALL").toUpperCase();...}`
const TAGS: [&str; 3] = ["ALL", "KOL", "MPC"];
/// `var Wa=new Set(["1","56","8453","CT_501","4663"])` plus `5042 (ARC)` in the error text.
const CHAINS: [&str; 6] = ["1", "56", "8453", "CT_501", "4663", "5042"];
/// `var Ba=20` — the bundle's fixed page size inside `analyzeAddress`.
const ANALYZE_PAGE_SIZE: i64 = 20;
const MAX_TOP_N: i64 = 5_000;

#[derive(clap::Subcommand, Debug)]
pub enum LeaderboardCommand {
    /// Query leaderboard (top traders)
    Query(QueryArgs),
    /// Analyze a single address (reverse-lookup within top N)
    Analyze(AnalyzeArgs),
    /// Alpha radar query (find addresses holding target tokens)
    AlphaRadar(AlphaRadarArgs),
    /// Manage leaderboard filter presets
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Manage alpha radar configs
    AlphaRadarConfig {
        #[command(subcommand)]
        command: RadarConfigCommand,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum PresetCommand {
    /// List saved presets
    List,
    /// Save presets (replaces all; --config is a JSON array of PresetItem)
    Save(PresetSaveArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum RadarConfigCommand {
    /// List alpha radar configs
    List(RadarConfigListArgs),
    /// Save alpha radar configs (--config is a JSON array of AlphaRadarConfigItem)
    Save(RadarConfigSaveArgs),
}

#[derive(Args, Debug)]
pub struct QueryArgs {
    /// Chain ID
    #[arg(
        short = 'c',
        long = "chain-id",
        value_name = "chainId",
        value_parser = nonempty
    )]
    pub chain_id: String,
    /// Period: 7d/30d/90d
    #[arg(short = 'p', long, value_name = "period", default_value = "30d")]
    pub period: String,
    /// Tag: ALL/KOL/MPC
    #[arg(short = 't', long, value_name = "tag", default_value = "ALL")]
    pub tag: String,
    /// Sort field (0=PnL 20=win 30=vol 50=tx 60=active 70=rate 80=tokens)
    #[arg(long = "sort-by", value_name = "n", default_value = "0")]
    pub sort_by: i64,
    /// Order (0/2=desc 1=asc)
    #[arg(long = "order-by", value_name = "n", default_value = "0")]
    pub order_by: i64,
    /// Page number (from 0)
    #[arg(long, value_name = "page", default_value = "0")]
    pub page: i64,
    /// Page size (max 20)
    #[arg(long, value_name = "size", default_value = "20")]
    pub size: i64,
}

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Chain ID
    #[arg(
        short = 'c',
        long = "chain-id",
        value_name = "chainId",
        value_parser = nonempty
    )]
    pub chain_id: String,
    /// Address to analyze
    #[arg(
        short = 'a',
        long = "address",
        value_name = "address",
        value_parser = nonempty
    )]
    pub address: String,
    /// Period: 7d/30d/90d
    #[arg(short = 'p', long, value_name = "period", default_value = "30d")]
    pub period: String,
    /// Scan top N entries (default 1000, max 5000)
    #[arg(long = "top-n", value_name = "n", default_value = "1000")]
    pub top_n: i64,
}

#[derive(Args, Debug)]
pub struct AlphaRadarArgs {
    /// Chain ID
    #[arg(
        short = 'c',
        long = "chain-id",
        value_name = "chainId",
        value_parser = nonempty
    )]
    pub chain_id: String,
    /// Comma-separated token addresses
    #[arg(short = 't', long = "tokens", value_name = "tokens")]
    pub tokens: String,
    /// Min number of tokens to match (>=1)
    #[arg(short = 'm', long = "match-count", value_name = "count")]
    pub match_count: i64,
    /// Period: 7d/30d/90d
    #[arg(short = 'p', long, value_name = "period", default_value = "30d")]
    pub period: String,
    /// Page number (from 0)
    #[arg(long, value_name = "page", default_value = "0")]
    pub page: i64,
    /// Page size (max 20)
    #[arg(long, value_name = "size", default_value = "20")]
    pub size: i64,
}

#[derive(Args, Debug)]
pub struct PresetSaveArgs {
    /// JSON array of preset items
    #[arg(long = "config", value_name = "json")]
    pub config: String,
}

#[derive(Args, Debug)]
pub struct RadarConfigListArgs {
    /// Chain ID
    #[arg(
        short = 'c',
        long = "chain-id",
        value_name = "chainId",
        value_parser = nonempty
    )]
    pub chain_id: String,
}

#[derive(Args, Debug)]
pub struct RadarConfigSaveArgs {
    /// Chain ID
    #[arg(
        short = 'c',
        long = "chain-id",
        value_name = "chainId",
        value_parser = nonempty
    )]
    pub chain_id: String,
    /// JSON array of AlphaRadarConfigItem
    #[arg(long = "config", value_name = "config")]
    pub config: String,
}

fn nonempty(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("value must be nonempty".into())
    } else {
        Ok(value.into())
    }
}

pub fn is_mutation(command: &LeaderboardCommand) -> bool {
    matches!(
        command,
        LeaderboardCommand::Preset {
            command: PresetCommand::Save(_)
        } | LeaderboardCommand::AlphaRadarConfig {
            command: RadarConfigCommand::Save(_)
        }
    )
}

/// Deterministic intent digest input for the durable journal: the operation
/// discriminator plus the config content that is about to be sent — never a timestamp,
/// backend value, session token or display text.
pub fn mutation_payload(command: &LeaderboardCommand) -> Result<Value, String> {
    match command {
        LeaderboardCommand::Preset {
            command: PresetCommand::Save(args),
        } => Ok(json!({
            "operation": "leaderboard.preset.save",
            "presetList": config_array(&args.config)?,
        })),
        LeaderboardCommand::AlphaRadarConfig {
            command: RadarConfigCommand::Save(args),
        } => Ok(json!({
            "operation": "leaderboard.alpha-radar-config.save",
            "chainId": chain(&args.chain_id)?,
            "alphaRadarConfigList": config_array(&args.config)?,
        })),
        _ => Err("not a mutation".into()),
    }
}

pub async fn run(client: &mut Client, command: &LeaderboardCommand) -> Result<Value, String> {
    match command {
        LeaderboardCommand::Query(args) => query(client, args).await,
        LeaderboardCommand::Analyze(args) => analyze(client, args).await,
        LeaderboardCommand::AlphaRadar(args) => alpha_radar(client, args).await,
        LeaderboardCommand::Preset { command } => match command {
            PresetCommand::List => preset_list(client).await,
            PresetCommand::Save(args) => preset_save(client, args).await,
        },
        LeaderboardCommand::AlphaRadarConfig { command } => match command {
            RadarConfigCommand::List(args) => radar_list(client, args).await,
            RadarConfigCommand::Save(args) => radar_save(client, args).await,
        },
    }
}

/// `async queryLeaderboard(e){let n=this.buildQueryString(e);return this.request(`${bl}${n}`,
/// {method:"GET"})}` with the action body
/// `{chainId:$(n.chainId),period:Rr(n.period),tag:sc(n.tag),sortBy:parseInt(n.sortBy,10),
/// orderBy:parseInt(n.orderBy,10),pageNo:i,pageSize:s}`.
///
/// `buildQueryString` skips null/empty values and percent-encodes the rest; every value we
/// emit is validated down to unreserved ASCII (chain/tag/period whitelists, base-10
/// integers), so the encoding step is a no-op here.
async fn query(client: &mut Client, args: &QueryArgs) -> Result<Value, String> {
    let path = leaderboard_query_path(
        &chain(&args.chain_id)?,
        &period(&args.period)?,
        &tag(&args.tag)?,
        args.sort_by,
        args.order_by,
        page(args.page)?,
        page_size(args.size)?,
    );
    let data = client.request(Method::GET, &path, None).await?;
    // `async queryLeaderboard(e){...return this.unwrap(n,"Leaderboard query failed")?.data??[]}`
    // — the rows are the nested `data` of the unwrapped payload.
    Ok(Value::Array(query_rows(data)?))
}

fn leaderboard_query_path(
    chain_id: &str,
    period: &str,
    tag: &str,
    sort_by: i64,
    order_by: i64,
    page: i64,
    size: i64,
) -> String {
    format!(
        "{QUERY}?chainId={chain_id}&period={period}&tag={tag}&sortBy={sort_by}&orderBy={order_by}&pageNo={page}&pageSize={size}"
    )
}

/// `async queryAlphaRadar(e){return this.request(Tl,{method:"POST",body:e})}` with the action body
/// `{chainId:$(e.chainId),period:Rr(e.period),tokenAddressList:o,matchTokenCount:s,pageNo:i,pageSize:a}`
/// where `o=String(e.tokens).split(",").map(u=>u.trim()).filter(Boolean)`.
async fn alpha_radar(client: &mut Client, args: &AlphaRadarArgs) -> Result<Value, String> {
    let chain_id = chain(&args.chain_id)?;
    let period = period(&args.period)?;
    let tokens: Vec<&str> = args
        .tokens
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Err("at least one token address required".into());
    }
    if args.match_count < 1 || args.match_count as usize > tokens.len() {
        return Err("match count must be between 1 and the number of tokens".into());
    }
    let body = json!({
        "chainId": chain_id,
        "period": period,
        "tokenAddressList": tokens,
        "matchTokenCount": args.match_count,
        "pageNo": page(args.page)?,
        "pageSize": page_size(args.size)?,
    });
    let data = client
        .request(Method::POST, ALPHA_RADAR, Some(body))
        .await?;
    // `async queryAlphaRadar(e){...return this.unwrap(n,"Alpha radar query failed")??[]}`
    Ok(list_or_empty(data))
}

/// `async listPreset(){return this.request(Sl,{method:"POST",body:{}})}` — the bundle always
/// posts an empty object, not a bodyless request.
async fn preset_list(client: &mut Client) -> Result<Value, String> {
    let data = client
        .request(Method::POST, PRESET_LIST, Some(json!({})))
        .await?;
    // `async listPreset(){let e=await this.api.listPreset();return this.unwrap(e,"Preset list failed")??[]}`
    Ok(list_or_empty(data))
}

/// `async savePreset(e){return this.request(Al,{method:"POST",body:e})}` where the tracker
/// wrapper sends `{presetList:e}`; the leaf prints `{success:a,count:s.length}`.
async fn preset_save(client: &mut Client, args: &PresetSaveArgs) -> Result<Value, String> {
    let presets = config_array(&args.config)?;
    let count = presets.len();
    let body = json!({"presetList": presets});
    let data = client
        .request(Method::POST, PRESET_SAVE, Some(body))
        .await?;
    Ok(json!({"success": data, "count": count}))
}

/// `async listRadarConfig(e){let n=await this.api.listRadarConfig({chainId:e});...}`
/// and `async listRadarConfig(e){return this.request(Cl,{method:"POST",body:e})}`.
async fn radar_list(client: &mut Client, args: &RadarConfigListArgs) -> Result<Value, String> {
    let body = json!({"chainId": chain(&args.chain_id)?});
    let data = client.request(Method::POST, RADAR_LIST, Some(body)).await?;
    // `return this.unwrap(n,"Radar config list failed")??[]`
    Ok(list_or_empty(data))
}

/// `saveRadarConfig({chainId:$(n.chainId),alphaRadarConfigList:s})` posted by
/// `async saveRadarConfig(e){return this.request(wl,{method:"POST",body:e})}`; the leaf prints
/// `{success:a,count:s.length}`.
async fn radar_save(client: &mut Client, args: &RadarConfigSaveArgs) -> Result<Value, String> {
    let configs = config_array(&args.config)?;
    let count = configs.len();
    let body = json!({
        "chainId": chain(&args.chain_id)?,
        "alphaRadarConfigList": configs,
    });
    let data = client.request(Method::POST, RADAR_SAVE, Some(body)).await?;
    Ok(json!({"success": data, "count": count}))
}

/// `async analyzeAddress(e,n,r="30d",o=1e3)` is orchestration, not an endpoint: it walks
/// `queryLeaderboard({tag:"ALL",sortBy:0,orderBy:0,pageNo:P,pageSize:20})` page by page,
/// deduplicates addresses and scores the first match locally with `scoreLeaderboardItem`.
/// The bundle fetches five pages concurrently; `Client::request` needs `&mut self`, so the
/// pages are fetched sequentially in the same order — identical requests and identical result.
async fn analyze(client: &mut Client, args: &AnalyzeArgs) -> Result<Value, String> {
    let chain_id = chain(&args.chain_id)?;
    let period = period(&args.period)?;
    let target = args.address.to_ascii_lowercase();
    // `i=Math.min(Math.max(parseInt(e.topN,10)||1e3,1),5e3)`
    let top_n = args.top_n.clamp(1, MAX_TOP_N);
    let pages = (top_n + ANALYZE_PAGE_SIZE - 1) / ANALYZE_PAGE_SIZE;
    let mut seen: HashSet<String> = HashSet::new();
    let mut scanned = 0usize;
    for page_no in 0..pages {
        let path =
            leaderboard_query_path(&chain_id, &period, "ALL", 0, 0, page_no, ANALYZE_PAGE_SIZE);
        let data = client.request(Method::GET, &path, None).await?;
        let rows = query_rows(data)?;
        let Some(head) = rows.first() else {
            continue;
        };
        // The bundle skips a whole page whose head was already collected (rank shift guard).
        let head = head
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase);
        if head.as_ref().is_some_and(|head| seen.contains(head)) {
            continue;
        }
        for row in rows {
            let Some(address) = row.get("address").and_then(Value::as_str) else {
                continue;
            };
            let address = address.to_ascii_lowercase();
            if !seen.insert(address.clone()) {
                continue;
            }
            scanned += 1;
            if address == target {
                return Ok(score_item(&row, scanned, &args.address));
            }
        }
    }
    if scanned == 0 {
        // `throw u===0?new m(d.TRACKER_LEADERBOARD_EMPTY,"Leaderboard is empty")`
        Err("leaderboard is empty".into())
    } else {
        // `TRACKER_ADDRESS_NOT_RANKED` — static, never the backend's message text.
        Err("address not ranked in requested top N".into())
    }
}

/// `scoreLeaderboardItem(e,n)` — the bundle's documented local rating formula.
fn score_item(item: &Value, rank: usize, address: &str) -> Value {
    let winrate = round1((number(item.get("winRate")) * 25.0).clamp(0.0, 25.0));
    let daily: &[Value] = item
        .get("dailyPNL")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice);
    let positives = daily
        .iter()
        .filter(|row| number(row.get("realizedPnl")) > 0.0)
        .count();
    let stability = if daily.is_empty() {
        0.0
    } else {
        round1((positives as f64 / daily.len() as f64 * 20.0).clamp(0.0, 20.0))
    };
    let counts = |key: &str| {
        item.get("tokenDistribution")
            .and_then(|dist| dist.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let total = counts("gt500Cnt")
        + counts("between0And500Cnt")
        + counts("between0AndNegative50Cnt")
        + counts("ltNegative50Cnt");
    let losers = counts("between0AndNegative50Cnt") + counts("ltNegative50Cnt");
    let drawdown = if total > 0.0 {
        round1(((1.0 - losers / total) * 20.0).clamp(0.0, 20.0))
    } else {
        0.0
    };
    let tag_count = item
        .get("tags")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        + item
            .get("genericAddressTagList")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
    let tags = round1((tag_count as f64 * 5.0).clamp(0.0, 15.0));
    let pnl = round1((number(item.get("realizedPnlPercent")) / 100.0 * 10.0).clamp(0.0, 10.0));
    let txs = item
        .get("totalTxCnt")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let follow_friendly = if (20.0..=500.0).contains(&txs) {
        10
    } else if txs > 500.0 && txs <= 2_000.0 {
        6
    } else if (5.0..20.0).contains(&txs) {
        5
    } else if txs > 2_000.0 {
        3
    } else {
        0
    };
    let score = round1(winrate + stability + drawdown + tags + pnl + follow_friendly as f64);
    let rating = if score >= 80.0 {
        "\u{2b50}\u{2b50}\u{2b50}"
    } else if score >= 65.0 {
        "\u{2b50}\u{2b50}"
    } else if score >= 50.0 {
        "\u{2b50}"
    } else {
        "\u{274c}"
    };
    json!({
        "address": address,
        "score": number_value(score),
        "rating": rating,
        "breakdown": {
            "winrate": number_value(winrate),
            "stability": number_value(stability),
            "drawdown": number_value(drawdown),
            "tags": number_value(tags),
            "pnl": number_value(pnl),
            "followFriendly": follow_friendly,
        },
        "rank": rank,
        "item": item,
    })
}

/// `let r=q=>{let K=typeof q=="string"?parseFloat(q):q??0;return isNaN(K)?0:K}` —
/// a numeric field may arrive as a JSON number or as a numeric string.
fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => parse_float(text),
        _ => 0.0,
    }
}

/// `parseFloat` reads the longest numeric prefix; the fields here are short, so the
/// shrinking-prefix scan is cheaper than a hand-written number grammar.
fn parse_float(text: &str) -> f64 {
    let text = text.trim_start();
    let mut end = text.len();
    while end > 0 {
        if let Ok(value) = text[..end].parse::<f64>() {
            return value;
        }
        end -= 1;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
    }
    0.0
}

/// `Math.round(x*10)/10`
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// `JSON.stringify` prints integral doubles without a fractional part; match that so the
/// JSON surface lines up with the JavaScript CLI.
fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// `unwrap(n,"Leaderboard query failed")?.data??[]` — the query rows are nested one level
/// deeper than every other leaderboard payload. A bare array is also accepted so a raw
/// array payload is never silently reduced to an empty page.
fn query_rows(data: Value) -> Result<Vec<Value>, String> {
    match data {
        Value::Null => Ok(vec![]),
        Value::Array(rows) => Ok(rows),
        Value::Object(mut map) => match map.remove("data") {
            None | Some(Value::Null) => Ok(vec![]),
            Some(Value::Array(rows)) => Ok(rows),
            Some(_) => Err("invalid leaderboard response".into()),
        },
        _ => Err("invalid leaderboard response".into()),
    }
}

/// `this.unwrap(n,...)??[]` for the leaves whose payload already is the list.
fn list_or_empty(data: Value) -> Value {
    if data.is_null() {
        json!([])
    } else {
        data
    }
}

fn chain(value: &str) -> Result<String, String> {
    if CHAINS.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err("unsupported chain id".into())
    }
}

fn period(value: &str) -> Result<String, String> {
    if PERIODS.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err("period must be 7d, 30d or 90d".into())
    }
}

fn tag(value: &str) -> Result<String, String> {
    let upper = value.to_ascii_uppercase();
    if TAGS.contains(&upper.as_str()) {
        Ok(upper)
    } else {
        Err("tag must be ALL, KOL or MPC".into())
    }
}

fn page(value: i64) -> Result<i64, String> {
    if value < 0 {
        Err("page must be a non-negative integer".into())
    } else {
        Ok(value)
    }
}

fn page_size(value: i64) -> Result<i64, String> {
    if (1..=20).contains(&value) {
        Ok(value)
    } else {
        Err("size must be between 1 and 20".into())
    }
}

/// `--config` is opaque to the CLI: both bundle leaves do
/// `JSON.parse(--config)` then `Array.isArray(s)||T(...)`, then send the parsed array
/// verbatim. Item field names are therefore undetermined by the bundle and pass through
/// unchanged.
fn config_array(raw: &str) -> Result<Vec<Value>, String> {
    let parsed: Value =
        serde_json::from_str(raw).map_err(|_| "invalid --config JSON".to_string())?;
    match parsed {
        Value::Array(items) => Ok(items),
        _ => Err("--config must be a JSON array".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[derive(Parser)]
    struct Leaf {
        #[command(subcommand)]
        command: LeaderboardCommand,
    }

    fn parse(argv: &[&str]) -> Result<LeaderboardCommand, clap::Error> {
        Leaf::try_parse_from([&["leaderboard"][..], argv].concat()).map(|leaf| leaf.command)
    }

    fn credentials() -> crate::session::Credentials {
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    fn envelope(data: Value) -> String {
        json!({"code":"000000","data":data}).to_string()
    }

    /// Serves one canned response body per expected request and returns every raw request
    /// (request line + headers + body) it saw, in order.
    async fn server(bodies: Vec<String>) -> (Client, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = vec![];
            for body in bodies {
                let accepted =
                    tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                        .await
                        .expect("client must connect");
                let (mut socket, _) = accepted.unwrap();
                let request = read_request(&mut socket).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.unwrap();
                socket.write_all(body.as_bytes()).await.unwrap();
                requests.push(request);
            }
            requests
        });
        (Client::test_client(credentials(), &origin).unwrap(), task)
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 1024];
        let mut head = None;
        while head.is_none() {
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0, "peer closed before headers");
            bytes.extend_from_slice(&buf[..n]);
            head = bytes.windows(4).position(|w| w == b"\r\n\r\n");
        }
        let head = head.unwrap() + 4;
        let headers = String::from_utf8_lossy(&bytes[..head]).to_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while bytes.len() < head + length {
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0, "peer closed inside body");
            bytes.extend_from_slice(&buf[..n]);
        }
        String::from_utf8(bytes).unwrap()
    }

    fn line(request: &str) -> &str {
        request.lines().next().unwrap()
    }

    fn body(request: &str) -> Value {
        let (head, body) = request.split_once("\r\n\r\n").expect("complete request");
        assert!(!head.is_empty());
        serde_json::from_str(body).expect("json body")
    }

    fn query_args() -> QueryArgs {
        QueryArgs {
            chain_id: "56".into(),
            period: "30d".into(),
            tag: "all".into(),
            sort_by: 30,
            order_by: 1,
            page: 2,
            size: 7,
        }
    }

    fn analyze_args() -> AnalyzeArgs {
        AnalyzeArgs {
            chain_id: "56".into(),
            address: "0xTarget".into(),
            period: "7d".into(),
            top_n: 40,
        }
    }

    fn radar_args() -> AlphaRadarArgs {
        AlphaRadarArgs {
            chain_id: "CT_501".into(),
            tokens: " So111 , ,TokenB ".into(),
            match_count: 2,
            period: "90d".into(),
            page: 1,
            size: 5,
        }
    }

    fn radar_save_args() -> RadarConfigSaveArgs {
        RadarConfigSaveArgs {
            chain_id: "8453".into(),
            config: r#"[{"name":"alpha","matchTokenCount":2,"tokenAddressList":["0xa"]}]"#.into(),
        }
    }

    fn preset_save_args() -> PresetSaveArgs {
        PresetSaveArgs {
            config: r#"[{"name":"whales","period":"7d"},{"name":"wins","period":"30d"}]"#.into(),
        }
    }

    fn row(address: &str) -> Value {
        json!({"address":address,"winRate":"0.5","realizedPnl":"1.23456789"})
    }

    #[tokio::test]
    async fn query_sends_exact_get_query_and_returns_nested_rows() {
        let rows = vec![row("0xAAA"), row("0xBBB")];
        let (mut client, task) = server(vec![envelope(json!({"data":rows,"total":2}))]).await;
        let data = run(&mut client, &LeaderboardCommand::Query(query_args()))
            .await
            .unwrap();
        assert_eq!(data, json!(rows));
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "GET /bapi/defi/v1/public/wallet-direct/market/leaderboard/query?chainId=56&period=30d&tag=ALL&sortBy=30&orderBy=1&pageNo=2&pageSize=7 HTTP/1.1"
        );
        assert_eq!(requests[0].split("\r\n\r\n").nth(1), Some(""));
    }

    #[tokio::test]
    async fn query_missing_nested_data_is_an_empty_page_not_a_wire_failure() {
        let (mut client, task) = server(vec![envelope(json!({"total":0}))]).await;
        assert_eq!(
            run(&mut client, &LeaderboardCommand::Query(query_args()))
                .await
                .unwrap(),
            json!([])
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn analyze_scans_pages_and_scores_the_match_locally() {
        let target = json!({
            "address":"0xTARGET",
            "winRate":"0.8",
            "dailyPNL":[{"realizedPnl":"1"},{"realizedPnl":"-2"},{"realizedPnl":3.5}],
            "tokenDistribution":{"gt500Cnt":1,"between0And500Cnt":1,"between0AndNegative50Cnt":1,"ltNegative50Cnt":1},
            "tags":["smart"],
            "genericAddressTagList":[],
            "realizedPnlPercent":"50",
            "totalTxCnt":100,
            "unknownField":["kept"]
        });
        let (mut client, task) = server(vec![
            envelope(json!({"data":[row("0xAAA")]})),
            envelope(json!({"data":[target.clone()]})),
        ])
        .await;
        let data = run(&mut client, &LeaderboardCommand::Analyze(analyze_args()))
            .await
            .unwrap();
        assert_eq!(
            data,
            json!({
                "address":"0xTarget",
                "score":63.3,
                "rating":"\u{2b50}",
                "breakdown":{
                    "winrate":20,
                    "stability":13.3,
                    "drawdown":10,
                    "tags":5,
                    "pnl":5,
                    "followFriendly":10
                },
                "rank":2,
                "item":target
            })
        );
        let requests = task.await.unwrap();
        for (index, request) in requests.iter().enumerate() {
            assert_eq!(
                line(request),
                format!(
                    "GET /bapi/defi/v1/public/wallet-direct/market/leaderboard/query?chainId=56&period=7d&tag=ALL&sortBy=0&orderBy=0&pageNo={index}&pageSize=20 HTTP/1.1"
                )
            );
        }
    }

    #[tokio::test]
    async fn analyze_distinguishes_empty_leaderboard_from_an_unranked_address() {
        let (mut client, task) = server(vec![
            envelope(json!({"data":[]})),
            envelope(json!({"data":[]})),
        ])
        .await;
        assert_eq!(
            run(&mut client, &LeaderboardCommand::Analyze(analyze_args()))
                .await
                .unwrap_err(),
            "leaderboard is empty"
        );
        task.await.unwrap();
        let (mut client, task) = server(vec![
            envelope(json!({"data":[row("0xAAA")]})),
            envelope(json!({"data":[row("0xBBB")]})),
        ])
        .await;
        let error = run(&mut client, &LeaderboardCommand::Analyze(analyze_args()))
            .await
            .unwrap_err();
        assert_eq!(error, "address not ranked in requested top N");
        assert!(!error.contains("0x"));
        task.await.unwrap();
    }

    #[tokio::test]
    async fn analyze_skips_a_page_whose_head_was_already_scanned() {
        // `if(c.length>0){let v=P[0]?.address...;if(v&&c.some(k=>...===v))continue}`
        let (mut client, task) = server(vec![
            envelope(json!({"data":[row("0xAAA"), row("0xBBB")]})),
            envelope(json!({"data":[row("0xaaa"), row("0xCCC")]})),
        ])
        .await;
        assert_eq!(
            run(&mut client, &LeaderboardCommand::Analyze(analyze_args()))
                .await
                .unwrap_err(),
            "address not ranked in requested top N"
        );
        assert_eq!(task.await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn alpha_radar_posts_the_documented_body_and_returns_the_payload() {
        let rows = json!([{"address":"So111","marghedTokens":[]}]);
        let (mut client, task) = server(vec![envelope(rows.clone())]).await;
        let data = run(&mut client, &LeaderboardCommand::AlphaRadar(radar_args()))
            .await
            .unwrap();
        assert_eq!(data, rows);
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "POST /bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/query HTTP/1.1"
        );
        assert_eq!(
            body(&requests[0]),
            json!({
                "chainId":"CT_501",
                "period":"90d",
                "tokenAddressList":["So111","TokenB"],
                "matchTokenCount":2,
                "pageNo":1,
                "pageSize":5
            })
        );
    }

    #[tokio::test]
    async fn preset_list_posts_an_empty_object_and_returns_the_array() {
        let presets = json!([{"name":"whales","period":"7d"}]);
        let (mut client, task) = server(vec![envelope(presets.clone())]).await;
        let data = run(
            &mut client,
            &LeaderboardCommand::Preset {
                command: PresetCommand::List,
            },
        )
        .await
        .unwrap();
        assert_eq!(data, presets);
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "POST /bapi/defi/v1/public/wallet-direct/agent/market/leaderboard/preset/list HTTP/1.1"
        );
        assert_eq!(body(&requests[0]), json!({}));
    }

    #[tokio::test]
    async fn preset_save_posts_the_parsed_array_and_reports_the_count() {
        let (mut client, task) = server(vec![envelope(json!(true))]).await;
        let data = run(
            &mut client,
            &LeaderboardCommand::Preset {
                command: PresetCommand::Save(preset_save_args()),
            },
        )
        .await
        .unwrap();
        assert_eq!(data, json!({"success":true,"count":2}));
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "POST /bapi/defi/v1/public/wallet-direct/agent/market/leaderboard/preset/save HTTP/1.1"
        );
        assert_eq!(
            body(&requests[0]),
            json!({"presetList":[{"name":"whales","period":"7d"},{"name":"wins","period":"30d"}]})
        );
    }

    #[tokio::test]
    async fn radar_config_list_posts_only_the_chain_id() {
        let configs =
            json!([{"name":"alpha","uuid":"u1","matchTokenCount":2,"tokenAddressList":["0xa"]}]);
        let (mut client, task) = server(vec![envelope(configs.clone())]).await;
        let data = run(
            &mut client,
            &LeaderboardCommand::AlphaRadarConfig {
                command: RadarConfigCommand::List(RadarConfigListArgs {
                    chain_id: "56".into(),
                }),
            },
        )
        .await
        .unwrap();
        assert_eq!(data, configs);
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "POST /bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/config/list HTTP/1.1"
        );
        assert_eq!(body(&requests[0]), json!({"chainId":"56"}));
    }

    #[tokio::test]
    async fn radar_config_save_posts_chain_and_config_list() {
        let (mut client, task) = server(vec![envelope(json!(true))]).await;
        let data = run(
            &mut client,
            &LeaderboardCommand::AlphaRadarConfig {
                command: RadarConfigCommand::Save(radar_save_args()),
            },
        )
        .await
        .unwrap();
        assert_eq!(data, json!({"success":true,"count":1}));
        let requests = task.await.unwrap();
        assert_eq!(
            line(&requests[0]),
            "POST /bapi/defi/v1/public/wallet-direct/agent/market/alpharadar/config/save HTTP/1.1"
        );
        assert_eq!(
            body(&requests[0]),
            json!({
                "chainId":"8453",
                "alphaRadarConfigList":[{"name":"alpha","matchTokenCount":2,"tokenAddressList":["0xa"]}]
            })
        );
    }

    #[tokio::test]
    async fn mutation_classification_covers_every_leaf_exactly() {
        let reads = [
            LeaderboardCommand::Query(query_args()),
            LeaderboardCommand::Analyze(analyze_args()),
            LeaderboardCommand::AlphaRadar(radar_args()),
            LeaderboardCommand::Preset {
                command: PresetCommand::List,
            },
            LeaderboardCommand::AlphaRadarConfig {
                command: RadarConfigCommand::List(RadarConfigListArgs {
                    chain_id: "56".into(),
                }),
            },
        ];
        for leaf in &reads {
            assert!(!is_mutation(leaf), "{leaf:?} must stay a read");
            assert_eq!(mutation_payload(leaf).unwrap_err(), "not a mutation");
        }
        let preset = LeaderboardCommand::Preset {
            command: PresetCommand::Save(preset_save_args()),
        };
        assert!(is_mutation(&preset));
        assert_eq!(
            mutation_payload(&preset).unwrap(),
            json!({
                "operation":"leaderboard.preset.save",
                "presetList":[{"name":"whales","period":"7d"},{"name":"wins","period":"30d"}]
            })
        );
        let radar = LeaderboardCommand::AlphaRadarConfig {
            command: RadarConfigCommand::Save(radar_save_args()),
        };
        assert!(is_mutation(&radar));
        let payload = mutation_payload(&radar).unwrap();
        assert_eq!(
            payload,
            json!({
                "operation":"leaderboard.alpha-radar-config.save",
                "chainId":"8453",
                "alphaRadarConfigList":[{"name":"alpha","matchTokenCount":2,"tokenAddressList":["0xa"]}]
            })
        );
        assert_eq!(payload, mutation_payload(&radar).unwrap());
        let text = payload.to_string();
        assert!(!text.contains("synthetic"));
    }

    #[test]
    fn mutation_payload_rejects_config_that_is_not_a_json_array() {
        for config in ["not json", "{\"presetList\":[]}", "\"scalar\""] {
            let command = LeaderboardCommand::Preset {
                command: PresetCommand::Save(PresetSaveArgs {
                    config: config.into(),
                }),
            };
            assert!(mutation_payload(&command).is_err());
            let command = LeaderboardCommand::AlphaRadarConfig {
                command: RadarConfigCommand::Save(RadarConfigSaveArgs {
                    chain_id: "56".into(),
                    config: config.into(),
                }),
            };
            assert!(matches!(
                mutation_payload(&command).unwrap_err().as_str(),
                "invalid --config JSON" | "--config must be a JSON array"
            ));
        }
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let body =
            json!({"code":"123456","message":"secret backend detail","data":null}).to_string();
        let (mut client, task) = server(vec![body]).await;
        let error = run(&mut client, &LeaderboardCommand::Query(query_args()))
            .await
            .unwrap_err();
        assert_eq!(error, "Binance API code 123456");
        assert!(!error.contains("secret"));
        task.await.unwrap();
    }

    #[test]
    fn missing_or_invalid_required_input_never_reaches_the_network() {
        for argv in [
            vec!["query"],
            vec!["alpha-radar", "--chain-id", "56", "--tokens", "0xa"],
            vec!["alpha-radar", "--chain-id", "56", "--match-count", "1"],
            vec!["analyze", "--chain-id", "56"],
            vec!["preset", "save"],
            vec!["alpha-radar-config", "list"],
            vec!["alpha-radar-config", "save", "--chain-id", "56"],
        ] {
            let error = parse(&argv).expect_err("must not parse");
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::MissingRequiredArgument,
                "{argv:?}"
            );
        }
        for argv in [
            vec!["analyze", "--chain-id", "56", "--address", "  "],
            vec!["query", "--chain-id", " "],
        ] {
            assert_eq!(
                parse(&argv)
                    .expect_err("empty value must be rejected")
                    .kind(),
                clap::error::ErrorKind::ValueValidation,
                "{argv:?}"
            );
        }
    }

    #[tokio::test]
    async fn unsupported_chain_and_bad_filters_fail_before_connecting() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &origin).unwrap();
        let mut unsupported = query_args();
        unsupported.chain_id = "999".into();
        assert_eq!(
            run(&mut client, &LeaderboardCommand::Query(unsupported))
                .await
                .unwrap_err(),
            "unsupported chain id"
        );
        let mut bad_size = query_args();
        bad_size.size = 21;
        assert_eq!(
            run(&mut client, &LeaderboardCommand::Query(bad_size))
                .await
                .unwrap_err(),
            "size must be between 1 and 20"
        );
        let mut bad_period = radar_args();
        bad_period.period = "1d".into();
        assert_eq!(
            run(&mut client, &LeaderboardCommand::AlphaRadar(bad_period))
                .await
                .unwrap_err(),
            "period must be 7d, 30d or 90d"
        );
        let mut too_many = radar_args();
        too_many.match_count = 3;
        assert_eq!(
            run(&mut client, &LeaderboardCommand::AlphaRadar(too_many))
                .await
                .unwrap_err(),
            "match count must be between 1 and the number of tokens"
        );
        let mut no_tokens = radar_args();
        no_tokens.tokens = " , ".into();
        assert_eq!(
            run(&mut client, &LeaderboardCommand::AlphaRadar(no_tokens))
                .await
                .unwrap_err(),
            "at least one token address required"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "no request may reach the wire"
        );
    }

    #[test]
    fn analyze_clamps_top_n_to_the_documented_range() {
        for (value, expected) in [(0, 1), (-5, 1), (40, 40), (9_000, MAX_TOP_N)] {
            let args = AnalyzeArgs {
                top_n: value,
                ..analyze_args()
            };
            assert_eq!(args.top_n.clamp(1, MAX_TOP_N), expected);
        }
    }

    #[test]
    fn scoring_matches_the_bundle_formula_on_edge_rows() {
        let flat = json!({"address":"0x1"});
        let scored = score_item(&flat, 1, "0x1");
        assert_eq!(scored["score"], json!(0));
        assert_eq!(scored["rating"], json!("\u{274c}"));
        assert_eq!(scored["breakdown"]["followFriendly"], json!(0));
        let perfect = json!({
            "address":"0x2",
            "winRate":"1",
            "dailyPNL":[{"realizedPnl":"5"}],
            "tokenDistribution":{"gt500Cnt":3},
            "tags":["a","b"],
            "genericAddressTagList":["c","d","e"],
            "realizedPnlPercent":"500",
            "totalTxCnt":20
        });
        let scored = score_item(&perfect, 7, "0x2");
        assert_eq!(scored["score"], json!(100));
        assert_eq!(scored["rating"], json!("\u{2b50}\u{2b50}\u{2b50}"));
        assert_eq!(scored["breakdown"]["tags"], json!(15));
        assert_eq!(scored["breakdown"]["pnl"], json!(10));
        assert_eq!(scored["rank"], json!(7));
    }
}
