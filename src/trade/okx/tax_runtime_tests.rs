use super::approval_tests::{execute_counts, guard, setup, AMOUNT};
use super::prepared_tests::{swap_guard, trigger};
use super::tests::{Server, FROM, SPENDER};
use super::*;
use crate::exec::execution::{allowance_scope, TaskAllowance};

fn counts(backend: &Server, okx: &Server, rpc: &Server) -> (usize, usize, usize) {
    (
        backend.seen.lock().unwrap().len(),
        okx.seen.lock().unwrap().len(),
        rpc.seen.lock().unwrap().len(),
    )
}
#[tokio::test]
async fn generic_input_output_and_double_tax_use_net_preview_without_extra_trigger_calls() {
    for mode in ["tax-input", "tax-output", "tax-both"] {
        let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
        a.quote.max_from_tax_percent = "3".into();
        a.quote.max_to_tax_percent = "3".into();
        if mode != "tax-input" {
            a.okx_min_output_units = Some("9600".into());
        }
        let mut prep = guard(d.path(), &a);
        let proof = allowance_scope(
            TaskAllowance::Prepare,
            swap(&mut client, d.path(), &a, &api, Some(&mut prep)),
        )
        .await
        .unwrap();
        prep.finish(&Ok(proof.clone())).unwrap();
        drop(prep);
        let before = counts(&backend, &okx, &rpc);
        let mut g = swap_guard(d.path(), &a);
        let out = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
            .await
            .unwrap();
        assert_eq!(out["status"], "BROADCASTED", "{mode}");
        assert_eq!(
            out["runtimeBalanceValidation"]["walletDebitUnits"],
            AMOUNT.to_string()
        );
        assert_eq!(
            out["runtimeBalanceValidation"]["walletNetCreditUnits"],
            if mode == "tax-input" { "9900" } else { "9702" }
        );
        assert_eq!(
            out["quote"]["toTokenAmount"],
            if mode == "tax-input" { "9900" } else { "9702" },
            "must not blindly deduct tax twice"
        );
        assert_eq!(
            out["onchainNetMinimumDecoded"], false,
            "opaque tax opt-in must not claim a hard net minimum"
        );
        assert_eq!(out["transferTax"]["taxed"], true);
        let after = counts(&backend, &okx, &rpc);
        assert_eq!(after, (before.0 + 2, before.1 + 1, before.2));
        let rows = backend.seen.lock().unwrap();
        assert!(rows[before.0].contains("/contract-call/preview "));
        assert!(rows[before.0 + 1].contains("/contract-call/execute "));
        drop(rows);
        assert_eq!(execute_counts(&backend), (0, 1));
    }
}
#[tokio::test]
async fn no_fixed_floor_taxed_output_uses_fresh_net_minimum_with_three_core_calls() {
    let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "tax-output");
    a.quote.max_to_tax_percent = "3".into();
    a.okx_min_output_units = None;
    let mut prep = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut prep)),
    )
    .await
    .unwrap();
    prep.finish(&Ok(proof.clone())).unwrap();
    drop(prep);
    let before = counts(&backend, &okx, &rpc);
    let mut g = swap_guard(d.path(), &a);
    let out = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
        .await
        .unwrap();
    assert_eq!(out["status"], "BROADCASTED");
    assert_eq!(out["reportedMinimumOutputUnits"], "9605");
    assert_eq!(
        out["runtimeBalanceValidation"]["minimumNetOutputUnits"],
        "9605"
    );
    assert_eq!(
        out["runtimeBalanceValidation"]["walletNetCreditUnits"],
        "9702"
    );
    assert_eq!(a.okx_min_output_units, None);
    assert_eq!(
        counts(&backend, &okx, &rpc),
        (before.0 + 2, before.1 + 1, before.2)
    );
    assert_eq!(execute_counts(&backend), (0, 1));
}

