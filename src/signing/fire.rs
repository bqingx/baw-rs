//! `contract-call fire` and the in-memory preview→execute path used by tasks.
use super::*;

/// `contract-call fire`: preview then execute in one process, no policy file and no
/// human digest review. The caller authorises with `--confirm` + a unique
/// `--operation-id`; the preview's risk and authority data is returned so it can be
/// recorded even though nobody read it before execution.
pub(super) async fn fire_contract(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
) -> Result<Value, String> {
    if let Some(policy_path) = &args.policy {
        return run_contract(client, dir, args, policy_path).await;
    }
    let mut timing = FireTimings::default();
    let mut result = memory_fire(client, dir, args, skip_prechecks(), &mut timing).await?;
    result["timings"] = serde_json::to_value(timing).map_err(|_| "timing encoding failed")?;
    Ok(result)
}
fn fire_preview_checks(
    v: &Value,
    chain: &str,
    sender: &str,
    require_binding: bool,
    expiry: &mut u64,
) -> Result<(), String> {
    fn walk(
        v: &Value,
        chain: &str,
        sender: &str,
        positive: &mut bool,
        expiry: &mut u64,
        nested: bool,
    ) -> Result<(), String> {
        check_output(v, chain, sender)?;
        if let Some(code) = v.get("simulationCode") {
            if code != "000000000" {
                return Err("preview contains failed or malformed simulation code".into());
            }
            *positive = true;
        }
        if v.get("simulationErrorDetail")
            .is_some_and(|e| !e.is_null() && e != "")
        {
            return Err("preview contains simulation error detail".into());
        }
        for key in ["simulationSuccess", "simulationPassed", "success"] {
            if let Some(flag) = v.get(key) {
                if !flag.is_boolean() {
                    return Err("malformed simulation boolean".into());
                }
                *positive |= flag == true && (nested || key != "success");
            }
        }
        if v["simulationStatus"] == "SUCCESS" || (nested && v["status"] == "SUCCESS") {
            *positive = true;
        }
        for key in [
            "expiresAt",
            "expireAt",
            "expireTime",
            "requestExpireTime",
            "expiryTime",
        ] {
            if let Some(value) = v.get(key) {
                *expiry = (*expiry).min(timestamp(value).ok_or("invalid preview expiry")?);
            }
        }
        for key in [
            "simulation",
            "simulationResult",
            "simulationDetails",
            "simulationResults",
        ] {
            if let Some(value) = v.get(key) {
                if let Some(rows) = value.as_array() {
                    if rows.is_empty() {
                        return Err("empty simulation results".into());
                    }
                    for row in rows {
                        let mut row_positive = false;
                        walk(row, chain, sender, &mut row_positive, expiry, true)?;
                        if !row_positive {
                            return Err("every simulation result requires explicit success".into());
                        }
                        *positive = true;
                    }
                } else {
                    walk(value, chain, sender, positive, expiry, true)?;
                }
            }
        }
        Ok(())
    }
    let mut positive = false;
    walk(v, chain, sender, &mut positive, expiry, false)?;
    if !positive {
        return Err("preview requires explicit successful simulation".into());
    }
    // Some real responses omit top-level from/chain. The authenticated preview POST
    // already carried chain; accept its sender evidence in simulated balance changes.
    // Any explicit chain/from mismatch is still rejected by check_output above.
    let sender_evidence = v["from"]
        .as_str()
        .is_some_and(|s| same_address(chain, s, sender))
        || v["simulationResult"]["balanceChanges"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["owner"]
                        .as_str()
                        .is_some_and(|s| same_address(chain, s, sender))
                })
            });
    if require_binding && !sender_evidence {
        return Err(
            "preview requires explicit backend chain and sender binding when prechecks are skipped"
                .into(),
        );
    }
    if *expiry <= now()? {
        return Err("preview expired".into());
    }
    Ok(())
}
pub(crate) async fn memory_fire(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
    skip: bool,
    timing: &mut FireTimings,
) -> Result<Value, String> {
    memory_fire_checked(client, dir, args, skip, timing, &mut || Ok(())).await
}

pub(crate) async fn memory_fire_checked<F: FnMut() -> Result<(), String> + Send>(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
    skip: bool,
    timing: &mut FireTimings,
    state_check: &mut F,
) -> Result<Value, String> {
    memory_fire_guarded(client, dir, args, skip, timing, state_check, None).await
}

