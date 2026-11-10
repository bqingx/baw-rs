//! Preview-bound external signing; never signs locally.
use clap::{Args, Subcommand};
use serde_json::{json, Value};
#[derive(Args, Debug, Clone)]
pub struct ContractPreview {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long = "inputData")]
    pub input_data: Option<String>,
    #[arg(long = "unsignedTx")]
    pub unsigned_tx: Option<String>,
    #[arg(long = "gasLimit")]
    pub gas_limit: Option<u64>,
}
fn nonempty(s: &str) -> Result<(), String> {
    if s.is_empty() || s.trim() != s || s.chars().any(char::is_control) {
        Err("invalid empty or padded identifier".into())
    } else {
        Ok(())
    }
}
pub(crate) fn is_solana(chain: &str) -> bool {
    chain == "CT_501"
}
pub fn address(chain: &str, s: &str) -> Result<(), String> {
    if is_solana(chain) {
        if (32..=44).contains(&s.len())
            && s.bytes()
                .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
        {
            return Ok(());
        }
    } else if let Some(hex) = s.strip_prefix("0x") {
        if hex.len() == 40
            && hex.bytes().all(|b| b.is_ascii_hexdigit())
            && (hex == hex.to_ascii_lowercase() || crate::core::client::checksum_address(s) == s)
        {
            return Ok(());
        }
    }
    Err("invalid chain address or EIP55 checksum".into())
}
fn same_address(chain: &str, a: &str, b: &str) -> bool {
    if is_solana(chain) {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}
fn unsigned_value(s: &str) -> bool {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    match hex {
        Some(h) => !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()),
        None => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
    }
}
pub(crate) fn contract_payload(a: &ContractPreview) -> Result<Value, String> {
    nonempty(&a.binance_chain_id)?;
    address(&a.binance_chain_id, &a.from)?;
    let mut p = json!({"binanceChainId":a.binance_chain_id,"from":a.from});
    if is_solana(&a.binance_chain_id) {
        use base64::Engine;
        if a.to.is_some() || a.value.is_some() || a.input_data.is_some() || a.gas_limit.is_some() {
            return Err("EVM fields are not supported on Solana".into());
        }
        let tx = a
            .unsigned_tx
            .as_deref()
            .ok_or("Solana requires unsignedTx")?;
        if base64::engine::general_purpose::STANDARD
            .decode(tx)
            .map_err(|_| "invalid unsignedTx base64")?
            .is_empty()
        {
            return Err("empty unsignedTx".into());
        }
        p["unsignedTx"] = json!(tx);
    } else {
        if a.unsigned_tx.is_some() {
            return Err("unsignedTx is Solana only".into());
        }
        let to = a.to.as_deref().ok_or("EVM requires to")?;
        address(&a.binance_chain_id, to)?;
        p["to"] = json!(to);
        if let Some(v) = &a.value {
            if !unsigned_value(v) {
                return Err("value must be unsigned decimal or nonempty hex wei".into());
            }
            p["value"] = json!(v);
        }
        if let Some(v) = &a.input_data {
            let h = v
                .strip_prefix("0x")
                .ok_or("inputData must be 0x-prefixed even hex")?;
            if h.len() % 2 != 0 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("inputData must be even hex".into());
            }
            p["inputData"] = json!(v);
        }
        if let Some(v) = a.gas_limit {
            if !(21000..=15000000).contains(&v) {
                return Err("gasLimit must be 21000..15000000".into());
            }
            p["gasLimit"] = json!(v);
        }
    }
    Ok(p)
}
#[derive(Subcommand, Debug)]
pub enum ContractCommand {
    Preview(ContractPreview),
    Execute {
        #[arg(long = "requestId")]
        request_id: String,
    },
    /// Preview then execute in one process with no policy gate and no human digest
    /// review. Consent is still `--confirm` + a unique `--operation-id`; the preview's
    /// risks and authority changes are returned in the output for the caller to log.
    Fire(ContractFire),
}
#[derive(Args, Debug, Clone)]
pub struct ContractFire {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long = "inputData")]
    pub input_data: Option<String>,
    #[arg(long = "gasLimit")]
    pub gas_limit: Option<u64>,
    /// Optional policy file. Without it `fire` runs with no policy gate; with it every
    /// pinned field (target, value, selector, calldata digest, gas cap) must match.
    #[arg(long)]
    pub policy: Option<std::path::PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct MessagePreview {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub message: String,
    #[arg(long = "signType")]
    pub sign_type: String,
}
#[derive(Args, Debug, Clone)]
pub struct MessageHistory {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
    #[arg(long = "nextToken")]
    pub next_token: Option<String>,
    #[arg(long = "startTime")]
    pub start_time: Option<u64>,
    #[arg(long = "endTime")]
    pub end_time: Option<u64>,
    #[arg(long = "sortType")]
    pub sort_type: Option<String>,
}
#[derive(Subcommand, Debug)]
pub enum SignCommand {
    Preview(MessagePreview),
    Execute {
        #[arg(long = "requestId")]
        request_id: String,
    },
    Result {
        #[arg(long = "order-id")]
        order_id: String,
    },
    History(MessageHistory),
}
use crate::core::client::{Client, API};
use reqwest::Method;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_CACHE: u64 = 4 * 1024 * 1024;
const PREVIEW_TTL: u64 = 300;
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(|_| "invalid local clock".into())
}
fn sha(bytes: &[u8]) -> String {
    crate::util::validate::sha256_hex(bytes)
}
fn value_sha(v: &Value) -> Result<String, String> {
    Ok(sha(
        &serde_json::to_vec(v).map_err(|_| "cannot digest preview")?
    ))
}
fn timestamp(v: &Value) -> Option<u64> {
    let n = v.as_u64().or_else(|| v.as_str()?.parse().ok())?;
    Some(if n >= 1_000_000_000_000 { n / 1000 } else { n })
}
static SKIP_PRECHECKS_DEFAULT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// A resident server started with `--skip-prechecks` (or the env var) applies it to every
/// request it serves; a request may still turn it on itself, never off.
pub fn set_skip_prechecks_default(on: bool) {
    SKIP_PRECHECKS_DEFAULT.store(on, std::sync::atomic::Ordering::Relaxed);
}

