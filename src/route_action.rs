//! Post-event, read-only OKX V6 swap construction. Never signs or submits a transaction.
use num_bigint::BigUint;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    chain_index: String,
    binance_chain_id: String,
    from_token_address: String,
    to_token_address: String,
    amount: String,
    slippage_percent: String,
    user_wallet_address: String,
    expected_spender: String,
    allowed_router_addresses: Vec<String>,
    allowed_selectors: Vec<String>,
    max_value_wei: String,
    max_gas_limit: u64,
    deadline_unix_ms: u64,
    timeout_ms: u64,
    connect_timeout_ms: u64,
    max_wait_ms: u64,
    min_request_interval_ms: u64,
    max_retries: u32,
    retry_backoff_ms: u64,
    credential_file: String,
    #[serde(default)]
    allow_zero_min_out: bool,
    #[serde(default)]
    require_runtime_preview: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    api_key: String,
    secret_key: String,
    passphrase: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.api_key.zeroize();
        self.secret_key.zeroize();
        self.passphrase.zeroize();
    }
}
fn load_credentials(path: &str) -> Result<Credentials, String> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use zeroize::Zeroizing;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open route credential file")?;
    let m = file
        .metadata()
        .map_err(|_| "cannot inspect route credential file")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != 0o600
        || m.len() > 65536
    {
        return Err(
            "route credential file must be owner-owned regular mode 0600 and bounded".into(),
        );
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read route credential file")?;
    if bytes.len() > 65536 {
        return Err("credential file too large".into());
    }
    let c: Credentials =
        serde_json::from_slice(&bytes).map_err(|_| "invalid route credential schema")?;
    for s in [&c.api_key, &c.secret_key, &c.passphrase] {
        if s.is_empty() || s.len() > 4096 || !s.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err("invalid route credential field".into());
        }
    }
    Ok(c)
}

pub struct RouteActionBuilder {
    config: Config,
    credentials: Credentials,
    client: reqwest::Client,
    origin: String,
    next_request: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

// RFC 2104 HMAC over SHA-256; long keys are hashed before block padding.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    use zeroize::Zeroizing;
    let mut block = Zeroizing::new([0u8; 64]);
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Zeroizing::new([0x36u8; 64]);
    let mut outer = Zeroizing::new([0x5cu8; 64]);
    for i in 0..64 {
        inner[i] ^= block[i];
        outer[i] ^= block[i];
    }
    let mut h = Sha256::new();
    h.update(&inner[..]);
    h.update(message);
    let digest = h.finalize();
    let mut h = Sha256::new();
    h.update(&outer[..]);
    h.update(digest);
    h.finalize().into()
}

