//! Post-event, read-only OKX V6 swap construction. Never signs or submits a transaction.
use crate::util::uint::uint256_canonical;
use num_bigint::BigUint;
use serde::Deserialize;
use serde_json::Value;

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
    max_value_wei: String,
    /// Absolute gas ceiling for this task. Omitted/0 means "use the built-in per-transaction
    /// ceiling": the cap only bounds the gas *limit* we hand to the backend, never the fee, so a
    /// wide ceiling costs nothing when unused and the chain still charges `gasUsed`.
    #[serde(default)]
    max_gas_limit: u64,
    /// OKX documents `priceImpactPercent` as a value the caller must check before submitting and
    /// block on when it is too high. Route tasks therefore carry the same threshold.
    #[serde(default = "default_max_price_impact_percent")]
    max_price_impact_percent: String,
    #[serde(default = "default_gas_buffer_percent")]
    gas_buffer_percent: u64,
    #[serde(default = "default_gas_buffer_absolute")]
    gas_buffer_absolute: u64,
    #[serde(default)]
    allow_zero_min_out: bool,
}

fn default_max_price_impact_percent() -> String {
    "2".into()
}
/// OKX's own documentation tells callers to raise the quoted gas limit by 50% before submitting.
fn default_gas_buffer_percent() -> u64 {
    300
}
fn default_gas_buffer_absolute() -> u64 {
    500_000
}
/// Hard per-transaction ceiling for the gas *limit*. Measured 2026-11-02: the same 1 U → BNB quote
/// ranged from 310,620 to 1,013,123 gas within hours, so a tight configured cap silently turned
/// viable tasks into "OKX gas exceeds gas-unit cap". The ceiling bounds what we are willing to put
/// in a transaction; it never sets the fee, because the chain charges `gasUsed`.
const GAS_CEILING: u64 = 5_000_000;
/// Part of mutation/preparation bindings: ABI-decoding intent must never be silently reused.
pub(crate) const CALLDATA_POLICY: &str = "pinned_okx_opaque_v3";
/// The ceiling actually applied: the configured value when set, else the built-in maximum.
fn gas_ceiling(c: &Config) -> u64 {
    if c.max_gas_limit == 0 {
        GAS_CEILING
    } else {
        c.max_gas_limit
    }
}

/// The provider quotes a point estimate for exactly the calldata it built. Measured headroom
/// over the real requirement ranged from 0.2% to 17%, so a quote that lands at the low end is
/// rejected by the backend simulation (`351805`) before a signature exists. Raising the cap
/// cannot raise the fee (the backend charges gas used, not the cap) and cannot weaken any
/// allowlist, so the headroom is safe to apply; `max_gas_limit` still bounds the result.
fn buffered_gas_limit(quoted: u64, percent: u64, absolute: u64, cap: u64) -> u64 {
    if quoted == 0 || cap == 0 {
        // Zero gas is unusable and a zero ceiling allows nothing: never return a limit above `cap`.
        return 0;
    }
    let scaled = quoted.saturating_mul(percent).saturating_div(100);
    let padded = quoted.saturating_add(absolute);
    scaled.max(padded).min(cap)
}

/// Parse a non-negative decimal string ("0.06", "2", "1.5") into (mantissa, scale). Returns None for
/// anything else, including signs and exponents, so a malformed threshold fails the load.
fn scaled_decimal(raw: &str) -> Option<(BigUint, u32)> {
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let mantissa = BigUint::parse_bytes(digits.as_bytes(), 10)?;
    Some((mantissa, fraction.len() as u32))
}

