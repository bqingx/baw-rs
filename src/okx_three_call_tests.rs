use super::*;

fn ready(api: &Okx, args: &SwapArgs, client: &Client) -> Value {
    crate::okx_trade::prepared::proof(api, args, client, FROM, 4102444800, json!({})).unwrap()
}
fn assert_no_new_requests(
    backend: &Server,
    okx: &Server,
    rpc: &Server,
    before: (usize, usize, usize),
) {
    assert_eq!(backend.seen.lock().unwrap().len(), before.0);
    assert_eq!(okx.seen.lock().unwrap().len(), before.1);
    assert_eq!(rpc.seen.lock().unwrap().len(), before.2);
}
fn lengths(backend: &Server, okx: &Server, rpc: &Server) -> (usize, usize, usize) {
    (
        backend.seen.lock().unwrap().len(),
        okx.seen.lock().unwrap().len(),
        rpc.seen.lock().unwrap().len(),
    )
}
#[tokio::test]
async fn minimum_policy_is_explicit_for_success_and_unknown_execute_without_rewriting_args() {
    for mode in ["normal", "swap-execute-error"] {
        for floor in [None, Some("0009800")] {
            let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
            a.okx_min_output_units = floor.map(str::to_owned);
            let proof = ready(&api, &a, &client);
            let before = lengths(&backend, &okx, &rpc);
            let mut g = swap_guard(d.path(), &a);
            let out = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
                .await
                .unwrap();
            assert_eq!(
                out["status"],
                if mode == "normal" {
                    "BROADCASTED"
                } else {
                    "UNCERTAIN"
                }
            );
            assert_eq!(
                out["minimumOutputPolicy"],
                json!({
                    "mode": if floor.is_some() { "fresh_route_plus_fixed_floor" } else { "fresh_route" },
                    "fixedUserMinimumUnits": floor.map(|_| "9800"),
                    "reportedMinimumOutputUnits": "9801",
                    "slippageReferenceMinimumUnits": "9801",
                    "runtimeNetMinimumUnits": "9801",
                    "onchainMinimumVerified": false
                })
            );
            assert_eq!(
                a.okx_min_output_units.as_deref(),
                floor,
                "normalization is reporting-only"
            );
            assert_eq!(
                lengths(&backend, &okx, &rpc),
                (before.0 + 2, before.1 + 1, before.2)
            );
            assert_eq!(execute_counts(&backend), (0, 1));
        }
    }
}

#[tokio::test]
async fn no_fixed_floor_standalone_and_prepared_use_fresh_reported_minimum() {
    for prepared in [false, true] {
        let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
        a.okx_min_output_units = None;
        a.auto_approve = false;
        let proof = if prepared {
            let mut g = guard(d.path(), &a);
            let proof = allowance_scope(
                TaskAllowance::Prepare,
                swap(&mut client, d.path(), &a, &api, Some(&mut g)),
            )
            .await
            .unwrap();
            g.finish(&Ok(proof.clone())).unwrap();
            Some(proof)
        } else {
            None
        };
        let before = lengths(&backend, &okx, &rpc);
        let mut g = swap_guard(d.path(), &a);
        let result = if let Some(proof) = proof {
            trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await
        } else {
            swap(&mut client, d.path(), &a, &api, Some(&mut g)).await
        }
        .unwrap();
        assert_eq!(result["status"], "BROADCASTED");
        assert_eq!(
            a.okx_min_output_units, None,
            "must not synthesize a fixed floor"
        );
        assert_eq!(result["reportedMinimumOutputUnits"], "9801");
        assert_eq!(result["minimumOutputPolicy"]["mode"], "fresh_route");
        assert_eq!(
            result["minimumOutputPolicy"]["fixedUserMinimumUnits"],
            Value::Null
        );
        assert_eq!(
            result["runtimeBalanceValidation"]["minimumNetOutputUnits"],
            "9801"
        );
        assert_eq!(execute_counts(&backend), (0, 1));
        let rows = backend.seen.lock().unwrap();
        let core: Vec<_> = rows[before.0..]
            .iter()
            .filter(|r| r.lines().next().unwrap().contains("/contract-call/"))
            .collect();
        assert_eq!(core.len(), 2);
        assert!(core[0].contains("/contract-call/preview "));
        assert!(core[1].contains("/contract-call/execute "));
        let routes = okx.seen.lock().unwrap();
        assert_eq!(
            routes[before.1..]
                .iter()
                .filter(|r| r.contains("/swap?"))
                .count(),
            1
        );
        if prepared {
            assert_eq!(routes.len() - before.1, 1);
            assert_eq!(
                rows.len() - before.0,
                2,
                "prepared trigger must have exactly 3 core HTTP calls"
            );
            assert_eq!(rpc.seen.lock().unwrap().len(), before.2);
        }
    }
}

