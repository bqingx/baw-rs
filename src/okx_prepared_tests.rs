use super::*;
use crate::execution::{allowance_scope, TaskAllowance};
#[path = "okx_tax_runtime_tests.rs"]
mod tax_runtime_tests;
#[path = "okx_three_call_tests.rs"]
mod three_call_tests;

async fn trigger<F: std::future::Future>(proof: Value, future: F) -> F::Output {
    crate::execution::binding_scope(
        Arc::new(|| Ok(())),
        crate::execution::task_scope(
            (chrono::Utc::now().timestamp_millis() as u64) + 60_000,
            Arc::new(AtomicBool::new(false)),
            allowance_scope(TaskAllowance::Prepared(proof), future),
        ),
    )
    .await
}
fn test_client() -> Client {
    Client::test_client(
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-okx-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-okx-session".into()),
        },
        "http://127.0.0.1:9",
    )
    .unwrap()
}
fn method_count(server: &Server, method: &str) -> usize {
    server
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| {
            s.split_once("\r\n\r\n")
                .and_then(|(_, body)| serde_json::from_str::<Value>(body).ok())
                .is_some_and(|v| v["method"] == method)
        })
        .count()
}
fn swap_guard(dir: &Path, args: &SwapArgs) -> OperationGuard {
    OperationGuard::begin(
        dir,
        "prepared-swap",
        "market-order.swap",
        &crate::trade::mutation_payload(&MarketCommand::Swap(args.clone())).unwrap(),
        "synthetic-okx-client",
    )
    .unwrap()
}

#[tokio::test]
async fn authorization_is_complete_before_trigger_and_trigger_does_not_query_or_approve() {
    let (d, a, api, w, rpc, okx, backend, mut client) = setup(0, "one-confirmation");
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(proof["status"], "ALLOWANCE_READY");
    assert_eq!(proof["autoApproval"]["confirmations"], 1);
    assert_eq!(proof["autoApproval"]["allowanceRechecked"], false);
    assert!(proof["autoApproval"].get("allowanceAfter").is_none());
    assert_eq!(execute_counts(&backend), (1, 0));
    assert_eq!(method_count(&rpc, "eth_call"), 1);
    assert_eq!(method_count(&rpc, "eth_blockNumber"), 0);
    assert_eq!(method_count(&rpc, "eth_getBlockByNumber"), 0);
    assert_eq!(method_count(&rpc, "eth_getTransactionReceipt"), 1);
    assert!(!w.phases.lock().unwrap().contains(&"swap_quote"));
    g.finish(&Ok(proof.clone())).unwrap();
    drop(g);
    let rpc_before = rpc.seen.lock().unwrap().len();
    let approve_requests = okx
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.contains("/approve-transaction?"))
        .count();
    let mut g = swap_guard(d.path(), &a);
    let result = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
        .await
        .unwrap();
    assert_eq!(result["status"], "BROADCASTED");
    assert_eq!(result["allowancePreparedBeforeTrigger"], true);
    assert_eq!(execute_counts(&backend), (1, 1));
    assert_eq!(rpc.seen.lock().unwrap().len(), rpc_before);
    assert_eq!(
        okx.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.contains("/approve-transaction?"))
            .count(),
        approve_requests
    );
    assert_eq!(
        *w.phases.lock().unwrap(),
        vec![
            "pre_quote",
            "approval_execute",
            "receipt",
            "swap_quote",
            "swap_execute"
        ]
    );
}