/// Compare two non-negative decimals exactly, without floating point.
fn decimal_greater(a: &str, b: &str) -> Option<bool> {
    let (am, as_) = scaled_decimal(a)?;
    let (bm, bs) = scaled_decimal(b)?;
    let scale = as_.max(bs);
    let lift = |m: BigUint, s: u32| m * BigUint::from(10u32).pow(scale - s);
    Some(lift(am, as_) > lift(bm, bs))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    pub(crate) api_key: String,
    pub(crate) secret_key: String,
    pub(crate) passphrase: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.api_key.zeroize();
        self.secret_key.zeroize();
        self.passphrase.zeroize();
    }
}
pub(crate) fn load_credentials(path: &str) -> Result<Credentials, String> {
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

/// Namespace for local route validation. The builder holds no transport: production calls
/// `validate_only` with a response obtained elsewhere, so no credentials, client or clock
/// state lives here.
pub struct RouteActionBuilder;

// RFC 2104 HMAC over SHA-256; long keys are hashed before block padding.
pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
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

fn hex_shape(s: &str, bytes: usize) -> bool {
    s.len() == bytes * 2 + 2 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn eq_addr(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
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
impl RouteActionBuilder {
    /// Pure validation for a caller that already owns a checked authenticated transport.
    pub(crate) fn validate_only(config: &Value, v: &Value) -> Result<Value, String> {
        Self::validate_with_config(&Self::configuration(config)?, v)
    }
    fn validate_with_config(c: &Config, v: &Value) -> Result<Value, String> {
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
                        "OKX swap requires an extra signature (signatureData); approve the input token first".into(),
                    );
                }
            }
        }
        check_echo(tx, "slippagePercent", &c.slippage_percent, false)?;
        // Documented as a number the caller must check before submitting. Missing is tolerated
        // (the field is advisory and the provider may omit it), but a present value above the
        // configured threshold stops the task before anything is signed.
        if let Some(impact) = row.pointer("/routerResult/priceImpactPercent") {
            let impact = impact.as_str().ok_or("malformed price impact")?;
            // OKX documents a *negative* impact as favourable (received value exceeds paid value) and
            // really returns it: a 1 U → BNB quote came back as "-0.08". Only a positive magnitude can
            // exceed the threshold, so the sign is stripped before the comparison.
            let magnitude = impact.trim().trim_start_matches('-');
            let favorable = impact.trim().starts_with('-');
            if !favorable
                && decimal_greater(magnitude, &c.max_price_impact_percent)
                    .ok_or("malformed price impact")?
            {
                return Err(format!(
                    "price impact {impact}% exceeds configured {}%",
                    c.max_price_impact_percent
                ));
            }
        }
        // `approveTransaction` / `approveAddress` / `approveContract` are not in the documented V6
        // swap schema: validating fields the provider is not specified to send only produced
        // failures. What the docs do describe is `signatureData`, checked above: a non-empty value
        // means this swap expects an extra signature (an approval), so the task stops instead.
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
        ] {
            check_echo(container, key, expected, address)?;
        }
        if let Some(router) = row.get("routerResult") {
            if !router.is_object() {
                return Err("malformed router result".into());
            }
            check_echo(router, "chainIndex", &c.chain_index, false)?;
            check_echo(router, "fromTokenAmount", &c.amount, false)?;
            // Token echoes are documented as quote-path metadata and duplicate the `tx`-level
            // checks; only the amounts and chain are bound here.
        }
        let from = field(tx, "from")?;
        let to = field(tx, "to")?;
        if !eq_addr(from, &c.user_wallet_address)
            || !c.allowed_router_addresses.iter().any(|a| eq_addr(a, to))
        {
            return Err("OKX transaction wallet/router mismatch".into());
        }
        let value = field(tx, "value")?;
        if uint256_canonical(value)? > uint256_canonical(&c.max_value_wei)? {
            return Err("OKX tx.value exceeds wei cap".into());
        }
        let gas_number = uint256_canonical(field(tx, "gas")?)?;
        let gas = gas_number
            .to_str_radix(10)
            .parse::<u64>()
            .map_err(|_| "gas unit overflow")?;
        let ceiling = gas_ceiling(c);
        if gas == 0 || gas > ceiling {
            return Err(format!(
                "OKX gas exceeds gas-unit cap (quoted {gas}, cap {ceiling})"
            ));
        }
        let gas_cap = buffered_gas_limit(gas, c.gas_buffer_percent, c.gas_buffer_absolute, ceiling);
        if let Some(n) = tx.get("gasLimit") {
            if uint256_canonical(n.as_str().ok_or("invalid gasLimit units")?)? != gas_number {
                return Err("conflicting gas/gasLimit units".into());
            }
        }
        // `gasPrice` / `maxFeePerGas` / `maxPriorityFeePerGas` are documented as advisory values and
        // are never forwarded (the backend prices the transaction), so they are not validated here:
        // rejecting a provider that fills them differently only ever killed valid swaps.
        // Provider calldata is always opaque: no ABI/selector/path/recipient/deadline decoding.
        // Retain envelope checks and require the signing runtime's independent simulation policy.
        // Even the previous V2 selectors follow this same path; there is no decoder fallback.
        let input = field(tx, "data")?;
        if input.len() < 4
            || input.len() > 262146
            || !input.starts_with("0x")
            || input.len() % 2 != 0
            || !input[2..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("malformed calldata".into());
        }
        let minimum = uint256_canonical(field(tx, "minReceiveAmount")?)?;
        if minimum.bits() == 0 && !c.allow_zero_min_out {
            return Err("zero minimum output forbidden".into());
        }
        // `minReceiveAmount` is documented as the minimum received at the slippage limit: it is the
        // provider's own number, and recomputing a floor from the quote here is second-guessing the
        // provider's slippage math with our arithmetic. Only "missing/zero" is checked above.
        Ok(
            serde_json::json!({"binance_chain_id":c.binance_chain_id,"from":from,"to":to,"value":value,
            "input_data":input,"gas_limit":gas_cap,"route_validation":{"recipient_verified":false,
            "calldata_min_out_verified":false,"requires_runtime_preview":true,"minimum_output_semantics":"unknown",
            "calldata_policy":CALLDATA_POLICY,"gas_quoted":gas,"gas_preview_cap":gas_cap,
            "reported_minimum_output":minimum.to_str_radix(10),"limitations":[
            "provider calldata is opaque: encoded recipient, token path, input amount, deadline and on-chain minimum are NOT locally decoded or verified",
            "reported minimum is response metadata only; successful simulation is not a future execution guarantee",
            "engine MUST perform runtime policy preview checking wallet debit/net credit, pinned router and bounded spender allowance before submission"]}}),
        )
    }
    /// Syntactic/local configuration only. No request or quote is made here.
    fn configuration(config: &Value) -> Result<Config, String> {
        let c: Config = serde_json::from_value(config.clone())
            .map_err(|_| "invalid route config (strict fields/types)")?;
        if c.chain_index != c.binance_chain_id
            || uint256_canonical(&c.chain_index)?.bits() == 0
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
        if uint256_canonical(&c.amount)?.bits() == 0 {
            return Err("amount must be positive".into());
        }
        uint256_canonical(&c.max_value_wei)?;
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
        if c.max_gas_limit > GAS_CEILING {
            return Err("gas ceiling above the per-transaction limit".into());
        }
        if !(100..=1000).contains(&c.gas_buffer_percent)
            || c.gas_buffer_absolute > 5_000_000
            || scaled_decimal(&c.max_price_impact_percent).is_none()
            || decimal_greater(&c.max_price_impact_percent, "100").unwrap_or(true)
        {
            return Err("invalid route bounds".into());
        }
        Ok(c)
    }
}

#[cfg(test)]
mod tests;