tokio::task_local! {
    static REQUEST_SKIP_PRECHECKS: bool;
}

/// Pin the ordinary CLI's policy for the entire asynchronous dispatch.
pub(crate) async fn request_precheck_policy<F: std::future::Future>(
    requested: bool,
    future: F,
) -> F::Output {
    let skip = requested || SKIP_PRECHECKS_DEFAULT.load(std::sync::atomic::Ordering::Relaxed);
    REQUEST_SKIP_PRECHECKS.scope(skip, future).await
}

pub fn skip_prechecks() -> bool {
    REQUEST_SKIP_PRECHECKS
        .try_with(|skip| *skip)
        .unwrap_or_else(|_| SKIP_PRECHECKS_DEFAULT.load(std::sync::atomic::Ordering::Relaxed))
}

async fn check_wallet(
    client: &mut Client,
    chain: &str,
    expected: Option<&str>,
) -> Result<(String, u64), String> {
    check_wallet_policy(client, chain, expected, skip_prechecks()).await
}
pub(crate) async fn check_wallet_policy(
    client: &mut Client,
    chain: &str,
    expected: Option<&str>,
    skip: bool,
) -> Result<(String, u64), String> {
    nonempty(chain)?;
    if let Some(policy) = crate::exec::execution::prepared_wallet() {
        crate::exec::execution::check_prepared_task()?;
        return policy.check(client, chain, expected);
    }
    if skip {
        // No settings read (the backend enforces developer mode anyway) and no address
        // read (the caller asserts the sender). Format is still validated locally, and the
        // preview is bounded by the ordinary five-minute window instead of the dev expiry.
        let sender = expected
            .ok_or("--from is required when pre-checks are skipped")?
            .to_owned();
        address(chain, &sender)?;
        return Ok((sender, now()? + 300));
    }
    // Revalidate against this client; the value is reused only for a few seconds inside
    // this process (BAW_RS_CHECK_TTL_MS, default 3000) and dropped on any session change.
    let settings = client.cached_settings().await?;
    let mode = &settings["devMode"];
    let expires =
        timestamp(&mode["expiresAt"]).ok_or("developer mode expiry is missing or invalid")?;
    if mode["enabled"] != true || expires <= now()? {
        return Err("developer mode is disabled or expired".into());
    }
    let wallets = client.cached_addresses().await?;
    let rows = wallets["addresses"]
        .as_array()
        .ok_or("wallet addresses missing")?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r["binanceChainId"] == chain)
        .collect();
    if matches.len() != 1 {
        return Err("wallet must have one unambiguous sender on requested chain".into());
    }
    let actual = matches[0]["address"]
        .as_str()
        .ok_or("wallet sender missing")?
        .to_owned();
    address(chain, &actual)?;
    if expected.is_some_and(|e| !same_address(chain, e, &actual)) {
        return Err("sender does not match actual wallet on requested chain".into());
    }
    Ok((actual, expires))
}
mod cache;
use cache::{Cache, Cached};
fn failed_simulation(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().any(failed_simulation),
        Value::Object(o) => o.iter().any(|(k, v)| {
            (matches!(
                k.as_str(),
                "success" | "simulationSuccess" | "simulationPassed"
            ) && v == false)
                || (matches!(k.as_str(), "status" | "simulationStatus")
                    && v.as_str().is_some_and(|s| {
                        matches!(
                            s.to_ascii_uppercase().as_str(),
                            "FAILED" | "FAIL" | "ERROR" | "REVERTED" | "SIMULATION_FAILED"
                        )
                    }))
                || (matches!(k.as_str(), "simulationError" | "error")
                    && !v.is_null()
                    && v != false
                    && v != "")
                || (k == "code" && !v.is_null() && v != "000000" && v != "0" && v != 0)
                || (matches!(
                    k.as_str(),
                    "simulation" | "simulationResult" | "simulationDetails" | "simulationResults"
                ) && failed_simulation(v))
        }),
        _ => false,
    }
}
fn check_output(output: &Value, chain: &str, sender: &str) -> Result<(), String> {
    if !output.is_object() || output.get("previewSha256").is_some() {
        return Err("invalid preview response".into());
    }
    if failed_simulation(output) {
        return Err("preview contains failed simulation or backend failure".into());
    }
    if output.get("binanceChainId").is_some_and(|v| v != chain) {
        return Err("preview chain mismatch".into());
    }
    for key in ["from", "signer"] {
        if let Some(v) = output.get(key) {
            if !v.as_str().is_some_and(|v| same_address(chain, v, sender)) {
                return Err("preview sender mismatch".into());
            }
        }
    }
    Ok(())
}
async fn preview_call(
    client: &mut Client,
    dir: &Path,
    kind: &str,
    chain: &str,
    sender: String,
    dev_expiry: u64,
    input: Value,
) -> Result<Value, String> {
    let cache = Cache::open(dir)?;
    let record = preview_record(
        client,
        &cache,
        kind,
        chain,
        sender,
        dev_expiry,
        input,
        &mut FireTimings::default(),
    )
    .await?;
    let digest = record.digest()?;
    cache.store(&record)?;
    let mut output = record.output;
    output["previewSha256"] = json!(digest);
    Ok(output)
}
#[derive(Default, serde::Serialize)]
pub(crate) struct FireTimings {
    #[serde(skip)]
    pub preview_output: Option<Value>,
    pub preview: crate::core::client::HttpTiming,
    pub execute: crate::core::client::HttpTiming,
    pub validation_ms: f64,
    pub local_prep_ms: f64,
    pub prechecks_ms: f64,
    pub consume_ms: f64,
}
#[allow(clippy::too_many_arguments)]
async fn preview_record(
    client: &mut Client,
    cache: &Cache,
    kind: &str,
    chain: &str,
    sender: String,
    dev_expiry: u64,
    input: Value,
    timing: &mut FireTimings,
) -> Result<Cached, String> {
    let output = client
        .request(
            Method::POST,
            &format!("{API}/{kind}/preview"),
            Some(input.clone()),
        )
        .await;
    timing.preview = client.last_http.clone();
    let output = output?;
    timing.preview_output = Some(output.clone());
    let validation_start = std::time::Instant::now();
    let result = (|| {
        check_output(&output, chain, &sender)?;
        let request_id = output["requestId"]
            .as_str()
            .ok_or("preview requestId missing")?
            .to_owned();
        nonempty(&request_id)?;
        // The installed protocol does not require an expiry field. Always impose our
        // own five-minute limit, tightened by every server bound that is present.
        let time = now()?;
        let mut expires_at = dev_expiry.min(time.saturating_add(PREVIEW_TTL));
        for key in [
            "expiresAt",
            "expireAt",
            "expireTime",
            "requestExpireTime",
            "expiryTime",
        ] {
            if let Some(value) = output.get(key) {
                expires_at = expires_at.min(timestamp(value).ok_or("invalid preview expiry")?);
            }
        }
        if expires_at <= time {
            return Err("preview expired".into());
        }
        let record = Cached {
            version: 1,
            kind: kind.into(),
            client_binding: sha(client.credentials().client_id.as_bytes()),
            wallet_device: cache.wallet_device,
            wallet_inode: cache.wallet_inode,
            chain: chain.into(),
            sender,
            request_id,
            expires_at,
            input,
            output,
        };
        Ok(record)
    })();
    timing.validation_ms += validation_start.elapsed().as_secs_f64() * 1000.0;
    result
}
async fn execute_call(
    client: &mut Client,
    dir: &Path,
    kind: &str,
    id: &str,
    accepted: Option<&str>,
) -> Result<Value, String> {
    nonempty(id)?;
    let accepted=accepted.ok_or("explicit --accept-preview-sha256 review is required, including all risks and authorityChanges")?;
    if accepted.len() != 64
        || !accepted
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid accepted preview digest".into());
    }
    let cache = Cache::open(dir)?;
    cache.check_unconsumed(id)?;
    let record = cache.read(id)?;
    if record.version != 1
        || record.kind != kind
        || record.request_id != id
        || record.client_binding != sha(client.credentials().client_id.as_bytes())
        || record.wallet_device != cache.wallet_device
        || record.wallet_inode != cache.wallet_inode
    {
        return Err("preview cache wallet, operation or request binding mismatch".into());
    }
    if record.digest()? != accepted {
        return Err("preview digest mismatch; review the exact risks and authorityChanges".into());
    }
    if record.expires_at <= now()? {
        return Err("preview expired".into());
    }
    // Consume before any network revalidation: an accepted attempt is one-shot even on error.
    cache.consume(id, accepted)?;
    check_output(&record.output, &record.chain, &record.sender)?;
    let (actual, _) = check_wallet(client, &record.chain, Some(&record.sender)).await?;
    if kind == "sign-message" {
        validate_message(&record.input, &actual)?;
    }
    if record.expires_at <= now()? {
        return Err("preview expired during wallet revalidation; preview remains consumed".into());
    }
    // The durable preview marker and operation journal prohibit retries of this POST.
    let output = client
        .request(
            Method::POST,
            &format!("{API}/{kind}/execute"),
            Some(json!({"requestId":id})),
        )
        .await?;
    if failed_simulation(&output) {
        return Err("execute backend failure; preview remains consumed, do not retry".into());
    }
    Ok(output)
}
pub async fn contract(
    client: &mut Client,
    dir: &Path,
    command: &ContractCommand,
    accepted_digest: Option<&str>,
) -> Result<Value, String> {
    match command {
        ContractCommand::Preview(a) => {
            let input = contract_payload(a)?;
            let (sender, expires) =
                check_wallet(client, &a.binance_chain_id, Some(&a.from)).await?;
            preview_call(
                client,
                dir,
                "contract-call",
                &a.binance_chain_id,
                sender,
                expires,
                input,
            )
            .await
        }
        ContractCommand::Execute { request_id } => {
            execute_call(client, dir, "contract-call", request_id, accepted_digest).await
        }
        ContractCommand::Fire(a) => fire_contract(client, dir, a).await,
    }
}
fn chain_number(v: &Value) -> Option<u128> {
    if let Some(n) = v.as_u64() {
        return Some(n as u128);
    }
    let s = v.as_str()?;
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if h.is_empty() {
            return None;
        }
        u128::from_str_radix(h, 16).ok()
    } else if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}