fn uint(s: &str) -> Result<BigUint, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return Err("expected canonical decimal uint256 string".into());
    }
    let n = BigUint::parse_bytes(s.as_bytes(), 10).ok_or("invalid uint256")?;
    if n.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(n)
}
fn hex_shape(s: &str, bytes: usize) -> bool {
    s.len() == bytes * 2 + 2 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn eq_addr(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn unix_ms() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system time before epoch")?
        .as_millis()
        .try_into()
        .map_err(|_| "system time overflow".into())
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing/invalid OKX field {key}"))
}
fn check_echo(v: &Value, key: &str, expected: &str, address: bool) -> Result<(), String> {
    if let Some(actual) = v.get(key) {
        let actual = actual
            .as_str()
            .ok_or_else(|| format!("malformed OKX echo {key}"))?;
        if if address {
            !hex_shape(actual, 20) || !eq_addr(actual, expected)
        } else {
            actual != expected
        } {
            return Err(format!("OKX echo mismatch {key}"));
        }
    }
    Ok(())
}
struct DecodedV2 {
    recipient: String,
    minimum: BigUint,
    amount: BigUint,
    path: Vec<String>,
    deadline: BigUint,
}
fn decode_v2(input: &str) -> Result<DecodedV2, String> {
    let args = &input[10..];
    if args.len() < 8 * 64 || !args.len().is_multiple_of(64) {
        return Err("truncated/noncanonical V2 calldata".into());
    }
    let number = |i: usize| {
        BigUint::parse_bytes(&args.as_bytes()[i * 64..(i + 1) * 64], 16)
            .ok_or_else(|| "invalid V2 word".to_string())
    };
    let address = |i: usize| -> Result<String, String> {
        let w = &args[i * 64..(i + 1) * 64];
        if !w[..24].bytes().all(|b| b == b'0') {
            return Err("noncanonical ABI address".into());
        }
        Ok(format!("0x{}", &w[24..]))
    };
    if number(2)? != BigUint::from(160u32) {
        return Err("noncanonical V2 path offset".into());
    }
    let count = number(5)?
        .to_str_radix(10)
        .parse::<usize>()
        .map_err(|_| "V2 path count overflow")?;
    if !(2..=64).contains(&count) || args.len() != (6 + count) * 64 {
        return Err("invalid V2 path length/trailing bytes".into());
    }
    let mut path = Vec::new();
    for i in 6..6 + count {
        path.push(address(i)?);
    }
    Ok(DecodedV2 {
        recipient: address(3)?,
        minimum: number(1)?,
        amount: number(0)?,
        path,
        deadline: number(4)?,
    })
}
impl RouteActionBuilder {
    #[cfg(test)]
    fn new_with_origin(config: &Value, origin: &str) -> Result<Self, String> {
        Self::new(config)?.with_test_origin(origin)
    }
    #[cfg(test)]
    pub(crate) fn with_test_origin(mut self, origin: &str) -> Result<Self, String> {
        let u = url::Url::parse(origin).map_err(|_| "invalid mock origin")?;
        if u.scheme() != "http"
            || u.host_str() != Some("127.0.0.1")
            || !u.username().is_empty()
            || u.password().is_some()
            || u.path() != "/"
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err("loopback mock origin required".into());
        }
        self.origin = origin.trim_end_matches('/').to_owned();
        Ok(self)
    }
    /// Fetch one fresh swap construction after an event; cancellation drops the HTTP future.
    /// No event field may alter the configured wallet, amount, tokens, or permissions.
    pub async fn build(&self, _event: &Value) -> Result<Value, String> {
        let now = unix_ms()?;
        let budget = self
            .config
            .deadline_unix_ms
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or("route task deadline expired")?
            .min(self.config.max_wait_ms);
        tokio::time::timeout(Duration::from_millis(budget), self.fetch())
            .await
            .map_err(|_| "route deadline/max-wait exhausted".to_string())?
    }
    async fn fetch(&self) -> Result<Value, String> {
        let c = &self.config;
        let mut url = url::Url::parse(&format!("{}/api/v6/dex/aggregator/swap", self.origin))
            .map_err(|_| "invalid fixed OKX URL")?;
        url.query_pairs_mut().extend_pairs([
            ("chainIndex", c.chain_index.as_str()),
            ("fromTokenAddress", c.from_token_address.as_str()),
            ("toTokenAddress", c.to_token_address.as_str()),
            ("amount", c.amount.as_str()),
            ("slippagePercent", c.slippage_percent.as_str()),
            ("userWalletAddress", c.user_wallet_address.as_str()),
            ("swapMode", "exactIn"),
            ("autoSlippage", "false"),
        ]);
        let target = format!(
            "{}?{}",
            url.path(),
            url.query().ok_or("missing swap query")?
        );
        for attempt in 0..=c.max_retries {
            {
                let mut next = self.next_request.lock().await;
                if let Some(t) = *next {
                    tokio::time::sleep_until(t).await;
                }
                *next = Some(
                    tokio::time::Instant::now() + Duration::from_millis(c.min_request_interval_ms),
                );
            }
            if unix_ms()? >= c.deadline_unix_ms {
                return Err("route task deadline expired".into());
            }
            use base64::Engine;
            let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let signature = base64::engine::general_purpose::STANDARD.encode(hmac_sha256(
                self.credentials.secret_key.as_bytes(),
                format!("{timestamp}GET{target}").as_bytes(),
            ));
            let mut headers = reqwest::header::HeaderMap::new();
            for (name, value) in [
                ("ok-access-key", self.credentials.api_key.as_str()),
                ("ok-access-passphrase", self.credentials.passphrase.as_str()),
                ("ok-access-timestamp", timestamp.as_str()),
                ("ok-access-sign", signature.as_str()),
            ] {
                let mut v = reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| "invalid authentication field")?;
                v.set_sensitive(true);
                headers.insert(name, v);
            }
            let mut response = self
                .client
                .get(url.clone())
                .headers(headers)
                .send()
                .await
                .map_err(|_| "OKX swap request failed (details redacted)")?;
            let status = response.status();
            if status.as_u16() == 429 || status.is_server_error() {
                if attempt == c.max_retries {
                    return Err(format!(
                        "OKX HTTP {} retry budget exhausted",
                        status.as_u16()
                    ));
                }
                let backoff = c.retry_backoff_ms.saturating_mul(1u64 << attempt);
                let server_delay =
                    if let Some(h) = response.headers().get(reqwest::header::RETRY_AFTER) {
                        let s = h.to_str().map_err(|_| "malformed Retry-After")?;
                        if let Ok(seconds) = s.parse::<u64>() {
                            seconds.saturating_mul(1000)
                        } else {
                            let date = chrono::DateTime::parse_from_rfc2822(s)
                                .map_err(|_| "malformed Retry-After")?;
                            date.timestamp_millis()
                                .saturating_sub(
                                    i64::try_from(unix_ms()?).map_err(|_| "clock overflow")?,
                                )
                                .max(0) as u64
                        }
                    } else {
                        0
                    };
                let delay = server_delay.max(backoff);
                if delay > c.max_wait_ms {
                    return Err("OKX retry delay exceeds max-wait budget".into());
                }
                drop(response);
                tokio::time::sleep(Duration::from_millis(delay)).await;
                continue;
            }
            if !status.is_success() {
                return Err(format!("OKX HTTP {}", status.as_u16()));
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
            let data: Value = serde_json::from_slice(&bytes).map_err(|_| "malformed OKX JSON")?;
            let call = self.validate_response(&data)?;
            if unix_ms()? >= c.deadline_unix_ms {
                return Err("route task deadline expired".into());
            }
            return Ok(call);
        }
        Err("OKX retry budget exhausted".into())
    }
    fn validate_response(&self, v: &Value) -> Result<Value, String> {
        let c = &self.config;
        if v.get("code").and_then(Value::as_str) != Some("0") {
            return Err("OKX business code nonzero or malformed (message redacted)".into());
        }
        let rows = v
            .get("data")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 1)
            .ok_or("OKX data must contain exactly one swap")?;
        let row = &rows[0];
        let tx = row
            .get("tx")
            .filter(|t| t.is_object())
            .ok_or("missing OKX transaction")?;
        for obj in [row, tx] {
            if let Some(s) = obj.get("signatureData") {
                // OKX signals "nothing to sign" either by omitting the field or by echoing a
                // placeholder (`[""]`). Only real, non-blank entries mean the calldata needs an
                // extra signature this builder cannot produce.
                let entries = s.as_array().ok_or("malformed signature data")?;
                if entries
                    .iter()
                    .any(|v| v.as_str().is_none_or(|t| !t.trim().is_empty()))
                {
                    return Err(
                        "additional OKX signing data unsupported by EVM call builder".into(),
                    );
                }
            }
        }
        check_echo(tx, "slippagePercent", &c.slippage_percent, false)?;
        // OKX also returns an encoded approve transaction as a JSON string. Ignoring it would let a
        // response claim one spender while the approval payload targets another, so any present value
        // must parse and name exactly the configured spender; malformed or foreign values fail closed.
        for obj in [row, tx] {
            if let Some(raw) = obj.get("approveTransaction") {
                let encoded = raw
                    .as_str()
                    .ok_or("malformed approve transaction metadata")?;
                let inner: Value = serde_json::from_str(encoded)
                    .map_err(|_| "malformed approve transaction metadata")?;
                if !inner.is_object() {
                    return Err("malformed approve transaction metadata".into());
                }
                check_echo(&inner, "approveContract", &c.expected_spender, true)?;
                check_echo(&inner, "approveSpender", &c.expected_spender, true)?;
            }
        }
        for (container, key, expected, address) in [
            (row, "chainIndex", c.chain_index.as_str(), false),
            (tx, "chainId", c.chain_index.as_str(), false),
            (tx, "chainIndex", c.chain_index.as_str(), false),
            (tx, "recipient", c.user_wallet_address.as_str(), true),
            (
                tx,
                "userWalletAddress",
                c.user_wallet_address.as_str(),
                true,
            ),
            (row, "approveAddress", c.expected_spender.as_str(), true),
            (tx, "approveAddress", c.expected_spender.as_str(), true),
            (row, "approveContract", c.expected_spender.as_str(), true),
            (tx, "approveContract", c.expected_spender.as_str(), true),
        ] {
            check_echo(container, key, expected, address)?;
        }
        if let Some(router) = row.get("routerResult") {
            if !router.is_object() {
                return Err("malformed router result".into());
            }
            check_echo(router, "chainIndex", &c.chain_index, false)?;
            check_echo(router, "fromTokenAmount", &c.amount, false)?;
            for (key, expected) in [
                ("fromToken", &c.from_token_address),
                ("toToken", &c.to_token_address),
            ] {
                if let Some(token) = router.get(key) {
                    if !token.is_object() {
                        return Err("malformed token echo".into());
                    }
                    check_echo(token, "tokenContractAddress", expected, true)?;
                }
            }
            if let Some(n) = router.get("toTokenAmount") {
                uint(n.as_str().ok_or("invalid quote amount units")?)?;
            }
        }
        let from = field(tx, "from")?;
        let to = field(tx, "to")?;
        if !eq_addr(from, &c.user_wallet_address)
            || !c.allowed_router_addresses.iter().any(|a| eq_addr(a, to))
        {
            return Err("OKX transaction wallet/router mismatch".into());
        }
        let value = field(tx, "value")?;
        if uint(value)? > uint(&c.max_value_wei)? {
            return Err("OKX tx.value exceeds wei cap".into());
        }
        let gas_number = uint(field(tx, "gas")?)?;
        let gas = gas_number
            .to_str_radix(10)
            .parse::<u64>()
            .map_err(|_| "gas unit overflow")?;
        if gas == 0 || gas > c.max_gas_limit {
            return Err("OKX gas exceeds gas-unit cap".into());
        }
        if let Some(n) = tx.get("gasLimit") {
            if uint(n.as_str().ok_or("invalid gasLimit units")?)? != gas_number {
                return Err("conflicting gas/gasLimit units".into());
            }
        }
        // OKX returns a legacy `gasPrice` plus a `maxPriorityFeePerGas` hint and leaves
        // `maxFeePerGas` empty. These fields are never forwarded (the backend assigns gas), so the
        // only real ambiguity is a legacy price *and* an EIP-1559 max fee; a priority hint must
        // still fit under whichever ceiling is stated.
        let price = match (tx.get("gasPrice"), tx.get("maxFeePerGas")) {
            (Some(_), Some(_)) => return Err("ambiguous legacy/EIP1559 gas fields".into()),
            (Some(p), None) => Some(uint(p.as_str().ok_or("invalid gas price wei units")?)?),
            (None, Some(m)) => Some(uint(m.as_str().ok_or("invalid gas price wei units")?)?),
            (None, None) => None,
        };
        if let Some(priority) = tx.get("maxPriorityFeePerGas") {
            let priority = uint(priority.as_str().ok_or("invalid priority fee units")?)?;
            match price {
                Some(ceiling) if priority > ceiling => {
                    return Err("priority fee exceeds max fee".into())
                }
                Some(_) => {}
                None => return Err("priority fee without a fee ceiling".into()),
            }
        }
        let input = field(tx, "data")?;
        if input.len() < 10
            || input.len() > 262146
            || !input.starts_with("0x")
            || input.len() % 2 != 0
            || !input[2..].bytes().all(|b| b.is_ascii_hexdigit())
            || !c
                .allowed_selectors
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&input[..10]))
        {
            return Err("invalid or forbidden calldata selector".into());
        }
        let minimum = uint(field(tx, "minReceiveAmount")?)?;
        if minimum.bits() == 0 && !c.allow_zero_min_out {
            return Err("zero minimum output forbidden".into());
        }
        if let Some(quote) = row.pointer("/routerResult/toTokenAmount") {
            let quote = uint(quote.as_str().ok_or("invalid quote amount units")?)?;
            let (whole, fraction) = c
                .slippage_percent
                .split_once('.')
                .unwrap_or((&c.slippage_percent, ""));
            let scale = BigUint::from(10u32).pow(fraction.len() as u32);
            let percent = BigUint::from(100u32) * &scale;
            let slip = BigUint::parse_bytes(format!("{whole}{fraction}").as_bytes(), 10)
                .ok_or("invalid slippage")?;
            let floor = &quote * (&percent - slip) / &percent;
            if minimum > quote
                || (minimum < floor && !(minimum.bits() == 0 && c.allow_zero_min_out))
            {
                return Err(
                    "reported hard minimum violates requested slippage/quote bounds".into(),
                );
            }
        }
        if !input[..10].eq_ignore_ascii_case("0x38ed1739") {
            if !c.require_runtime_preview {
                return Err("unsupported calldata requires runtime policy preview".into());
            }
            return Ok(
                serde_json::json!({"binance_chain_id":c.binance_chain_id,"from":from,"to":to,"value":value,
                "input_data":input,"gas_limit":gas,"route_validation":{"recipient_verified":false,
                "calldata_min_out_verified":false,"requires_runtime_preview":true,
                "reported_minimum_output":minimum.to_str_radix(10),"limitations":[
                "unsupported selector: recipient, token path, input amount, deadline and actual on-chain minimum are NOT decoded or verified",
                "reported minimum is response metadata only; quote output is not a hard minimum",
                "engine MUST perform runtime policy preview binding recipient, tokens, amount, minimum and allowed spender before submission"]}}),
            );
        }
        let decoded = decode_v2(input)?;
        if !eq_addr(&decoded.recipient, &c.user_wallet_address)
            || decoded.minimum != minimum
            || decoded.amount != uint(&c.amount)?
            || !eq_addr(&decoded.path[0], &c.from_token_address)
            || !eq_addr(
                decoded.path.last().ok_or("empty V2 path")?,
                &c.to_token_address,
            )
            || decoded.deadline <= BigUint::from(unix_ms()? / 1000)
            || uint(value)?.bits() != 0
            || !eq_addr(to, &c.expected_spender)
        {
            return Err("calldata amount/path/recipient/deadline/minimum mismatch".into());
        }
        Ok(
            serde_json::json!({"binance_chain_id":c.binance_chain_id,"from":from,"to":to,"value":value,"input_data":input,"gas_limit":gas,
            "route_validation":{"recipient_verified":true,"calldata_min_out_verified":true,"requires_runtime_preview":false,
            "minimum_output":minimum.to_str_radix(10),"limitations":["allowlists identify trusted contracts; they do not prove deployed bytecode or successful execution"]}}),
        )
    }
    /// Syntactic/local configuration only. No request or quote is made here.
    pub fn new(config: &Value) -> Result<Self, String> {
        let c: Config = serde_json::from_value(config.clone())
            .map_err(|_| "invalid route config (strict fields/types)")?;
        if c.chain_index != c.binance_chain_id
            || uint(&c.chain_index)?.bits() == 0
            || c.chain_index == "501"
        {
            return Err("explicit matching EVM chain identifiers required".into());
        }
        for a in [
            &c.from_token_address,
            &c.to_token_address,
            &c.user_wallet_address,
            &c.expected_spender,
        ] {
            if !hex_shape(a, 20) || a[2..].bytes().all(|b| b == b'0') {
                return Err("invalid route address".into());
            }
        }
        if eq_addr(&c.from_token_address, &c.to_token_address) {
            return Err("identical route tokens".into());
        }
        if uint(&c.amount)?.bits() == 0 {
            return Err("amount must be positive".into());
        }
        uint(&c.max_value_wei)?;
        let parts: Vec<_> = c.slippage_percent.split('.').collect();
        if parts.len() > 2
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            || parts[0].len() > 2
            || parts[0].parse::<u32>().map_err(|_| "invalid slippage")? >= 100
            || (parts.len() == 2 && parts[1].len() > 8)
        {
            return Err("invalid slippage percent".into());
        }
        if c.allowed_router_addresses.is_empty()
            || c.allowed_router_addresses.iter().any(|a| !hex_shape(a, 20))
        {
            return Err("explicit router allowlist required".into());
        }
        if c.allowed_selectors.is_empty() || c.allowed_selectors.iter().any(|s| !hex_shape(s, 4)) {
            return Err("explicit selector allowlist required".into());
        }
        if [
            c.max_gas_limit,
            c.deadline_unix_ms,
            c.timeout_ms,
            c.connect_timeout_ms,
            c.max_wait_ms,
            c.min_request_interval_ms,
            c.retry_backoff_ms,
        ]
        .contains(&0)
            || c.max_retries > 5
            || c.credential_file.is_empty()
        {
            return Err("invalid route bounds".into());
        }
        if [
            c.timeout_ms,
            c.connect_timeout_ms,
            c.max_wait_ms,
            c.min_request_interval_ms,
            c.retry_backoff_ms,
        ]
        .iter()
        .any(|n| *n > 86_400_000)
        {
            return Err("route duration bounds must not exceed one day".into());
        }
        let credentials = load_credentials(&c.credential_file)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_millis(c.connect_timeout_ms))
            .timeout(Duration::from_millis(c.timeout_ms))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| "cannot construct persistent OKX client")?;
        Ok(Self {
            config: c,
            credentials,
            client,
            origin: "https://web3.okx.com".into(),
            next_request: tokio::sync::Mutex::new(None),
        })
    }
}
#[cfg(test)]
#[path = "route_action_tests.rs"]
mod tests;