#[tokio::test]
async fn tax_over_cap_fails_before_preparation_spends_approval_gas() {
    let (d, mut a, api, _w, _rpc, _okx, backend, mut client) = setup(0, "tax-prep-cap");
    a.quote.max_from_tax_percent = "3".into();
    let mut g = guard(d.path(), &a);
    let out = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await;
    assert!(out.is_err());
    assert_eq!(execute_counts(&backend), (0, 0));
}
#[tokio::test]
async fn changed_tax_is_checked_on_fresh_swap_and_never_repaired_or_reapproved() {
    let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(0, "tax-cap-change");
    a.quote.max_from_tax_percent = "3".into();
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    assert_eq!(
        proof["autoApproval"]["amount"],
        AMOUNT.to_string(),
        "approval must never be grossed up for tax"
    );
    g.finish(&Ok(proof.clone())).unwrap();
    drop(g);
    let before = counts(&backend, &okx, &rpc);
    let mut g = swap_guard(d.path(), &a);
    assert!(
        trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
            .await
            .is_err()
    );
    let after = counts(&backend, &okx, &rpc);
    assert_eq!(after, (before.0, before.1 + 1, before.2));
    assert_eq!(execute_counts(&backend), (1, 0));
}
#[tokio::test]
async fn simulated_net_shortfall_and_extra_sender_tax_stop_before_execute() {
    for (mode, floor) in [
        ("tax-output-shortfall", Some("9800")),
        ("tax-extra-debit", Some("9800")),
        ("tax-output-shortfall", None),
        ("tax-extra-debit", None),
    ] {
        let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
        a.okx_min_output_units = floor.map(str::to_owned);
        a.quote.max_from_tax_percent = "3".into();
        a.quote.max_to_tax_percent = "3".into();
        let proof =
            crate::trade::okx::prepared::proof(&api, &a, &client, FROM, 4102444800, json!({}))
                .unwrap();
        let before = counts(&backend, &okx, &rpc);
        let mut g = swap_guard(d.path(), &a);
        let result = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await;
        assert!(result.err().unwrap().contains("swap bounds"), "{mode}");
        assert_eq!(execute_counts(&backend), (0, 0));
        let after = counts(&backend, &okx, &rpc);
        assert_eq!(after, (before.0 + 1, before.1 + 1, before.2));
    }
}
#[tokio::test]
async fn tax_caps_are_bound_to_prepared_intent_before_any_route_request() {
    let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "tax-input");
    a.quote.max_from_tax_percent = "3".into();
    let proof =
        crate::trade::okx::prepared::proof(&api, &a, &client, FROM, 4102444800, json!({})).unwrap();
    let before = counts(&backend, &okx, &rpc);
    a.quote.max_from_tax_percent = "4".into();
    assert!(trigger(proof, swap(&mut client, d.path(), &a, &api, None))
        .await
        .is_err());
    assert_eq!(counts(&backend, &okx, &rpc), before);
}
#[tokio::test]
async fn output_tax_routes_all_use_opaque_policy_and_net_preview() {
    for mode in ["tax-output-v2", "tax-output-fot"] {
        let (d, mut a, mut api, _w, _rpc, _okx, backend, mut client) = setup(AMOUNT, mode);
        a.quote.max_to_tax_percent = "3".into();
        a.okx_min_output_units = Some("9600".into());
        api.profile.allowed_router_addresses = vec![SPENDER.into()];
        let proof =
            crate::trade::okx::prepared::proof(&api, &a, &client, FROM, 4102444800, json!({}))
                .unwrap();
        let mut g = swap_guard(d.path(), &a);
        let result = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await;
        let out = result.unwrap();
        assert_eq!(out["status"], "BROADCASTED");
        assert_eq!(out["onchainMinimumDecoded"], false);
        assert_eq!(out["onchainNetMinimumDecoded"], false);
        assert_eq!(out["routeValidation"]["recipient_verified"], false);
        assert_eq!(
            out["calldataPolicy"],
            crate::trade::route_action::CALLDATA_POLICY
        );
        assert_eq!(
            out["runtimeBalanceValidation"]["walletNetCreditUnits"],
            "9702"
        );
        assert_eq!(execute_counts(&backend), (0, 1));
    }
}

#[tokio::test]
async fn zero_default_cap_still_blocks_taxed_tokens_without_silent_opt_in() {
    let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "tax-input");
    let proof =
        crate::trade::okx::prepared::proof(&api, &a, &client, FROM, 4102444800, json!({})).unwrap();
    let before = counts(&backend, &okx, &rpc);
    assert!(trigger(proof, swap(&mut client, d.path(), &a, &api, None))
        .await
        .is_err());
    assert_eq!(execute_counts(&backend), (0, 0));
    let after = counts(&backend, &okx, &rpc);
    assert_eq!(after, (before.0, before.1 + 1, before.2));
}