#[tokio::test]
async fn no_fixed_floor_auto_approval_validates_metadata_without_freezing_preparation_quote() {
    let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(0, "lower-fresh-output");
    a.okx_min_output_units = None;
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(proof["status"], "ALLOWANCE_READY");
    assert_eq!(execute_counts(&backend), (1, 0));
    assert_eq!(
        okx.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/quote?"))
            .count(),
        1
    );
    g.finish(&Ok(proof.clone())).unwrap();
    drop(g);
    let before = (
        backend.seen.lock().unwrap().len(),
        okx.seen.lock().unwrap().len(),
        rpc.seen.lock().unwrap().len(),
    );
    let mut g = swap_guard(d.path(), &a);
    let out = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
        .await
        .unwrap();
    assert_eq!(out["status"], "BROADCASTED");
    assert_eq!(out["quote"]["toTokenAmount"], "9000");
    assert_eq!(out["reportedMinimumOutputUnits"], "8910");
    assert_eq!(out["minimumOutputPolicy"]["mode"], "fresh_route");
    assert_eq!(
        out["minimumOutputPolicy"]["fixedUserMinimumUnits"],
        Value::Null
    );
    assert_eq!(
        out["minimumOutputPolicy"]["slippageReferenceMinimumUnits"],
        "8910"
    );
    assert_eq!(
        out["runtimeBalanceValidation"]["walletNetCreditUnits"],
        "9000"
    );
    assert_eq!(execute_counts(&backend), (1, 1));
    assert_eq!(
        (
            backend.seen.lock().unwrap().len(),
            okx.seen.lock().unwrap().len(),
            rpc.seen.lock().unwrap().len()
        ),
        (before.0 + 2, before.1 + 1, before.2)
    );
    assert_eq!(a.okx_min_output_units, None);
}

#[tokio::test]
async fn no_fixed_floor_metadata_tax_validation_still_precedes_approval() {
    let (d, mut a, api, _w, _rpc, okx, backend, mut client) = setup(0, "tax-prep-cap");
    a.okx_min_output_units = None;
    a.quote.max_from_tax_percent = "3".into();
    let mut g = guard(d.path(), &a);
    assert!(allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .is_err());
    assert_eq!(
        okx.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/quote?"))
            .count(),
        1
    );
    assert_eq!(execute_counts(&backend), (0, 0));
}

#[tokio::test]
async fn consumed_prepared_allowance_fails_without_on_trigger_reapproval() {
    let (d, a, api, w, rpc, _okx, backend, mut client) = setup(0, "unchanged");
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(proof["status"], "ALLOWANCE_READY");
    g.finish(&Ok(proof.clone())).unwrap();
    drop(g);
    let before = rpc.seen.lock().unwrap().len();
    let mut g = swap_guard(d.path(), &a);
    assert!(
        trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
            .await
            .is_err()
    );
    assert_eq!(rpc.seen.lock().unwrap().len(), before);
    assert_eq!(execute_counts(&backend), (1, 0));
    assert_eq!(
        w.phases
            .lock()
            .unwrap()
            .iter()
            .filter(|s| **s == "approval_execute")
            .count(),
        1
    );
}

#[tokio::test]
async fn existing_allowance_without_auto_approval_can_be_prepared_without_writes() {
    let (d, mut a, api, _w, rpc, _okx, backend, mut client) = setup(AMOUNT, "normal");
    a.auto_approve = false;
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(proof["status"], "ALLOWANCE_READY");
    assert_eq!(proof["autoApproval"]["needed"], false);
    assert_eq!(proof["autoApproval"]["enabled"], false);
    assert_eq!(method_count(&rpc, "eth_call"), 1);
    assert_eq!(execute_counts(&backend), (0, 0));
}

#[tokio::test]
async fn insufficient_allowance_without_approval_consent_fails_before_signing() {
    let (d, mut a, api, _w, _rpc, _okx, backend, mut client) = setup(0, "normal");
    a.auto_approve = false;
    let mut g = guard(d.path(), &a);
    let error = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap_err();
    assert!(error.contains("not authorized"));
    assert_eq!(execute_counts(&backend), (0, 0));
}

