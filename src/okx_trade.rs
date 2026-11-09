//! Optional OKX V6 exact-in trades. No fallback; bounded approval is explicitly opt-in.
use crate::{
    client::Client,
    route_action, signing,
    trade::{MarketCommand, QuoteArgs, SwapArgs},
};
use num_bigint::{BigInt, BigUint, Sign};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const NATIVE: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const PREFIX: &str = "/api/v6/dex/aggregator/";

#[path = "okx_approval.rs"]
mod approval;
#[path = "okx_prepared.rs"]
mod prepared;
#[path = "okx_tax.rs"]
mod tax;
use crate::execution::OperationGuard;
use approval::validate_rpc_url;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    #[cfg(test)]
    #[serde(default)]
    test_origin: Option<String>,
    credential_file: PathBuf,
    chain_index: String,
    #[serde(default)]
    expected_spender: Option<String>,
    #[serde(default)]
    allowed_router_addresses: Vec<String>,
    #[serde(default = "default_gas")]
    max_gas_limit: u64,
    #[serde(default = "default_impact")]
    max_price_impact_percent: String,
    /// Private RPC URL, required only for ERC-20 --auto-approve; never echoed.
    #[serde(default)]
    rpc_url: Option<String>,
    #[serde(default = "default_approval_wait")]
    approval_wait_ms: u64,
    #[serde(default = "default_confirmations")]
    approval_confirmations: u64,
}
fn default_approval_wait() -> u64 {
    60_000
}
fn default_confirmations() -> u64 {
    2
}
fn default_gas() -> u64 {
    2_000_000
}
fn default_impact() -> String {
    "2".into()
}