#[tokio::test]
async fn no_fixed_floor_never_bypasses_fresh_minimum_or_net_preview_validation() {
    for prepared in [false, true] {
        for mode in [
            "minimum-missing",
            "minimum-zero",
            "minimum-zero-rounded-floor",
            "minimum-below-slippage",
            "minimum-above-output",
            "minimum-malformed",
            "preview-below-reported",
        ] {
            let (d, mut a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
            a.okx_min_output_units = None;
            a.auto_approve = false;
            let proof = ready(&api, &a, &client);
            let before = lengths(&backend, &okx, &rpc);
            let mut g = swap_guard(d.path(), &a);
            let result = if prepared {
                trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await
            } else {
                swap(&mut client, d.path(), &a, &api, Some(&mut g)).await
            };
            let error = result.unwrap_err();
            if mode == "preview-below-reported" {
                assert!(error.contains("swap bounds"), "{error}");
            } else {
                assert!(
                    error.contains("minReceiveAmount")
                        || error.contains("minimum")
                        || error.contains("uint256"),
                    "{mode}: {error}"
                );
            }
            assert_eq!(
                execute_counts(&backend),
                (0, 0),
                "{mode}, prepared={prepared}"
            );
            assert_eq!(
                okx.seen.lock().unwrap()[before.1..]
                    .iter()
                    .filter(|r| r.contains("/swap?"))
                    .count(),
                1,
                "must validate fresh route: {mode}"
            );
            let rows = backend.seen.lock().unwrap();
            let previews = rows[before.0..]
                .iter()
                .filter(|r| r.contains("/contract-call/preview "))
                .count();
            assert_eq!(
                previews,
                usize::from(mode == "preview-below-reported"),
                "{mode}"
            );
            if prepared {
                assert_eq!(okx.seen.lock().unwrap().len() - before.1, 1);
                assert_eq!(rpc.seen.lock().unwrap().len(), before.2);
                assert_eq!(rows.len() - before.0, previews);
            }
        }
    }
}

#[tokio::test]
async fn explicit_historical_floor_still_rejects_otherwise_valid_lower_reported_minimum() {
    for prepared in [false, true] {
        let (d, mut a, api, _w, _rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
        a.okx_min_output_units = Some("9850".into());
        a.auto_approve = false;
        let proof = ready(&api, &a, &client);
        let original = proof.clone();
        let mut g = swap_guard(d.path(), &a);
        let result = if prepared {
            trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await
        } else {
            swap(&mut client, d.path(), &a, &api, Some(&mut g)).await
        };
        assert!(result.unwrap_err().contains("minimum"));
        assert_eq!(a.okx_min_output_units.as_deref(), Some("9850"));
        assert_eq!(
            ready(&api, &a, &client),
            original,
            "historical explicit floor must not be rewritten"
        );
        assert_eq!(execute_counts(&backend), (0, 0));
        assert_eq!(
            okx.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.contains("/swap?"))
                .count(),
            1
        );
        assert!(!backend
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.contains("/contract-call/preview ")));
    }
}