pub(crate) async fn memory_fire_guarded<F: FnMut() -> Result<(), String> + Send>(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
    skip: bool,
    timing: &mut FireTimings,
    state_check: &mut F,
    policy: Option<&crate::policy::Policy>,
) -> Result<Value, String> {
    let prep = std::time::Instant::now();
    if is_solana(&args.binance_chain_id) {
        return Err("fire supports EVM chains only; use preview + execute for Solana".into());
    }
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
    timing.local_prep_ms += prep.elapsed().as_secs_f64() * 1000.0;
    let started = std::time::Instant::now();
    let checked = check_wallet_policy(client, &args.binance_chain_id, Some(&args.from), skip).await;
    timing.prechecks_ms += started.elapsed().as_secs_f64() * 1000.0;
    let (sender, dev_expiry) = checked?;
    let prep = std::time::Instant::now();
    let cache = Cache::open(dir)?;
    timing.local_prep_ms += prep.elapsed().as_secs_f64() * 1000.0;
    let mut record = preview_record(
        client,
        &cache,
        "contract-call",
        &args.binance_chain_id,
        sender,
        dev_expiry,
        payload,
        timing,
    )
    .await?;
    let started = std::time::Instant::now();
    let validated = fire_preview_checks(
        &record.output,
        &record.chain,
        &record.sender,
        skip,
        &mut record.expires_at,
    );
    timing.validation_ms += started.elapsed().as_secs_f64() * 1000.0;
    validated?;
    if let Some(policy) = policy {
        fn check(
            policy: &crate::policy::Policy,
            value: &Value,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 32 {
                return Err("preview policy nesting too deep".into());
            }
            if let Some(risks) = value.get("risks").filter(|v| !v.is_null()) {
                if !risks.is_object() {
                    return Err("malformed preview risks".into());
                }
                if let Some(details) = risks.get("riskDetails").filter(|v| !v.is_null()) {
                    let rows = details.as_array().ok_or("malformed preview riskDetails")?;
                    if rows
                        .iter()
                        .any(|r| r.get("code").and_then(Value::as_str).is_none())
                    {
                        return Err("malformed preview risk code".into());
                    }
                }
            }
            policy.check_preview(value)?;
            for key in [
                "simulation",
                "simulationResult",
                "simulationDetails",
                "simulationResults",
            ] {
                if let Some(child) = value.get(key) {
                    if let Some(rows) = child.as_array() {
                        for row in rows {
                            check(policy, row, depth + 1)?;
                        }
                    } else {
                        check(policy, child, depth + 1)?;
                    }
                }
            }
            Ok(())
        }
        check(policy, &record.output, 0)?;
    }
    let prep = std::time::Instant::now();
    // If the backend echoes transaction fields, they must describe the exact request.
    // Check supported nested transaction wrappers as well as top-level echoes.
    let mut echoes = vec![&record.output];
    for key in ["tx", "transaction", "unsignedTx"] {
        if let Some(v) = record.output.get(key) {
            if !v.is_object() {
                return Err("malformed preview transaction echo".into());
            }
            echoes.push(v);
        }
    }
    if let Some(parsed) = record.output.get("parsedTx").filter(|v| v.is_object()) {
        if let Some(target) = parsed.get("contractAddress").filter(|v| !v.is_null()) {
            if !target
                .as_str()
                .zip(record.input["to"].as_str())
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                return Err("preview parsed contract mismatch".into());
            }
        }
    }
    for echo in echoes {
        check_output(echo, &record.chain, &record.sender)?;
        // A backend that has no gas estimate yet echoes `gasLimit: null`; absent and null both
        // mean "not stated", so only a concrete, unparseable value is a protocol error.
        if let Some(actual) = echo.get("gasLimit").filter(|v| !v.is_null()) {
            let gas = actual
                .as_u64()
                .or_else(|| actual.as_str().and_then(|s| s.parse::<u64>().ok()))
                .ok_or("malformed preview gasLimit echo")?;
            if args.gas_limit.is_some_and(|requested| gas != requested)
                || policy
                    .and_then(|p| p.max_gas_limit)
                    .is_some_and(|ceiling| gas > ceiling)
            {
                return Err("preview gasLimit contradicts pinned request or policy ceiling".into());
            }
        }
        for key in ["to", "inputData"] {
            if let Some(actual) = echo.get(key) {
                if !actual
                    .as_str()
                    .zip(record.input[key].as_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
                {
                    return Err("preview transaction intent mismatch".into());
                }
            }
        }
        if let Some(value) = echo.get("value") {
            let text = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            if crate::uint::uint256(&text)?
                != crate::uint::uint256(record.input["value"].as_str().unwrap_or("0"))?
            {
                return Err("preview transaction value mismatch".into());
            }
        }
    }
    let digest = record.digest()?;
    let request_id = record.request_id.clone();
    timing.local_prep_ms += prep.elapsed().as_secs_f64() * 1000.0;
    let started = std::time::Instant::now();
    let consumed = cache.consume(&request_id, &digest);
    timing.consume_ms = started.elapsed().as_secs_f64() * 1000.0;
    consumed?;
    let started = std::time::Instant::now();
    let checked = check_wallet_policy(client, &record.chain, Some(&record.sender), skip).await;
    timing.prechecks_ms += started.elapsed().as_secs_f64() * 1000.0;
    checked?;
    if record.expires_at <= now()? {
        return Err("preview expired; remains consumed".into());
    }
    state_check()?;
    let execution = client
        .request(
            Method::POST,
            &format!("{API}/contract-call/execute"),
            Some(json!({"requestId":request_id})),
        )
        .await;
    timing.execute = client.last_http.clone();
    let execution = execution?;
    let acknowledged = execution["orderId"]
        .as_str()
        .is_some_and(|s| !s.trim().is_empty())
        || execution["txHash"].as_str().is_some_and(|s| {
            s.len() == 66 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
        });
    if !execution.is_object() || !acknowledged || failed_simulation(&execution) {
        return Err("execute backend failure or malformed response; never retry".into());
    }
    let mut preview = record.output;
    preview["previewSha256"] = json!(digest);
    Ok(json!({
        "policy": "none (fire performs no policy check)",
        "preview": preview,
        "execution": execution,
    }))
}
