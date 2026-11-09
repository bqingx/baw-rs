//! Private task-local handoff: preparation never swaps, execution never reads remote policy.
use super::*;
use crate::execution::PreparedWalletPolicy;

pub(super) fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn binding(api: &Okx, args: &SwapArgs, policy: &PreparedWalletPolicy) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(&json!({"profile":api.profile_hash,"args":args,"walletPolicy":policy,"calldataPolicy":route_action::CALLDATA_POLICY}))
            .map_err(|_| "cannot bind prepared allowance")?;
    Ok(digest(&bytes))
}
pub(super) fn proof(
    api: &Okx,
    args: &SwapArgs,
    client: &Client,
    sender: &str,
    expires: u64,
    approval: Value,
) -> Result<Value, String> {
    let policy =
        PreparedWalletPolicy::capture(client, &args.quote.binance_chain_id, sender, expires)?;
    Ok(
        json!({"status":"ALLOWANCE_READY","preparedBinding":binding(api,args,&policy)?,
        "walletPolicy":policy,"autoApproval":approval,"preparedBeforeTrigger":true,"calldataPolicy":route_action::CALLDATA_POLICY}),
    )
}
pub(super) fn validate(
    api: &Okx,
    args: &SwapArgs,
    client: &Client,
    proof: &Value,
) -> Result<PreparedWalletPolicy, String> {
    if proof["calldataPolicy"] != route_action::CALLDATA_POLICY {
        return Err(
            "prepared calldata policy changed or missing; no implicit intent upgrade".into(),
        );
    }
    let policy = PreparedWalletPolicy::from_ready(proof)?;
    policy.check(client, &args.quote.binance_chain_id, None)?;
    if proof["preparedBinding"].as_str() != Some(binding(api, args, &policy)?.as_str()) {
        return Err("prepared allowance does not match wallet, intent or current OKX profile; no on-trigger refresh permitted".into());
    }
    Ok(policy)
}