#[tokio::test]
async fn successful_trigger_is_exactly_one_route_one_preview_one_execute() {
    let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
    let mut g = guard(d.path(), &a);
    let proof = allowance_scope(
        TaskAllowance::Prepare,
        swap(&mut client, d.path(), &a, &api, Some(&mut g)),
    )
    .await
    .unwrap();
    g.finish(&Ok(proof.clone())).unwrap();
    drop(g);
    let before = lengths(&backend, &okx, &rpc);
    let mut g = swap_guard(d.path(), &a);
    let result = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g)))
        .await
        .unwrap();
    assert_eq!(result["status"], "BROADCASTED");
    assert_eq!(result["preparedWalletPolicyUsed"], true);
    let requests = backend.seen.lock().unwrap();
    let tail = &requests[before.0..];
    assert_eq!(tail.len(), 2);
    assert!(tail[0]
        .lines()
        .next()
        .unwrap()
        .contains("/contract-call/preview "));
    assert!(tail[1]
        .lines()
        .next()
        .unwrap()
        .contains("/contract-call/execute "));
    let requests = okx.seen.lock().unwrap();
    assert_eq!(requests.len() - before.1, 1);
    assert!(requests[before.1]
        .lines()
        .next()
        .unwrap()
        .contains("/swap?"));
    assert_eq!(rpc.seen.lock().unwrap().len(), before.2);
}
#[tokio::test]
async fn missing_legacy_or_modified_policy_stops_before_route_fetch() {
    let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
    let proof = ready(&api, &a, &client);
    let before = lengths(&backend, &okx, &rpc);
    let mut cases = Vec::new();
    let mut old_policy = proof.clone();
    old_policy.as_object_mut().unwrap().remove("calldataPolicy");
    cases.push(old_policy);
    let mut previous_schema = proof.clone();
    previous_schema["calldataPolicy"] = json!("pinned_okx_opaque_v2");
    cases.push(previous_schema);
    let mut wrong_policy = proof.clone();
    wrong_policy["calldataPolicy"] = json!("old_local_abi_policy");
    cases.push(wrong_policy);
    let mut legacy = proof.clone();
    legacy.as_object_mut().unwrap().remove("walletPolicy");
    cases.push(legacy);
    for (field, value) in [
        ("sender", json!(INPUT)),
        ("chain", json!("1")),
        ("developerModeExpiresAt", json!(1)),
        ("clientBinding", json!("0".repeat(64))),
    ] {
        let mut bad = proof.clone();
        bad["walletPolicy"][field] = value;
        cases.push(bad);
    }
    for bad in cases {
        assert!(trigger(bad, swap(&mut client, d.path(), &a, &api, None))
            .await
            .is_err());
        assert_no_new_requests(&backend, &okx, &rpc, before);
    }
}
#[tokio::test]
async fn prepared_record_cannot_be_used_without_original_execution_context() {
    let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
    let proof = ready(&api, &a, &client);
    let before = lengths(&backend, &okx, &rpc);
    assert!(allowance_scope(
        TaskAllowance::Prepared(proof.clone()),
        swap(&mut client, d.path(), &a, &api, None)
    )
    .await
    .is_err());
    assert!(crate::execution::task_scope(
        u64::MAX,
        Arc::new(AtomicBool::new(false)),
        allowance_scope(
            TaskAllowance::Prepared(proof),
            swap(&mut client, d.path(), &a, &api, None)
        )
    )
    .await
    .is_err());
    assert_no_new_requests(&backend, &okx, &rpc, before);
}
#[tokio::test]
async fn deadline_cancellation_or_changed_binding_prevents_every_core_request() {
    let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, "normal");
    let proof = ready(&api, &a, &client);
    let before = lengths(&backend, &okx, &rpc);
    for (deadline, cancel, binding_ok) in [
        (1, false, true),
        (u64::MAX, true, true),
        (u64::MAX, false, false),
    ] {
        let check = Arc::new(move || {
            if binding_ok {
                Ok(())
            } else {
                Err("changed wallet binding".into())
            }
        });
        assert!(crate::execution::binding_scope(
            check,
            crate::execution::task_scope(
                deadline,
                Arc::new(AtomicBool::new(cancel)),
                allowance_scope(
                    TaskAllowance::Prepared(proof.clone()),
                    swap(&mut client, d.path(), &a, &api, None)
                )
            )
        )
        .await
        .is_err());
        assert_no_new_requests(&backend, &okx, &rpc, before);
    }
}
#[tokio::test]
async fn failed_standalone_execute_precheck_is_not_reported_as_an_execute_http_request() {
    let (d, a, api, _w, _rpc, _okx, backend, mut client) = setup(AMOUNT, "mode-off-after-preview");
    let mut g = swap_guard(d.path(), &a);
    let result = swap(&mut client, d.path(), &a, &api, Some(&mut g))
        .await
        .unwrap();
    assert_eq!(result["status"], "UNCERTAIN");
    assert_eq!(execute_counts(&backend), (0, 0));
    let transport = &result["hotPathTiming"]["httpTransport"];
    assert_eq!(transport["binancePreview"]["endpoint"], "contract_preview");
    assert!(transport["binanceExecute"]["endpoint"].is_null());
    assert!(transport["binanceExecute"]["connection"].is_null());
}