fn validate_message(input: &Value, sender: &str) -> Result<(), String> {
    let chain = input["binanceChainId"]
        .as_str()
        .ok_or("message chain missing")?;
    nonempty(chain)?;
    let kind = input["signType"].as_str().ok_or("signType missing")?;
    nonempty(kind)?;
    let message = input["message"]
        .as_str()
        .ok_or("message must remain a string")?;
    if message.is_empty() {
        return Err("message is empty".into());
    }
    if kind.eq_ignore_ascii_case("EIP712") {
        if kind != "EIP712" || is_solana(chain) {
            return Err("EIP712 requires EVM and exact signType EIP712".into());
        }
        let rpc: Value = serde_json::from_str(message)
            .map_err(|_| "EIP712 requires a JSON-RPC wrapper string")?;
        if rpc["jsonrpc"] != "2.0" || rpc["method"] != "eth_signTypedData_v4" {
            return Err("EIP712 requires eth_signTypedData_v4 JSON-RPC 2.0".into());
        }
        let params = rpc["params"]
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or("typed-data wrapper requires signer and typed data")?;
        let signer = params[0].as_str().ok_or("typed-data signer missing")?;
        address(chain, signer)?;
        if !same_address(chain, signer, sender) {
            return Err("typed-data signer does not match wallet".into());
        }
        let typed = if let Some(s) = params[1].as_str() {
            serde_json::from_str::<Value>(s).map_err(|_| "invalid wrapped typed data")?
        } else {
            params[1].clone()
        };
        let domain = chain_number(&typed["domain"]["chainId"])
            .ok_or("typed-data domain chainId missing or invalid")?;
        if Some(domain) != chain_number(&json!(chain)) {
            return Err("typed-data domain chain mismatch".into());
        }
        if !typed["types"].is_object()
            || !typed["message"].is_object()
            || typed["primaryType"].as_str().is_none_or(str::is_empty)
        {
            return Err("typed-data structure missing".into());
        }
    }
    Ok(())
}
pub async fn message(
    client: &mut Client,
    dir: &Path,
    command: &SignCommand,
    accepted_digest: Option<&str>,
) -> Result<Value, String> {
    match command {
        SignCommand::Preview(a) => {
            nonempty(&a.binance_chain_id)?;
            nonempty(&a.sign_type)?;
            let input = json!({"binanceChainId":a.binance_chain_id,"message":a.message,"signType":a.sign_type});
            let (sender, expiry) = check_wallet(client, &a.binance_chain_id, None).await?;
            validate_message(&input, &sender)?;
            preview_call(
                client,
                dir,
                "sign-message",
                &a.binance_chain_id,
                sender,
                expiry,
                input,
            )
            .await
        }
        SignCommand::Execute { request_id } => {
            execute_call(client, dir, "sign-message", request_id, accepted_digest).await
        }
        SignCommand::Result { order_id } => {
            nonempty(order_id)?;
            client
                .request(
                    Method::POST,
                    &format!("{API}/sign-message/result"),
                    Some(json!({"orderId":order_id})),
                )
                .await
        }
        SignCommand::History(a) => {
            if a.limit == 0 || a.limit > 100 {
                return Err("limit must be 1..100".into());
            }
            if a.start_time.zip(a.end_time).is_some_and(|(s, e)| s > e) {
                return Err("startTime exceeds endTime".into());
            }
            if a.sort_type
                .as_deref()
                .is_some_and(|s| !matches!(s, "ASC" | "DESC"))
            {
                return Err("sortType must be ASC or DESC".into());
            }
            let query = {
                let mut q = url::form_urlencoded::Serializer::new(String::new());
                if let Some(chain) = &a.binance_chain_id {
                    nonempty(chain)?;
                    q.append_pair("binanceChainIds", chain);
                }
                q.append_pair("limit", &a.limit.to_string());
                if let Some(token) = &a.next_token {
                    nonempty(token)?;
                    q.append_pair("nextToken", token);
                }
                if let Some(t) = a.start_time {
                    q.append_pair("startTime", &t.to_string());
                }
                if let Some(t) = a.end_time {
                    q.append_pair("endTime", &t.to_string());
                }
                if let Some(sort) = &a.sort_type {
                    q.append_pair("sortType", sort);
                }
                q.finish()
            };
            client
                .request(
                    Method::GET,
                    &format!("{API}/sign-message/history?{query}"),
                    None,
                )
                .await
        }
    }
}
pub fn needs_accepted_digest(command: &ContractCommand) -> bool {
    matches!(command, ContractCommand::Execute { .. })
}
pub fn is_contract_mutating(command: &ContractCommand) -> bool {
    matches!(
        command,
        ContractCommand::Execute { .. } | ContractCommand::Fire(_)
    )
}
pub fn is_message_mutating(command: &SignCommand) -> bool {
    matches!(command, SignCommand::Execute { .. })
}
pub fn contract_intent(command: &ContractCommand) -> Value {
    match command {
        ContractCommand::Execute { request_id } => {
            json!({"operation":"contract-call","requestId":request_id})
        }
        ContractCommand::Preview(a) => {
            json!({"operation":"contract-call-preview","binanceChainId":a.binance_chain_id,"from":a.from,"to":a.to,"value":a.value,"inputData":a.input_data,"unsignedTx":a.unsigned_tx,"gasLimit":a.gas_limit})
        }
        ContractCommand::Fire(a) => {
            json!({"operation":"contract-call-fire","binanceChainId":a.binance_chain_id,"from":a.from,"to":a.to,"value":a.value,"inputData":a.input_data,"gasLimit":a.gas_limit,"policy":a.policy.as_ref().map(|p| p.to_string_lossy().into_owned())})
        }
    }
}
pub fn message_intent(command: &SignCommand) -> Value {
    match command {
        SignCommand::Execute { request_id } => {
            json!({"operation":"sign-message","requestId":request_id})
        }
        SignCommand::Preview(a) => {
            json!({"operation":"sign-message-preview","binanceChainId":a.binance_chain_id,"message":a.message,"signType":a.sign_type})
        }
        SignCommand::Result { order_id } => {
            json!({"operation":"sign-message-result","orderId":order_id})
        }
        SignCommand::History(_) => json!({"operation":"sign-message-history"}),
    }
}
/// Unattended execution: preview then execute in one process, gated by a pinned
/// policy file instead of a human digest review. Everything the policy pins is
/// checked before the preview request is sent, and the preview is checked again
/// before execution. No retries: an ambiguous execute is reported, never repeated.
async fn run_contract(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
    policy_path: &Path,
) -> Result<Value, String> {
    use crate::exec::policy::{Intent, Policy};
    if is_solana(&args.binance_chain_id) {
        return Err("policy run supports EVM chains only".into());
    }
    let policy = Policy::load(policy_path)?;
    let value = args.value.as_deref().unwrap_or("0");
    let intent = Intent {
        chain_id: &args.binance_chain_id,
        to: &args.to,
        value,
        input_data: args.input_data.as_deref(),
        gas_limit: args.gas_limit,
    };
    policy.check_intent(&intent)?;
    let preview_args = ContractPreview {
        binance_chain_id: args.binance_chain_id.clone(),
        from: args.from.clone(),
        to: Some(args.to.clone()),
        value: args.value.clone(),
        input_data: args.input_data.clone(),
        unsigned_tx: None,
        gas_limit: args.gas_limit,
    };
    let payload = contract_payload(&preview_args)?;
    let (sender, dev_expiry) =
        check_wallet(client, &args.binance_chain_id, Some(&args.from)).await?;
    let preview = preview_call(
        client,
        dir,
        "contract-call",
        &args.binance_chain_id,
        sender,
        dev_expiry,
        payload,
    )
    .await?;
    policy.check_preview(&preview)?;
    let request_id = preview["requestId"]
        .as_str()
        .ok_or("preview requestId missing")?
        .to_owned();
    let digest = preview["previewSha256"]
        .as_str()
        .ok_or("preview digest missing")?
        .to_owned();
    let execution = execute_call(client, dir, "contract-call", &request_id, Some(&digest)).await?;
    Ok(json!({
        "policy": "matched",
        "preview": {
            "requestId": request_id,
            "previewSha256": digest,
            "requireConfirmation": preview.get("requireConfirmation").cloned().unwrap_or(Value::Null),
            "gasLimit": preview.get("gasLimit").cloned().unwrap_or(Value::Null),
        },
        "execution": execution,
    }))
}
mod fire;
use fire::fire_contract;
#[cfg(test)]
pub(crate) use fire::memory_fire;

#[cfg(test)]
mod tests;
