//! Opt-in exact-amount approval, under the parent market-order intent and write lock.
use super::*;
use crate::execution::OperationGuard;

pub(super) enum Outcome {
    Continue(Value),
    Stop(Value),
}

pub(super) fn validate_rpc_url(raw: &str) -> Result<(), String> {
    let url = url::Url::parse(raw).map_err(|_| "invalid private OKX RPC URL")?;
    let local_test = cfg!(test)
        && url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if (url.scheme() != "https" && !local_test)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("OKX approval RPC requires HTTPS without userinfo/fragment".into());
    }
    Ok(())
}
fn hex(raw: &str, bytes: usize) -> bool {
    raw.len() == bytes * 2 + 2
        && raw.starts_with("0x")
        && raw[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn quantity(v: &Value) -> Result<u64, String> {
    let s = v
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .filter(|s| !s.is_empty() && s.len() <= 16)
        .ok_or("invalid approval RPC quantity")?;
    u64::from_str_radix(s, 16).map_err(|_| "invalid approval RPC quantity".into())
}
impl Okx {
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        if !matches!(
            method,
            "eth_chainId"
                | "eth_call"
                | "eth_getTransactionReceipt"
                | "eth_blockNumber"
                | "eth_getBlockByNumber"
        ) {
            return Err("unsupported read-only approval RPC method".into());
        }
        let url = self
            .profile
            .rpc_url
            .as_deref()
            .ok_or("ERC-20 --auto-approve requires rpc_url in private OKX profile")?;
        validate_rpc_url(url)?;
        let mut r = self
            .http
            .post(url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| "approval RPC request failed (details redacted)")?;
        if !r.status().is_success() {
            return Err("approval RPC HTTP failure (details redacted)".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = r.chunk().await.map_err(|_| "approval RPC read failed")? {
            if bytes.len().saturating_add(chunk.len()) > 1_048_576 {
                return Err("approval RPC response too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid approval RPC JSON")?;
        if v["jsonrpc"] != "2.0"
            || v["id"] != 1
            || v.get("error").is_some()
            || v.get("result").is_none()
        {
            return Err("approval RPC error (details redacted)".into());
        }
        Ok(v["result"].clone())
    }
    async fn check_rpc_chain(&self, chain: &str) -> Result<(), String> {
        let actual = quantity(&self.rpc("eth_chainId", json!([])).await?)?;
        if actual.to_string() != chain {
            return Err("approval RPC chain mismatch".into());
        }
        Ok(())
    }
    async fn allowance(&self, token: &str, owner: &str, spender: &str) -> Result<BigUint, String> {
        let data = format!(
            "0xdd62ed3e{:0>64}{:0>64}",
            owner[2..].to_ascii_lowercase(),
            spender[2..].to_ascii_lowercase()
        );
        let v = self
            .rpc("eth_call", json!([{"to":token,"data":data},"latest"]))
            .await?;
        let raw = v
            .as_str()
            .filter(|s| hex(s, 32))
            .ok_or("allowance must be exactly one ABI uint256 word")?;
        crate::uint::uint256(raw)
    }
    async fn wait_approval(&self, hash: &str, owner: &str, token: &str) -> Result<bool, String> {
        tokio::time::timeout(
            Duration::from_millis(self.profile.approval_wait_ms),
            async {
                loop {
                    let r = self.rpc("eth_getTransactionReceipt", json!([hash])).await?;
                    if !r.is_null() {
                        if r["transactionHash"]
                            .as_str()
                            .is_none_or(|s| !s.eq_ignore_ascii_case(hash))
                            || r["from"]
                                .as_str()
                                .is_none_or(|s| !s.eq_ignore_ascii_case(owner))
                            || r["to"]
                                .as_str()
                                .is_none_or(|s| !s.eq_ignore_ascii_case(token))
                        {
                            return Err("approval receipt transaction binding mismatch".into());
                        }
                        let success = match r["status"].as_str() {
                            Some("0x1") => true,
                            Some("0x0") => false,
                            _ => return Err("invalid approval receipt status".into()),
                        };
                        let number = quantity(&r["blockNumber"])?;
                        let block_hash = r["blockHash"]
                            .as_str()
                            .filter(|s| hex(s, 32))
                            .ok_or("invalid approval block hash")?;
                        let head = quantity(&self.rpc("eth_blockNumber", json!([])).await?)?;
                        if head >= number
                            && head - number >= self.profile.approval_confirmations - 1
                        {
                            let block = self
                                .rpc("eth_getBlockByNumber", json!([r["blockNumber"], false]))
                                .await?;
                            if quantity(&block["number"])? != number
                                || block["hash"]
                                    .as_str()
                                    .is_none_or(|s| !s.eq_ignore_ascii_case(block_hash))
                            {
                                return Err("approval receipt block is no longer canonical".into());
                            }
                            return Ok(success);
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            },
        )
        .await
        .map_err(|_| "approval confirmation timeout; submission may still be live".to_string())?
    }
}

fn same_alias(row: &Value, keys: &[&str], expected: &str) -> bool {
    let mut seen = false;
    for key in keys {
        if let Some(v) = row.get(*key) {
            seen = true;
            if v.as_str().is_none_or(|s| !s.eq_ignore_ascii_case(expected)) {
                return false;
            }
        }
    }
    seen
}
fn amount_value(value: &Value) -> Result<BigUint, String> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.is_number().then(|| value.to_string()))
        .ok_or("allowance amount must be an exact uint256 string or integer")?;
    uint(&raw)
}
fn aliased_amount(row: &Value, keys: &[&str]) -> Result<BigUint, String> {
    let mut found = None;
    for key in keys {
        if let Some(value) = row.get(*key) {
            let amount = amount_value(value)?;
            if found.as_ref().is_some_and(|n| *n != amount) {
                return Err("conflicting allowance amount aliases".into());
            }
            found = Some(amount);
        }
    }
    found.ok_or_else(|| "allowance amount missing".into())
}
fn erc20_metadata(row: &Value) -> Result<(), String> {
    if row
        .get("tokenType")
        .is_some_and(|v| v.as_str() != Some("Erc20"))
        || row.get("tokenId").is_some_and(|v| !v.is_null())
    {
        return Err("allowance metadata is not a fungible ERC-20 token".into());
    }
    Ok(())
}

/// An exact locally encoded approve may have no allowance metadata, or one precisely bound grant.
/// Unknown schemas are refused instead of broadly allowing authority changes.
pub(super) fn validate_preview(
    v: &Value,
    owner: &str,
    token: &str,
    spender: &str,
    amount: &BigUint,
) -> Result<(), String> {
    if v["parsedTx"]["contractAddress"]
        .as_str()
        .is_none_or(|s| !s.eq_ignore_ascii_case(token))
    {
        return Err("approval preview target mismatch".into());
    }
    let sim = &v["simulationResult"];
    if sim["simulationCode"] != "000000000" {
        return Err("approval requires successful Binance simulation".into());
    }
    empty_array(sim, "authorityChanges")?;
    empty_array(sim, "balanceChanges")?;
    let grants = sim["allowanceChanges"]
        .as_array()
        .filter(|rows| rows.len() <= 1)
        .ok_or("invalid approval allowance changes")?;
    for row in grants {
        if !same_alias(row, &["owner", "ownerAddress"], owner)
            || !same_alias(row, &["contractAddress", "tokenAddress"], token)
            || !same_alias(row, &["spender", "spenderAddress"], spender)
        {
            return Err("approval preview owner/token/spender mismatch".into());
        }
        erc20_metadata(row)?;
        if row.get("allowanceBefore").is_some() || row.get("preAmount").is_some() {
            aliased_amount(row, &["allowanceBefore", "preAmount"])?;
        }
        let mut seen = false;
        let fields = row.as_object().ok_or("invalid approval grant")?;
        for (key, value) in fields {
            match key.as_str() {
                "amount" | "allowanceAfter" | "newAllowance" | "postAmount" => {
                    seen = true;
                    if amount_value(value)? != *amount {
                        return Err(
                            "approval preview exceeds or differs from exact requested amount"
                                .into(),
                        );
                    }
                }
                "allowanceBefore" | "preAmount" => {
                    amount_value(value)?;
                }
                "owner" | "ownerAddress" | "contractAddress" | "tokenAddress" | "spender"
                | "spenderAddress" | "tokenType" | "tokenId" => {}
                _ => return Err("unknown approval allowance schema; review manually".into()),
            }
        }
        if !seen {
            return Err("approval preview grant amount missing".into());
        }
    }
    let mut sanitized = v.clone();
    sanitized["simulationResult"]["allowanceChanges"] = json!([]);
    reject_nested_changes(&sanitized, 0)?;
    let risks = &v["risks"];
    empty_array(risks, "riskDetails")?;
    empty_array(risks, "riskBehaviors")?;
    if risks["addresses"].as_object().is_none_or(|a| !a.is_empty()) {
        return Err("approval preview contains or omits address risks".into());
    }
    for object in [v, sim] {
        for key in ["simulation", "simulationResults", "simulationDetails"] {
            if object.get(key).is_some_and(|v| !v.is_null()) {
                return Err("unexpected nested approval simulation".into());
            }
        }
    }
    Ok(())
}
/// Spending an already-approved token may report a decrease; never allow an increase or
/// unrelated spender/asset. Unknown allowance schemas fail closed.
pub(super) fn validate_consumption(
    rows: &Value,
    owner: &str,
    token: &str,
    spender: &str,
    amount: &BigUint,
) -> Result<(), String> {
    let rows = rows
        .as_array()
        .filter(|rows| rows.len() <= 1)
        .ok_or("invalid swap allowance changes")?;
    if is_native(token) && !rows.is_empty() {
        return Err("native input cannot consume ERC-20 allowance".into());
    }
    for row in rows {
        if !same_alias(row, &["owner", "ownerAddress"], owner)
            || !same_alias(row, &["contractAddress", "tokenAddress"], token)
            || !same_alias(row, &["spender", "spenderAddress"], spender)
        {
            return Err("swap allowance consumption owner/token/spender mismatch".into());
        }
        erc20_metadata(row)?;
        let before = aliased_amount(row, &["allowanceBefore", "preAmount"])?;
        let after = aliased_amount(row, &["allowanceAfter", "postAmount"])?;
        if after > before || &before - &after > *amount {
            return Err("swap allowance change exceeds permitted consumption".into());
        }
        for key in row
            .as_object()
            .ok_or("invalid allowance consumption")?
            .keys()
        {
            if !matches!(
                key.as_str(),
                "owner"
                    | "ownerAddress"
                    | "contractAddress"
                    | "tokenAddress"
                    | "spender"
                    | "spenderAddress"
                    | "allowanceBefore"
                    | "allowanceAfter"
                    | "preAmount"
                    | "postAmount"
                    | "tokenType"
                    | "tokenId"
            ) {
                return Err("unknown swap allowance schema; review manually".into());
            }
        }
    }
    Ok(())
}

fn stopped(status: &str, evidence: Value, reason: &str) -> Outcome {
    Outcome::Stop(
        json!({"provider":"okx","status":status,"stage":"approval","swapSubmitted":false,
        "retrySafe":false,"autoApproval":evidence,"message":reason}),
    )
}
pub(super) async fn prepare(
    client: &mut Client,
    dir: &Path,
    a: &SwapArgs,
    api: &Okx,
    sender: &str,
    route: &Value,
    journal: Option<&mut OperationGuard>,
) -> Result<Outcome, String> {
    if !a.auto_approve {
        return Ok(Outcome::Continue(Value::Null));
    }
    if is_native(&a.quote.from_token) {
        return Ok(Outcome::Continue(
            json!({"enabled":true,"needed":false,"reason":"native input needs no ERC-20 allowance"}),
        ));
    }
    let journal =
        journal.ok_or("automatic approval requires the parent durable operation guard")?;
    let q = &a.quote;
    let token = token(&q.from_token);
    let spender = text(route, "expected_spender")?;
    let amount = uint(text(route, "amount")?)?;
    api.check_rpc_chain(&q.binance_chain_id).await?;
    let before = api.allowance(&token, sender, spender).await?;
    let mut evidence = json!({"enabled":true,"needed":before<amount,"token":token,"spender":spender,
        "amount":amount.to_string(),"allowanceBefore":before.to_string(),"unlimited":false});
    if before >= amount {
        return Ok(Outcome::Continue(evidence));
    }
    // Fail on bad tokens, impact, or an already impossible output floor before spending approval gas.
    let mut params = query(q)?;
    params.push((
        "priceImpactProtectionPercent",
        api.profile.max_price_impact_percent.clone(),
    ));
    let raw = api.get("quote", &params).await?;
    let quote = api.validate_quote(q, single(&raw)?)?;
    if uint(text(&quote, "toTokenAmount")?)?
        < uint(a.okx_min_output_units.as_deref().ok_or("minimum missing")?)?
    {
        return Err("pre-approval quote is already below the user's output floor".into());
    }
    let data = crate::approvals::approve_calldata(spender, &amount.to_string(), false)?;
    let args = signing::ContractPreview {
        binance_chain_id: q.binance_chain_id.clone(),
        from: sender.into(),
        to: Some(token.clone()),
        value: Some("0".into()),
        input_data: Some(data),
        unsigned_tx: None,
        gas_limit: Some(api.profile.max_gas_limit.min(300_000)),
    };
    let preview =
        signing::contract(client, dir, &signing::ContractCommand::Preview(args), None).await?;
    validate_preview(&preview, sender, &token, spender, &amount)?;
    let request_id = text(&preview, "requestId")?.to_owned();
    let digest = text(&preview, "previewSha256")?;
    journal.checkpoint("approval_submitting", &json!({}))?;
    let execution = signing::contract(
        client,
        dir,
        &signing::ContractCommand::Execute { request_id },
        Some(digest),
    )
    .await;
    let result = match execution {
        Ok(v) => v,
        Err(_) => {
            journal.checkpoint("approval_uncertain", &json!({}))?;
            return Ok(stopped("APPROVAL_UNCERTAIN",evidence,"approval execute outcome unknown; reconcile before a new intent; no automatic retry"));
        }
    };
    if let Some(id) = result.get("orderId").and_then(Value::as_str) {
        evidence["orderId"] = json!(id);
    }
    let hash = result
        .get("txHash")
        .and_then(Value::as_str)
        .filter(|s| hex(s, 32));
    let recovery = json!({"approvalTxHash":hash,"approvalOrderId":result["orderId"]});
    let hash = match hash {
        Some(hash) => hash,
        None => {
            journal.checkpoint("approval_uncertain", &recovery)?;
            return Ok(stopped("APPROVAL_UNCERTAIN",evidence,"approval may await App confirmation or submission; no usable txHash, swap not attempted"));
        }
    };
    evidence["txHash"] = json!(hash);
    journal.checkpoint("approval_broadcast", &recovery)?;
    match api.wait_approval(hash, sender, &token).await {
        Ok(false) => {
            journal.checkpoint("approval_reverted", &recovery)?;
            return Ok(stopped(
                "APPROVAL_REVERTED",
                evidence,
                "approval reverted on chain; no reset-to-zero, retry, or swap was attempted",
            ));
        }
        Err(_) => {
            journal.checkpoint("approval_uncertain", &recovery)?;
            return Ok(stopped("APPROVAL_UNCERTAIN",evidence,"approval receipt unavailable, timed out, malformed, or non-canonical; reconcile before retrying"));
        }
        Ok(true) => {}
    }
    // The receipt alone is insufficient: verify this RPC is still on the right chain and read state.
    let checked = async {
        api.check_rpc_chain(&q.binance_chain_id).await?;
        api.allowance(&token, sender, spender).await
    }
    .await;
    let after = match checked {
        Ok(n) if n == amount => n,
        _ => {
            journal.checkpoint("approval_uncertain", &recovery)?;
            return Ok(stopped("APPROVAL_STATE_MISMATCH",evidence,"approval receipt succeeded but exact allowance was not verified; swap not attempted"));
        }
    };
    evidence["allowanceAfter"] = json!(after.to_string());
    evidence["confirmed"] = json!(true);
    evidence["confirmations"] = json!(api.profile.approval_confirmations);
    journal.checkpoint("approval_confirmed", &recovery)?;
    Ok(Outcome::Continue(evidence))
}