#[tokio::test]
async fn ambiguous_or_reverted_preparation_never_produces_a_ready_proof() {
    for mode in ["no-hash", "revert", "execute-error"] {
        let (d, a, api, _w, _rpc, _okx, backend, mut client) = setup(0, mode);
        let mut g = guard(d.path(), &a);
        let result = allowance_scope(
            TaskAllowance::Prepare,
            swap(&mut client, d.path(), &a, &api, Some(&mut g)),
        )
        .await
        .unwrap();
        assert_ne!(result["status"], "ALLOWANCE_READY");
        assert!(super::super::super::prepared::validate(&api, &a, &client, &result).is_err());
        assert_eq!(execute_counts(&backend), (1, 0));
    }
}

#[tokio::test]
async fn prepared_fixed_floor_option_and_value_are_bound_before_every_network_request() {
    let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
    let floors = [None, Some("9800"), Some("9850")];
    let mut bindings = Vec::new();
    for original in floors {
        a.okx_min_output_units = original.map(str::to_owned);
        let proof =
            crate::okx_trade::prepared::proof(&api, &a, &client, FROM, 4102444800, json!({}))
                .unwrap();
        let saved = proof.clone();
        bindings.push(proof["preparedBinding"].clone());
        assert!(crate::okx_trade::prepared::validate(&api, &a, &client, &proof).is_ok());
        for replacement in floors {
            if replacement == original {
                continue;
            }
            a.okx_min_output_units = replacement.map(str::to_owned);
            let before = (
                backend.seen.lock().unwrap().len(),
                okx.seen.lock().unwrap().len(),
                rpc.seen.lock().unwrap().len(),
            );
            let error = trigger(proof.clone(), swap(&mut client, d.path(), &a, &api, None))
                .await
                .unwrap_err();
            assert!(
                error.contains("does not match"),
                "{original:?} -> {replacement:?}: {error}"
            );
            assert_eq!(
                (
                    backend.seen.lock().unwrap().len(),
                    okx.seen.lock().unwrap().len(),
                    rpc.seen.lock().unwrap().len()
                ),
                before
            );
            assert_eq!(
                proof, saved,
                "old explicit-floor records must remain unchanged"
            );
        }
        a.okx_min_output_units = original.map(str::to_owned);
        assert!(crate::okx_trade::prepared::validate(&api, &a, &client, &proof).is_ok());
    }
    assert_ne!(bindings[0], bindings[1]);
    assert_ne!(bindings[0], bindings[2]);
    assert_ne!(bindings[1], bindings[2]);
}

#[test]
fn prepared_proof_binds_profile_sender_amount_and_policy() {
    use super::super::super::prepared::{proof, validate};
    let (_d, a, mut api) = fixture();
    let client = test_client();
    let ready = proof(&api, &a, &client, FROM, 4102444800, json!({})).unwrap();
    assert!(validate(&api, &a, &client, &ready).is_ok());
    let mut bad = ready.clone();
    bad["walletPolicy"]["sender"] = json!(INPUT);
    assert!(validate(&api, &a, &client, &bad).is_err());
    let mut changed = a.clone();
    changed.quote.from_token_qty = "2.5".into();
    assert!(validate(&api, &changed, &client, &ready).is_err());
    changed = a.clone();
    changed.quote.slippage = "20".into();
    assert!(validate(&api, &changed, &client, &ready).is_err());
    changed = a.clone();
    changed.acknowledge_token_risk = false;
    assert!(validate(&api, &changed, &client, &ready).is_err());
    api.profile_hash = "changed-private-profile".into();
    assert!(validate(&api, &a, &client, &ready).is_err());
}

#[tokio::test]
async fn native_input_preparation_needs_no_allowance_rpc_or_approval() {
    let (d, mut a, api, _w, rpc, _okx, backend, mut client) = setup(0, "normal");
    a.quote.from_token = NATIVE.into();
    a.quote.from_token_decimals = Some(18);
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(proof["status"], "ALLOWANCE_READY");
    assert_eq!(proof["autoApproval"]["needed"], false);
    assert!(rpc.seen.lock().unwrap().is_empty());
    assert_eq!(execute_counts(&backend), (0, 0));
}