#[tokio::test]
async fn standalone_order_only_or_malformed_hash_is_uncertain_and_durable() {
    for mode in ["swap-order-only", "swap-invalid-hash"] {
        let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
        let before = lengths(&backend, &okx, &rpc);
        let mut guard = swap_guard(d.path(), &a);
        let result = swap(&mut client, d.path(), &a, &api, Some(&mut guard)).await;
        let value = result.as_ref().unwrap();
        assert_eq!(value["status"], "UNCERTAIN");
        assert_eq!(value["backendReportedStatus"], "BROADCASTED");
        assert_eq!(value["orderId"], "swap-order");
        assert_eq!(value["retrySafe"], false);
        assert_eq!(execute_counts(&backend), (0, 1));
        guard.finish(&result).unwrap();
        drop(guard);
        let after = lengths(&backend, &okx, &rpc);
        assert!(OperationGuard::begin(
            d.path(),
            "prepared-swap",
            "market-order.swap",
            &json!({}),
            "synthetic-okx-client"
        )
        .is_err());
        assert_no_new_requests(&backend, &okx, &rpc, after);
        assert!(after.0 > before.0);
    }
}

#[tokio::test]
async fn backend_rejection_or_unknown_execute_never_refreshes_repairs_or_retries() {
    for mode in [
        "swap-preview-error",
        "swap-execute-error",
        "swap-pending-confirmation",
    ] {
        let (d, a, api, _w, rpc, okx, backend, mut client) = setup(AMOUNT, mode);
        let proof = ready(&api, &a, &client);
        let before = lengths(&backend, &okx, &rpc);
        let mut g = swap_guard(d.path(), &a);
        let result = trigger(proof, swap(&mut client, d.path(), &a, &api, Some(&mut g))).await;
        if mode == "swap-preview-error" {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.as_ref().unwrap()["status"],
                if mode == "swap-pending-confirmation" {
                    "PENDING_CONFIRMATION"
                } else {
                    "UNCERTAIN"
                }
            );
            assert_eq!(result.as_ref().unwrap()["retrySafe"], false);
        }
        if let Ok(value) = &result {
            let transport = &value["hotPathTiming"]["httpTransport"];
            assert_eq!(transport["binancePreview"]["endpoint"], "contract_preview");
            assert_eq!(transport["binanceExecute"]["endpoint"], "contract_execute");
            assert!(transport["binanceExecute"]["connection"].is_object());
        }
        g.finish(&result).unwrap();
        drop(g);
        let rows = backend.seen.lock().unwrap();
        let tail = &rows[before.0..];
        assert_eq!(tail.len(), if mode == "swap-preview-error" { 1 } else { 2 });
        assert!(tail.iter().all(|r| {
            let line = r.lines().next().unwrap();
            line.contains("contract-call/preview") || line.contains("contract-call/execute")
        }));
        assert_eq!(okx.seen.lock().unwrap().len() - before.1, 1);
        assert_eq!(rpc.seen.lock().unwrap().len(), before.2);
        assert!(OperationGuard::begin(
            d.path(),
            "prepared-swap",
            "market-order.swap",
            &json!({}),
            "synthetic-okx-client"
        )
        .is_err());
    }
}