fn private_read(path: &Path) -> Result<Vec<u8>, String> {
    if !path.is_absolute() {
        return Err("OKX profile/credential paths must be absolute".into());
    }
    let parent = path.parent().ok_or("OKX path needs a private parent")?;
    let p = std::fs::metadata(parent).map_err(|_| "cannot inspect OKX parent directory")?;
    if p.uid() != unsafe { libc::geteuid() }
        || p.mode() & 0o7777 != 0o700
        || parent.canonicalize().ok().as_deref() != Some(parent)
    {
        return Err("OKX parent must be owner-owned 0700 without symlink aliases".into());
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "cannot open OKX private file")?;
    let m = f
        .metadata()
        .map_err(|_| "cannot inspect OKX private file")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.len() > 65536
    {
        return Err("OKX file must be owner-owned regular 0600, single-link and bounded".into());
    }
    let mut bytes = Vec::new();
    f.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read OKX private file")?;
    if bytes.len() > 65536 {
        return Err("OKX private file too large".into());
    }
    Ok(bytes)
}
fn uint(raw: &str) -> Result<BigUint, String> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected decimal uint256 units".into());
    }
    crate::uint::uint256(raw)
}
pub(crate) fn percentage(raw: &str) -> Result<BigUint, String> {
    let n = amount_units(raw, 8, false)?;
    if n >= BigUint::from(100u32) * BigUint::from(10u32).pow(8) {
        return Err("OKX percentage must be 0..<100".into());
    }
    Ok(n)
}
pub(crate) fn amount_units(raw: &str, decimals: u8, positive: bool) -> Result<BigUint, String> {
    if decimals > 36 || raw.len() > 128 {
        return Err("unsupported decimals or amount length".into());
    }
    let (whole, frac) = raw.split_once('.').unwrap_or((raw, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || frac.len() > usize::from(decimals)
        || (raw.contains('.') && frac.is_empty())
    {
        return Err("amount must be exact decimal with no excess fractional digits".into());
    }
    let digits = format!(
        "{whole}{frac}{}",
        "0".repeat(usize::from(decimals) - frac.len())
    );
    let n = BigUint::parse_bytes(digits.as_bytes(), 10).ok_or("invalid amount")?;
    if n.bits() > 256 || (positive && n.bits() == 0) {
        return Err("amount must fit positive uint256".into());
    }
    Ok(n)
}
fn display_units(n: &BigUint, decimals: u8) -> String {
    if decimals == 0 {
        return n.to_string();
    }
    let s = format!(
        "{:0>width$}",
        n.to_string(),
        width = usize::from(decimals) + 1
    );
    let (a, b) = s.split_at(s.len() - usize::from(decimals));
    let b = b.trim_end_matches('0');
    if b.is_empty() {
        a.into()
    } else {
        format!("{a}.{b}")
    }
}
fn token(raw: &str) -> String {
    if raw.eq_ignore_ascii_case("0x0000000000000000000000000000000000000000") {
        NATIVE.into()
    } else {
        raw.to_ascii_lowercase()
    }
}
fn is_native(raw: &str) -> bool {
    token(raw) == NATIVE
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("OKX missing/invalid {key}"))
}
fn address(raw: &str) -> Result<(), String> {
    signing::address("56", raw)?;
    if raw[2..].bytes().all(|b| b == b'0') {
        return Err("zero address forbidden".into());
    }
    Ok(())
}

pub(crate) fn validate_quote_options(a: &QuoteArgs) -> Result<(), String> {
    if !matches!(a.binance_chain_id.as_str(), "1" | "56" | "8453" | "4663") {
        return Err("OKX market-order currently supports the existing EVM chain set only".into());
    }
    let path = a
        .okx_config
        .as_ref()
        .ok_or("--provider okx requires --okx-config")?;
    if !path.is_absolute() {
        return Err("--okx-config must be absolute".into());
    }
    signing::address(&a.binance_chain_id, &a.from_token)?;
    signing::address(&a.binance_chain_id, &a.to_token)?;
    let decimals = a
        .from_token_decimals
        .ok_or("OKX requires --fromTokenDecimals")?;
    amount_units(&a.from_token_qty, decimals, true)?;
    if is_native(&a.from_token) && decimals != 18 {
        return Err("native EVM coin requires 18 decimals".into());
    }
    percentage(&a.slippage).map_err(|_| "OKX requires explicit --slippage percent (not auto)")?;
    tax::validate_caps(a)?;
    if token(&a.from_token) == token(&a.to_token) {
        return Err("identical swap tokens".into());
    }
    Ok(())
}
pub(crate) fn validate_swap_options(a: &SwapArgs) -> Result<(), String> {
    tax::validate_caps(&a.quote)?;
    if a.mev != "false" || !a.gas_level.eq_ignore_ascii_case("MEDIUM") {
        return Err("OKX signing path cannot promise MEV/gas-price selection; require --mev false and default gasLevel".into());
    }
    if !a.acknowledge_token_risk {
        return Err("OKX swap requires --acknowledge-token-risk (not an audit)".into());
    }
    if let Some(minimum) = a.okx_min_output_units.as_deref() {
        if uint(minimum)?.bits() == 0 {
            return Err("OKX minimum output must be positive".into());
        }
    }
    Ok(())
}

struct Okx {
    profile: Profile,
    profile_hash: String,
    credentials: route_action::Credentials,
    http: std::sync::Arc<crate::http_observe::Transport>,
    origin: String,
    rpc_lease: std::sync::OnceLock<crate::chain_rpc::Lease>,
}
impl Okx {
    #[cfg(test)]
    fn load(a: &QuoteArgs) -> Result<Self, String> {
        Self::load_with_http(a, Client::build_okx_transport()?)
    }
    fn load_with_http(
        a: &QuoteArgs,
        http: std::sync::Arc<crate::http_observe::Transport>,
    ) -> Result<Self, String> {
        validate_quote_options(a)?;
        let bytes = private_read(a.okx_config.as_deref().ok_or("missing OKX profile")?)?;
        let profile_hash = prepared::digest(&bytes);
        let profile: Profile =
            serde_json::from_slice(&bytes).map_err(|_| "invalid strict OKX profile")?;
        if profile.chain_index != a.binance_chain_id {
            return Err("OKX profile chain mismatch".into());
        }
        if !(21000..=5_000_000).contains(&profile.max_gas_limit) {
            return Err("OKX gas ceiling must be 21000..5000000".into());
        }
        percentage(&profile.max_price_impact_percent)?;
        if !(1000..=120_000).contains(&profile.approval_wait_ms)
            || !(1..=20).contains(&profile.approval_confirmations)
        {
            return Err("OKX approval wait must be 1000..120000 ms and confirmations 1..20".into());
        }
        if let Some(endpoint) = &profile.rpc_url {
            validate_rpc_url(endpoint)?;
        }
        for router in &profile.allowed_router_addresses {
            address(router)?;
        }
        if let Some(spender) = &profile.expected_spender {
            address(spender)?;
        }
        // Validate the credential path using the same stricter metadata contract, without exposing bytes.
        let _bytes = zeroize::Zeroizing::new(private_read(&profile.credential_file)?);
        let credentials = route_action::load_credentials(
            profile
                .credential_file
                .to_str()
                .ok_or("invalid credential path")?,
        )?;
        let origin = "https://web3.okx.com".to_owned();
        #[cfg(test)]
        let origin = if let Some(origin) = &profile.test_origin {
            let parsed = url::Url::parse(origin).map_err(|_| "invalid test OKX origin")?;
            if parsed.scheme() != "http"
                || parsed.host_str() != Some("127.0.0.1")
                || parsed.path() != "/"
                || parsed.query().is_some()
                || parsed.fragment().is_some()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err("test OKX origin must be loopback HTTP".into());
            }
            origin.clone()
        } else {
            origin
        };
        Ok(Self {
            profile,
            profile_hash,
            credentials,
            http,
            origin,
            rpc_lease: std::sync::OnceLock::new(),
        })
    }
    async fn get(&self, endpoint: &str, query: &[(&str, String)]) -> Result<Value, String> {
        self.get_observed(endpoint, query)
            .await
            .map(|(value, _)| value)
    }
    async fn get_observed(
        &self,
        endpoint: &str,
        query: &[(&str, String)],
    ) -> Result<(Value, crate::http_observe::Observation), String> {
        if !matches!(endpoint, "quote" | "swap" | "approve-transaction") {
            return Err("unsupported OKX endpoint".into());
        }
        let mut url = url::Url::parse(&format!("{}{PREFIX}{endpoint}", self.origin))
            .map_err(|_| "invalid fixed OKX origin")?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        let target = format!("{}?{}", url.path(), url.query().ok_or("missing OKX query")?);
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        use base64::Engine;
        let signature =
            base64::engine::general_purpose::STANDARD.encode(route_action::hmac_sha256(
                self.credentials.secret_key.as_bytes(),
                format!("{timestamp}GET{target}").as_bytes(),
            ));
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in [
            ("ok-access-key", self.credentials.api_key.as_str()),
            ("ok-access-passphrase", self.credentials.passphrase.as_str()),
            ("ok-access-timestamp", &timestamp),
            ("ok-access-sign", &signature),
        ] {
            let mut value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| "invalid OKX authentication field")?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        // No retries, redirects, proxies, payments, subscriptions or partner-fee parameters.
        let request = self.http.get(url).headers(headers);
        let _core_guard = (endpoint == "swap")
            .then(|| crate::task_timing::CoreGuard::begin(crate::task_timing::CoreRequest::Route));
        let probe = self.http.begin();
        let response = request.send().await;
        let observed = probe.finish(response.as_ref().ok());
        let mut response = response.map_err(|_| "OKX request failed (details redacted)")?;
        if !response.status().is_success() {
            return Err(format!(
                "OKX HTTP {}; no automatic retry/payment",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "OKX response read failed")?
        {
            if bytes.len().saturating_add(chunk.len()) > 1_048_576 {
                return Err("OKX response too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid OKX JSON")?;
        if v["code"] != "0" {
            return Err("OKX business error (details redacted)".into());
        }
        Ok((v, observed))
    }
    fn validate_quote(&self, q: &QuoteArgs, row: &Value) -> Result<Value, String> {
        if text(row, "chainIndex")? != q.binance_chain_id || text(row, "swapMode")? != "exactIn" {
            return Err("OKX quote chain/mode mismatch".into());
        }
        let amount = amount_units(
            &q.from_token_qty,
            q.from_token_decimals.ok_or("input decimals missing")?,
            true,
        )?;
        if uint(text(row, "fromTokenAmount")?)? != amount {
            return Err("OKX input amount mismatch".into());
        }
        for (key, expected) in [("fromToken", &q.from_token), ("toToken", &q.to_token)] {
            if token(text(&row[key], "tokenContractAddress")?) != token(expected) {
                return Err("OKX token echo mismatch".into());
            }
            if row[key]["isHoneyPot"] != false {
                return Err("OKX token risk missing or honeypot detected".into());
            }
        }
        let taxes = tax::Taxes::from_quote(q, row)?;
        let decimals = |key: &str| -> Result<u8, String> {
            text(&row[key], "decimal")?
                .parse::<u8>()
                .ok()
                .filter(|d| *d <= 36)
                .ok_or_else(|| "invalid OKX token decimals".into())
        };
        if Some(decimals("fromToken")?) != q.from_token_decimals {
            return Err("OKX input decimals mismatch".into());
        }
        let out = uint(text(row, "toTokenAmount")?)?;
        if out.bits() == 0 {
            return Err("zero OKX output".into());
        }
        // Current API documents sign conventions inconsistently across versions. Bound the absolute
        // magnitude conservatively rather than treating a negative value as permission to bypass it.
        let impact = text(row, "priceImpactPercent")?;
        let magnitude = percentage(impact.strip_prefix('-').unwrap_or(impact))?;
        if magnitude > percentage(&self.profile.max_price_impact_percent)? {
            return Err("OKX price impact exceeds profile limit".into());
        }
        Ok(
            json!({"provider":"okx","chainIndex":q.binance_chain_id,"swapMode":"exactIn",
            "fromToken":token(&q.from_token),"toToken":token(&q.to_token),
            "fromTokenAmount":amount.to_string(),"toTokenAmount":out.to_string(),
            "fromCoinAmount":q.from_token_qty,"toCoinAmount":display_units(&out,decimals("toToken")?),
            "fromCoinSymbol":row["fromToken"]["tokenSymbol"],"toCoinSymbol":row["toToken"]["tokenSymbol"],
            "priceImpactPercent":impact,"tradeFee":row["tradeFee"],"estimateGasFee":row["estimateGasFee"],
            "transferTax":taxes.summary(q),
            "limitations":["quote is not a fill or an on-chain hard minimum","OKX infrastructure fees and actual gas may differ; no partner fee is added locally"]}),
        )
    }
    fn route_config(&self, q: &QuoteArgs, sender: &str) -> Result<Value, String> {
        let spender = self
            .profile
            .expected_spender
            .as_ref()
            .ok_or("OKX swap requires pinned expected_spender in profile")?;
        if self.profile.allowed_router_addresses.is_empty() {
            return Err("OKX swap requires pinned allowed_router_addresses".into());
        }
        let amount = amount_units(
            &q.from_token_qty,
            q.from_token_decimals.ok_or("input decimals missing")?,
            true,
        )?;
        Ok(
            json!({"chain_index":q.binance_chain_id,"binance_chain_id":q.binance_chain_id,
            "from_token_address":token(&q.from_token),"to_token_address":token(&q.to_token),
            "amount":amount.to_string(),"slippage_percent":q.slippage,"user_wallet_address":sender,
            "expected_spender":spender,"allowed_router_addresses":self.profile.allowed_router_addresses,
            "max_value_wei":if is_native(&q.from_token){amount.to_string()}else{"0".into()},
            "max_gas_limit":self.profile.max_gas_limit,"max_price_impact_percent":self.profile.max_price_impact_percent}),
        )
    }
}
fn query(q: &QuoteArgs) -> Result<Vec<(&'static str, String)>, String> {
    Ok(vec![
        ("chainIndex", q.binance_chain_id.clone()),
        ("fromTokenAddress", token(&q.from_token)),
        ("toTokenAddress", token(&q.to_token)),
        (
            "amount",
            amount_units(
                &q.from_token_qty,
                q.from_token_decimals.ok_or("missing decimals")?,
                true,
            )?
            .to_string(),
        ),
        ("swapMode", "exactIn".into()),
    ])
}
fn single(v: &Value) -> Result<&Value, String> {
    v["data"]
        .as_array()
        .filter(|r| r.len() == 1)
        .map(|r| &r[0])
        .ok_or_else(|| "OKX must return exactly one result".into())
}
fn empty_array(v: &Value, key: &str) -> Result<(), String> {
    if v[key].as_array().is_none_or(|a| !a.is_empty()) {
        return Err(format!("preview {key} missing, malformed or nonempty"));
    }
    Ok(())
}
fn reject_nested_changes(v: &Value, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("preview nesting too deep".into());
    }
    if let Some(obj) = v.as_object() {
        for (key, value) in obj {
            if key == "balanceChanges" && depth != 1 {
                return Err("unexpected nested or top-level balance changes".into());
            }
            if matches!(
                key.as_str(),
                "allowanceChanges" | "authorityChanges" | "riskDetails" | "riskBehaviors"
            ) && value.as_array().is_none_or(|rows| !rows.is_empty())
            {
                return Err("preview contains risk or authority changes".into());
            }
            if key == "risks" && !value.is_object() {
                return Err("malformed preview risks".into());
            }
            if key == "addresses"
                && obj.contains_key("riskDetails")
                && value.as_object().is_none_or(|rows| !rows.is_empty())
            {
                return Err("preview contains address risks".into());
            }
            reject_nested_changes(value, depth + 1)?;
        }
    } else if let Some(rows) = v.as_array() {
        for row in rows {
            reject_nested_changes(row, depth + 1)?;
        }
    }
    Ok(())
}
fn validate_preview(
    v: &Value,
    q: &QuoteArgs,
    sender: &str,
    minimum: &BigUint,
    target: &str,
    spender: &str,
) -> Result<Value, String> {
    if v["parsedTx"]["contractAddress"]
        .as_str()
        .is_none_or(|a| !a.eq_ignore_ascii_case(target))
    {
        return Err("preview parsed contract differs from pinned swap router".into());
    }
    let risks = &v["risks"];
    empty_array(risks, "riskDetails")?;
    empty_array(risks, "riskBehaviors")?;
    if risks["addresses"].as_object().is_none_or(|a| !a.is_empty()) {
        return Err("preview address risk missing or present".into());
    }
    let sim = &v["simulationResult"];
    if sim["simulationCode"] != "000000000" {
        return Err("OKX swap requires explicit successful Binance simulation".into());
    }
    let amount = amount_units(
        &q.from_token_qty,
        q.from_token_decimals.ok_or("missing decimals")?,
        true,
    )?;
    approval::validate_consumption(
        &sim["allowanceChanges"],
        sender,
        &token(&q.from_token),
        spender,
        &amount,
    )?;
    let mut sanitized = v.clone();
    sanitized["simulationResult"]["allowanceChanges"] = json!([]);
    reject_nested_changes(&sanitized, 0)?;
    empty_array(sim, "authorityChanges")?;
    // Refuse unreviewed nested simulations instead of silently ignoring risks/deltas in wrappers.
    for object in [v, sim] {
        for key in ["simulation", "simulationResults", "simulationDetails"] {
            if object.get(key).is_some_and(|x| !x.is_null()) {
                return Err("unexpected nested preview simulation".into());
            }
        }
    }
    let rows = sim["balanceChanges"]
        .as_array()
        .ok_or("preview balance changes missing")?;
    let mut debited = BigInt::from(0);
    let mut received = BigInt::from(0);
    for row in rows {
        if row["owner"]
            .as_str()
            .is_none_or(|a| !a.eq_ignore_ascii_case(sender))
            || !row["tokenId"].is_null()
            || !matches!(
                row["tokenType"].as_str(),
                Some("Erc20" | "Native" | "NativeToken")
            )
        {
            return Err("unexpected preview balance owner/NFT/type".into());
        }
        let change = &row["change"];
        let raw = change
            .as_str()
            .map(str::to_owned)
            .or_else(|| change.is_number().then(|| change.to_string()))
            .ok_or("invalid balance delta")?;
        let magnitude = raw.strip_prefix('-').unwrap_or(&raw);
        uint(magnitude)?;
        let n = BigInt::parse_bytes(raw.as_bytes(), 10).ok_or("invalid integer delta")?;
        let t = token(text(row, "contractAddress")?);
        if t == token(&q.from_token) {
            debited -= n;
        } else if t == token(&q.to_token) {
            received += n;
        } else if n != BigInt::from(0) {
            return Err("preview changes an unrelated asset".into());
        }
    }
    if debited <= BigInt::from(0)
        || debited > BigInt::from_biguint(Sign::Plus, amount)
        || received < BigInt::from_biguint(Sign::Plus, minimum.clone())
    {
        return Err("preview input debit/output credit violates swap bounds".into());
    }
    for key in ["allowanceChanges", "authorityChanges"] {
        if v.get(key)
            .is_some_and(|x| x.as_array().is_none_or(|r| !r.is_empty()))
        {
            return Err("unexpected top-level authority/allowance changes".into());
        }
    }
    Ok(
        json!({"walletDebitUnits":debited.to_string(),"walletNetCreditUnits":received.to_string(),
        "minimumNetOutputUnits":minimum.to_string(),"basis":"successful_simulation_wallet_net_balance_delta",
        "notAFillOrFutureStateGuarantee":true}),
    )
}

pub(crate) async fn run(
    client: &mut Client,
    dir: &Path,
    command: &MarketCommand,
    journal: Option<&mut OperationGuard>,
) -> Result<Value, String> {
    if matches!(command, MarketCommand::Swap(_)) && journal.is_none() {
        return Err("OKX swap requires the parent durable operation guard".into());
    }
    let q = match command {
        MarketCommand::Quote(q) => q,
        MarketCommand::Swap(a) => &a.quote,
        MarketCommand::Intent(a) => return crate::trade::intent_summary(a),
    };
    let api = Okx::load_with_http(q, client.okx_transport()?)?;
    match command {
        MarketCommand::Quote(_) => {
            let mut params = query(q)?;
            params.push((
                "priceImpactProtectionPercent",
                api.profile.max_price_impact_percent.clone(),
            ));
            let (raw, observation) = api.get_observed("quote", &params).await?;
            let mut output = api.validate_quote(q, single(&raw)?)?;
            output["httpTransport"] =
                serde_json::to_value(observation).map_err(|_| "cannot encode HTTP observation")?;
            Ok(output)
        }
        MarketCommand::Swap(a) => swap(client, dir, a, &api, journal).await,
        MarketCommand::Intent(a) => crate::trade::intent_summary(a),
    }
}
async fn swap(
    client: &mut Client,
    dir: &Path,
    a: &SwapArgs,
    api: &Okx,
    mut journal: Option<&mut OperationGuard>,
) -> Result<Value, String> {
    validate_quote_options(&a.quote)?;
    validate_swap_options(a)?;
    let q = &a.quote;
    let phase = crate::execution::task_allowance();
    if let Some(crate::execution::TaskAllowance::Prepared(proof)) = phase {
        crate::execution::check_prepared_task()?;
        let policy = prepared::validate(api, a, client, &proof)?;
        let sender = policy.sender.clone();
        let mut attempted = false;
        let mut output = crate::execution::prepared_wallet_scope(
            policy,
            perform_swap(client, dir, a, api, &sender, journal, &mut attempted),
        )
        .await?;
        output["autoApproval"] = proof["autoApproval"].clone();
        output["allowancePreparedBeforeTrigger"] = json!(true);
        output["preparedWalletPolicyUsed"] = json!(true);
        return Ok(output);
    }
    crate::trade::safeguards(client, &q.binance_chain_id).await?;
    // Preparation/standalone commands derive the real sender; a fast task uses only its bound record.
    let (sender, expires) =
        signing::check_wallet_policy(client, &q.binance_chain_id, None, false).await?;
    let route = api.route_config(q, &sender)?;
    if !is_native(&q.from_token) {
        let raw = api
            .get(
                "approve-transaction",
                &[
                    ("chainIndex", q.binance_chain_id.clone()),
                    ("tokenContractAddress", token(&q.from_token)),
                    ("approveAmount", text(&route, "amount")?.into()),
                ],
            )
            .await?;
        if !text(single(&raw)?, "dexContractAddress")?
            .eq_ignore_ascii_case(text(&route, "expected_spender")?)
        {
            return Err(
                "OKX spender differs from pinned profile; no approval or swap submitted".into(),
            );
        }
    }
    let approval =
        match approval::prepare(client, dir, a, api, &sender, &route, journal.as_deref_mut())
            .await?
        {
            approval::Outcome::Continue(note) => note,
            approval::Outcome::Stop(output) => return Ok(output),
        };
    if matches!(
        crate::execution::task_allowance(),
        Some(crate::execution::TaskAllowance::Prepare)
    ) {
        return prepared::proof(api, a, client, &sender, expires, approval);
    }
    let mut attempted = false;
    let result = perform_swap(client, dir, a, api, &sender, journal, &mut attempted).await;
    match result {
        Ok(mut output) => {
            if !approval.is_null() {
                output["autoApproval"] = approval;
            }
            Ok(output)
        }
        Err(_) if !approval.is_null() => Ok(json!({"provider":"okx",
            "status":if attempted {"UNCERTAIN"}else{"SWAP_NOT_SUBMITTED"},
            "swapSubmissionAttempted":attempted,"autoApproval":approval,"retrySafe":false,
            "message":"swap did not complete after allowance processing; inspect the operation journal and approval before any new intent"})),
        Err(e) => Err(e),
    }
}

async fn perform_swap(
    client: &mut Client,
    dir: &Path,
    a: &SwapArgs,
    api: &Okx,
    sender: &str,
    mut journal: Option<&mut OperationGuard>,
    attempted: &mut bool,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let q = &a.quote;
    // Standalone commands recheck after approval. Prepared tasks do not read remote state.
    if crate::execution::prepared_wallet().is_none() {
        crate::trade::safeguards(client, &q.binance_chain_id).await?;
    }
    let route = api.route_config(q, sender)?;
    let mut params = query(q)?;
    params.extend([
        ("slippagePercent", q.slippage.clone()),
        ("userWalletAddress", sender.to_owned()),
        ("swapReceiverAddress", sender.to_owned()),
        ("autoSlippage", "false".into()),
        (
            "priceImpactProtectionPercent",
            api.profile.max_price_impact_percent.clone(),
        ),
    ]);
    let fetch_started = std::time::Instant::now();
    let (raw, route_connection) = api.get_observed("swap", &params).await?;
    let route_fetch_ms = fetch_started.elapsed().as_secs_f64() * 1000.0;
    let validation_started = std::time::Instant::now();
    let row = single(&raw)?;
    let quote = api.validate_quote(q, &row["routerResult"])?;
    let taxes = tax::Taxes::from_quote(q, &row["routerResult"])?;
    let call = route_action::RouteActionBuilder::validate_only(&route, &raw)?;
    let note = &call["route_validation"];
    // Provider calldata is opaque under the versioned policy. All routes must still pass
    // the exact same fresh preview, wallet balance, risk and durable-binding checks below.
    // Optional fixed floor is bound as Option in the durable/prepared args. Absence
    // never means zero/1 wei: the fresh provider/slippage minimum remains mandatory.
    let minimum = a.okx_min_output_units.as_deref().map(uint).transpose()?;
    let reported = uint(text(&row["tx"], "minReceiveAmount")?)?;
    let out = uint(text(&quote, "toTokenAmount")?)?;
    let scale = BigUint::from(100u32) * BigUint::from(10u32).pow(8);
    let slippage_floor = &out * (&scale - percentage(&q.slippage)?) / &scale;
    if reported.bits() == 0
        || minimum.as_ref().is_some_and(|m| &reported < m)
        || reported < slippage_floor
        || reported > out
    {
        return Err("OKX reported minimum violates user floor/slippage bounds".into());
    }
    let minimum_policy = json!({"mode":if minimum.is_some(){"fresh_route_plus_fixed_floor"}else{"fresh_route"},"fixedUserMinimumUnits":minimum.as_ref().map(|m|m.to_string()),"reportedMinimumOutputUnits":reported.to_string(),"slippageReferenceMinimumUnits":slippage_floor.to_string(),"runtimeNetMinimumUnits":reported.to_string(),"onchainMinimumVerified":false});
    let amount = uint(text(&route, "amount")?)?;
    let expected_value = if is_native(&q.from_token) {
        amount
    } else {
        BigUint::from(0u8)
    };
    if uint(text(&call, "value")?)? != expected_value {
        return Err("OKX transaction value differs from exact input".into());
    }
    let args = signing::ContractPreview {
        binance_chain_id: q.binance_chain_id.clone(),
        from: sender.to_owned(),
        to: Some(text(&call, "to")?.into()),
        value: Some(text(&call, "value")?.into()),
        input_data: Some(text(&call, "input_data")?.into()),
        unsigned_tx: None,
        gas_limit: call["gas_limit"].as_u64(),
    };
    let route_validation_ms = validation_started.elapsed().as_secs_f64() * 1000.0;
    let preview_started = std::time::Instant::now();
    client.last_http = Default::default();
    let preview =
        signing::contract(client, dir, &signing::ContractCommand::Preview(args), None).await?;
    let preview_ms = preview_started.elapsed().as_secs_f64() * 1000.0;
    let preview_http = client.http_timing_for("contract_preview");
    let runtime_balances = validate_preview(
        &preview,
        q,
        sender,
        &reported,
        text(&call, "to")?,
        text(&route, "expected_spender")?,
    )?;
    if started.elapsed() > Duration::from_secs(60) {
        return Err("OKX swap preparation exceeded 60 seconds; not submitted".into());
    }
    let request_id = text(&preview, "requestId")?.to_owned();
    let digest = text(&preview, "previewSha256")?;
    if let Some(journal) = journal.as_deref_mut() {
        journal.checkpoint("swap_submitting", &json!({}))?;
    }
    *attempted = true;
    let before_execute_ms = started.elapsed().as_secs_f64() * 1000.0;
    let execute_started = std::time::Instant::now();
    client.last_http = Default::default();
    let result = signing::contract(
        client,
        dir,
        &signing::ContractCommand::Execute { request_id },
        Some(digest),
    )
    .await;
    let timings = json!({"routeFetchMs":route_fetch_ms,"routeValidationMs":route_validation_ms,
        "previewCallMs":preview_ms,"executeCallMs":execute_started.elapsed().as_secs_f64()*1000.0,
        "beforeExecuteCallMs":before_execute_ms,"buildThroughResponseMs":started.elapsed().as_secs_f64()*1000.0,
        "httpTransport":{"okxSwap":route_connection,"binancePreview":preview_http,"binanceExecute":client.http_timing_for("contract_execute")}});
    match result {
        Ok(mut output) => {
            if !output.is_object() {
                return Err(
                    "unexpected execute response; reconcile journal before any retry".into(),
                );
            }
            let hash = output
                .get("txHash")
                .and_then(Value::as_str)
                .filter(|raw| {
                    raw.len() == 66
                        && raw.starts_with("0x")
                        && raw[2..].bytes().all(|b| b.is_ascii_hexdigit())
                })
                .map(str::to_owned);
            if hash.is_none() && output["status"] != "PENDING_CONFIRMATION" {
                output["backendReportedStatus"] =
                    output.get("status").cloned().unwrap_or(Value::Null);
                output["status"] = json!("UNCERTAIN");
                output["message"]=json!("execute response has no valid transaction hash; order ID is not proof of submission; reconcile before any new action, never automatically resubmit");
            }
            if let (Some(journal), Some(hash)) = (journal, hash) {
                journal.checkpoint("swap_broadcast", &json!({"swapTxHash":hash}))?;
            }
            output["provider"] = json!("okx");
            output["hotPathTiming"] = timings;
            output["quote"] = quote;
            output["routeValidation"] = note.clone();
            output["retrySafe"] = json!(false);
            output["reportedMinimumOutputUnits"] = json!(reported.to_string());
            output["minimumOutputPolicy"] = minimum_policy;
            output["onchainMinimumDecoded"] = json!(false);
            output["onchainNetMinimumDecoded"] = json!(false);
            output["calldataPolicy"] = json!(route_action::CALLDATA_POLICY);
            output["transferTax"] = taxes.summary(q);
            output["runtimeBalanceValidation"] = runtime_balances;
            Ok(output)
        }
        Err(_) => Ok(
            json!({"provider":"okx","status":"UNCERTAIN","retrySafe":false,"hotPathTiming":timings,
            "calldataPolicy":route_action::CALLDATA_POLICY,"onchainMinimumDecoded":false,"onchainNetMinimumDecoded":false,
            "transferTax":taxes.summary(q),"runtimeBalanceValidation":runtime_balances,"minimumOutputPolicy":minimum_policy,
            "message":"execute outcome unknown; reconcile wallet operation journal and contract-call history; never automatically resubmit"}),
        ),
    }
}

#[cfg(test)]
#[path = "okx_trade_tests.rs"]
mod tests;
