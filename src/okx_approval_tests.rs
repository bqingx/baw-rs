use super::*;
const AMOUNT: u64 = 1_250_000;
fn approval_hash() -> String {
    format!("0x{}", "b".repeat(64))
}
fn block_hash() -> String {
    format!("0x{}", "c".repeat(64))
}
struct World {
    before: u64,
    after: u64,
    mode: &'static str,
    mined: AtomicBool,
    phases: Mutex<Vec<&'static str>>,
}
fn world(before: u64, mode: &'static str) -> Arc<World> {
    Arc::new(World {
        before,
        after: match mode {
            "mismatch" => AMOUNT + 1,
            "unchanged" => 0,
            _ => AMOUNT,
        },
        mode,
        mined: AtomicBool::new(false),
        phases: Mutex::new(Vec::new()),
    })
}
fn rpc(w: Arc<World>) -> Server {
    serve(move |request| {
        let v: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        let result = match v["method"].as_str().unwrap() {
            "eth_chainId" => json!(if w.mode == "wrong-chain" {
                "0x1"
            } else {
                "0x38"
            }),
            "eth_call" => {
                assert_eq!(v["params"][0]["to"], INPUT);
                let expected = format!("0xdd62ed3e{:0>64}{:0>64}", &FROM[2..], &SPENDER[2..]);
                assert_eq!(v["params"][0]["data"], expected);
                if w.mode == "malformed" {
                    json!("0x")
                } else {
                    let n = if w.mined.load(Ordering::SeqCst) {
                        w.phases.lock().unwrap().push("allowance_after");
                        w.after
                    } else {
                        w.before
                    };
                    json!(format!("0x{n:064x}"))
                }
            }
            "eth_getTransactionReceipt" => {
                assert_eq!(v["params"][0], approval_hash());
                if w.mode == "missing" {
                    Value::Null
                } else {
                    w.phases.lock().unwrap().push("receipt");
                    w.mined.store(true, Ordering::SeqCst);
                    json!({"transactionHash":approval_hash(),"from":FROM,"to":if w.mode=="bad-to"{SPENDER}else{INPUT},
                        "blockNumber":"0x64","blockHash":block_hash(),"status":if w.mode=="revert"{"0x0"}else{"0x1"}})
                }
            }
            "eth_blockNumber" => json!(if w.mode == "one-confirmation" {
                "0x64"
            } else {
                "0x65"
            }),
            "eth_getBlockByNumber" => {
                json!({"number":"0x64","hash":if w.mode=="reorg"{approval_hash()}else{block_hash()}})
            }
            method => panic!("unexpected RPC method {method}"),
        };
        (200, json!({"jsonrpc":"2.0","id":1,"result":result}))
    })
}
fn approval_preview() -> Value {
    let mut v = preview();
    v["requestId"] = json!("auto-approve-preview");
    v["parsedTx"]["contractAddress"] = json!(INPUT);
    v["simulationResult"]["balanceChanges"] = json!([]);
    v["simulationResult"]["allowanceChanges"] = json!([{"owner":FROM,"contractAddress":INPUT,"spender":SPENDER,"amount":AMOUNT.to_string()}]);
    v
}
fn binance_approval_preview() -> Value {
    let mut v = approval_preview();
    v["simulationResult"]["allowanceChanges"] = json!([{"owner":FROM,"tokenAddress":INPUT,
        "spender":SPENDER,"preAmount":0,"postAmount":AMOUNT,"tokenType":"Erc20","tokenId":null}]);
    v
}
fn backend(w: Arc<World>, approve_preview: Value) -> (Server, Client) {
    let server = serve(move |request| {
        let line = request.lines().next().unwrap();
        let value = if line.contains("settings/query") {
            json!({"dailyLimit":"100","devMode":{"enabled":true,"expiresAt":4102444800u64}})
        } else if line.contains("trading-limit/query") {
            json!({"quotaUsed":"1"})
        } else if line.contains("tx-lock-status") {
            json!({"status":"UNLOCKED"})
        } else if line.contains("mpc-wallet/list") {
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":FROM}]}]}])
        } else if line.contains("networks/active") {
            json!([])
        } else if line.contains("contract-call/preview") {
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            if body["inputData"]
                .as_str()
                .unwrap()
                .starts_with("0x095ea7b3")
            {
                assert_eq!(body["to"], INPUT);
                assert_eq!(body["value"], "0");
                assert_eq!(
                    body["inputData"],
                    crate::approvals::approve_calldata(SPENDER, &AMOUNT.to_string(), false)
                        .unwrap()
                );
                assert_eq!(body["gasLimit"], 300000);
                approve_preview.clone()
            } else {
                let mut v = preview();
                let before = if w.mined.load(Ordering::SeqCst) {
                    w.after
                } else {
                    w.before
                };
                v["simulationResult"]["allowanceChanges"] = json!([{"owner":FROM,"tokenAddress":INPUT,
                    "spender":SPENDER,"preAmount":before,"postAmount":before-AMOUNT,"tokenType":"Erc20","tokenId":null}]);
                v
            }
        } else if line.contains("contract-call/execute") {
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            if body["requestId"] == "auto-approve-preview" {
                w.phases.lock().unwrap().push("approval_execute");
                if w.mode == "execute-error" {
                    return (503, json!({"private":"not echoed"}));
                }
                if w.mode == "no-hash" {
                    json!({"orderId":"approval-order","status":"PENDING_CONFIRMATION"})
                } else {
                    json!({"orderId":"approval-order","status":"BROADCASTED","txHash":approval_hash()})
                }
            } else {
                w.phases.lock().unwrap().push("swap_execute");
                json!({"orderId":"swap-order","status":"BROADCASTED","txHash":format!("0x{}","a".repeat(64))})
            }
        } else {
            panic!("unexpected Binance request {line}")
        };
        (200, json!({"code":"000000","data":value}))
    });
    let c = crate::session::Credentials {
        client_id: zeroize::Zeroizing::new("synthetic-okx-client".into()),
        session_id: zeroize::Zeroizing::new("synthetic-okx-session".into()),
    };
    let client = Client::test_client(c, &server.origin).unwrap();
    (server, client)
}
fn provider(w: Arc<World>) -> Server {
    serve(move |request| {
        let line = request.lines().next().unwrap();
        if line.contains("/approve-transaction?") {
            (
                200,
                json!({"code":"0","data":[{"dexContractAddress":SPENDER}]}),
            )
        } else if line.contains("/quote?") {
            assert!(!w.mined.load(Ordering::SeqCst));
            w.phases.lock().unwrap().push("pre_quote");
            (200, json!({"code":"0","data":[quoted()]}))
        } else if line.contains("/swap?") {
            assert!(w.before >= AMOUNT || w.mined.load(Ordering::SeqCst));
            w.phases.lock().unwrap().push("swap_quote");
            let mut r = response();
            if w.mode == "bad-fresh-quote" {
                r["data"][0]["tx"]["minReceiveAmount"] = json!("1");
            }
            (200, r)
        } else {
            panic!("unexpected OKX request {line}")
        }
    })
}
fn guard(d: &Path, a: &SwapArgs) -> OperationGuard {
    OperationGuard::begin(
        d,
        "auto-test",
        "market-order.swap",
        &crate::trade::mutation_payload(&MarketCommand::Swap(a.clone())).unwrap(),
        "synthetic-okx-client",
    )
    .unwrap()
}
fn setup(
    before: u64,
    mode: &'static str,
) -> (
    tempfile::TempDir,
    SwapArgs,
    Okx,
    Arc<World>,
    Server,
    Server,
    Server,
    Client,
) {
    let (d, mut a, mut api) = fixture();
    a.auto_approve = true;
    let w = world(before, mode);
    let rpc = rpc(w.clone());
    let okx = provider(w.clone());
    api.profile.rpc_url = Some(rpc.origin.clone());
    api.origin = okx.origin.clone();
    if matches!(mode, "missing" | "one-confirmation") {
        api.profile.approval_wait_ms = 30;
    }
    let (binance, client) = backend(w.clone(), binance_approval_preview());
    (d, a, api, w, rpc, okx, binance, client)
}
fn execute_counts(backend: &Server) -> (usize, usize) {
    let seen = backend.seen.lock().unwrap();
    let executes: Vec<_> = seen
        .iter()
        .filter(|s| s.lines().next().unwrap().contains("/execute "))
        .collect();
    let approval = executes
        .iter()
        .filter(|s| s.contains("auto-approve-preview"))
        .count();
    (approval, executes.len() - approval)
}

#[test]
fn real_binance_allowance_schema_preserves_uint256_precision_and_rejects_nfts() {
    let mut v = binance_approval_preview();
    let amount = BigUint::from(1_000_000_000_000_000_000u64);
    v["simulationResult"]["allowanceChanges"][0]["postAmount"] =
        json!(1_000_000_000_000_000_000u64);
    assert!(approval::validate_preview(&v, FROM, INPUT, SPENDER, &amount).is_ok());
    for (field, value) in [
        ("postAmount", json!(1_000_000_000_000_000_001u64)),
        ("tokenType", json!("Erc721")),
        ("tokenId", json!("1")),
        ("postAmount", json!("1e18")),
        ("postAmount", json!(-1)),
        (
            "postAmount",
            json!("115792089237316195423570985008687907853269984665640564039457584007913129639936"),
        ),
        ("newAllowance", json!("1")),
        ("allowanceBefore", json!("1")),
    ] {
        let mut bad = v.clone();
        bad["simulationResult"]["allowanceChanges"][0][field] = value;
        assert!(
            approval::validate_preview(&bad, FROM, INPUT, SPENDER, &amount).is_err(),
            "{field}"
        );
    }
    let mut consumption = v["simulationResult"]["allowanceChanges"].clone();
    consumption[0]["preAmount"] = json!(1_000_000_000_000_000_000u64);
    consumption[0]["postAmount"] = json!(0);
    assert!(approval::validate_consumption(&consumption, FROM, INPUT, SPENDER, &amount).is_ok());
    for (field, value) in [
        ("postAmount", json!(1_000_000_000_000_000_001u64)),
        ("tokenId", json!("1")),
        ("tokenType", json!("Erc1155")),
        ("allowanceAfter", json!("1")),
        ("preAmount", json!("1.5")),
    ] {
        let mut bad = consumption.clone();
        bad[0][field] = value;
        assert!(
            approval::validate_consumption(&bad, FROM, INPUT, SPENDER, &amount).is_err(),
            "{field}"
        );
    }
}

#[test]
fn approval_preview_only_permits_the_exact_grant() {
    let good = approval_preview();
    let amount = BigUint::from(AMOUNT);
    assert!(approval::validate_preview(&good, FROM, INPUT, SPENDER, &amount).is_ok());
    for (p, value) in [
        (
            "/simulationResult/allowanceChanges/0/spender",
            json!(ROUTER),
        ),
        ("/simulationResult/allowanceChanges/0/owner", json!(ROUTER)),
        (
            "/simulationResult/allowanceChanges/0/contractAddress",
            json!(OUTPUT),
        ),
        (
            "/simulationResult/allowanceChanges/0/amount",
            json!((AMOUNT + 1).to_string()),
        ),
        (
            "/simulationResult/authorityChanges",
            json!([{"spender":SPENDER}]),
        ),
        ("/risks/riskDetails", json!([{"code":"RISK"}])),
        ("/simulationResult/balanceChanges", json!([{"change":"-1"}])),
    ] {
        let mut v = good.clone();
        *v.pointer_mut(p).unwrap() = value;
        assert!(
            approval::validate_preview(&v, FROM, INPUT, SPENDER, &amount).is_err(),
            "{p}"
        );
    }
    let mut v = good;
    v["simulationResult"]["allowanceChanges"][0]["unknownGrant"] = json!(true);
    assert!(approval::validate_preview(&v, FROM, INPUT, SPENDER, &amount).is_err());
}
#[tokio::test]
async fn exact_approval_confirms_rechecks_and_then_fetches_fresh_swap() {
    let (d, a, api, w, _rpc, _okx, backend, mut c) = setup(0, "success");
    let mut g = guard(d.path(), &a);
    let result = swap(&mut c, d.path(), &a, &api, Some(&mut g)).await;
    g.finish(&result).unwrap();
    let out = result.unwrap();
    assert_eq!(out["status"], "BROADCASTED");
    assert_eq!(out["autoApproval"]["amount"], AMOUNT.to_string());
    assert_eq!(out["autoApproval"]["unlimited"], false);
    assert_eq!(out["autoApproval"]["confirmed"], true);
    assert_eq!(out["autoApproval"]["confirmations"], 2);
    assert_eq!(execute_counts(&backend), (1, 1));
    let phases = w.phases.lock().unwrap();
    assert_eq!(
        *phases,
        vec![
            "pre_quote",
            "approval_execute",
            "receipt",
            "allowance_after",
            "swap_quote",
            "swap_execute"
        ]
    );
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(d.path().join(".baw-rs-operations/auto-test.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["progress"]["stage"], "swap_broadcast");
    assert_eq!(record["progress"]["approvalTxHash"], approval_hash());
    assert!(record["progress"]["swapTxHash"].is_string());
    drop(g);
    assert!(OperationGuard::begin(
        d.path(),
        "auto-test",
        "market-order.swap",
        &json!({}),
        "synthetic-okx-client"
    )
    .is_err());
}
#[tokio::test]
async fn sufficient_allowance_skips_approval_and_pre_quote() {
    let (d, a, api, w, _rpc, _okx, backend, mut c) = setup(AMOUNT + 100, "success");
    let mut g = guard(d.path(), &a);
    let out = swap(&mut c, d.path(), &a, &api, Some(&mut g))
        .await
        .unwrap();
    assert_eq!(out["autoApproval"]["needed"], false);
    assert_eq!(execute_counts(&backend), (0, 1));
    assert_eq!(
        *w.phases.lock().unwrap(),
        vec!["swap_quote", "swap_execute"]
    );
}
#[tokio::test]
async fn insufficient_nonzero_allowance_is_set_to_exact_amount_not_added() {
    let (d, a, api, _w, _rpc, _okx, backend, mut c) = setup(100, "success");
    let mut g = guard(d.path(), &a);
    let out = swap(&mut c, d.path(), &a, &api, Some(&mut g))
        .await
        .unwrap();
    assert_eq!(out["autoApproval"]["allowanceBefore"], "100");
    assert_eq!(out["autoApproval"]["allowanceAfter"], AMOUNT.to_string());
    assert_eq!(execute_counts(&backend), (1, 1));
}
#[tokio::test]
async fn uncertain_reverted_or_mismatching_approval_never_submits_a_swap() {
    for mode in [
        "revert",
        "missing",
        "one-confirmation",
        "mismatch",
        "unchanged",
        "reorg",
        "bad-to",
        "execute-error",
        "no-hash",
    ] {
        let (d, a, api, _w, _rpc, okx, backend, mut c) = setup(0, mode);
        let mut g = guard(d.path(), &a);
        let result = swap(&mut c, d.path(), &a, &api, Some(&mut g)).await;
        g.finish(&result).unwrap();
        let out = result.unwrap();
        assert_eq!(out["swapSubmitted"], false, "{mode}");
        assert_eq!(out["retrySafe"], false);
        assert_eq!(execute_counts(&backend), (1, 0), "{mode}");
        assert!(
            !okx.seen
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.lines().next().unwrap().contains("/swap?")),
            "{mode}"
        );
        let record: Value = serde_json::from_slice(
            &std::fs::read(d.path().join(".baw-rs-operations/auto-test.json")).unwrap(),
        )
        .unwrap();
        if !matches!(mode, "execute-error" | "no-hash") {
            assert_eq!(record["progress"]["approvalTxHash"], approval_hash());
        }
    }
}
#[tokio::test]
async fn invalid_chain_or_allowance_fails_before_spending_gas() {
    for mode in ["wrong-chain", "malformed"] {
        let (d, a, api, _w, _rpc, _okx, backend, mut c) = setup(0, mode);
        let mut g = guard(d.path(), &a);
        assert!(swap(&mut c, d.path(), &a, &api, Some(&mut g))
            .await
            .is_err());
        assert_eq!(execute_counts(&backend), (0, 0));
    }
}
#[tokio::test]
async fn approval_requires_rpc_and_a_parent_journal() {
    let (d, a, mut api, _w, _rpc, _okx, backend, mut c) = setup(0, "success");
    assert!(swap(&mut c, d.path(), &a, &api, None).await.is_err());
    assert_eq!(execute_counts(&backend), (0, 0));
    api.profile.rpc_url = None;
    let mut g = guard(d.path(), &a);
    assert!(swap(&mut c, d.path(), &a, &api, Some(&mut g))
        .await
        .is_err());
    assert_eq!(execute_counts(&backend), (0, 0));
}
#[tokio::test]
async fn native_input_never_calls_rpc_or_approves() {
    let (d, mut a, api, _w, rpc, okx, backend, mut c) = setup(0, "success");
    a.quote.from_token = NATIVE.into();
    a.quote.from_token_decimals = Some(18);
    let route = api.route_config(&a.quote, FROM).unwrap();
    let outcome = approval::prepare(&mut c, d.path(), &a, &api, FROM, &route, None)
        .await
        .unwrap();
    let approval::Outcome::Continue(v) = outcome else {
        panic!()
    };
    assert_eq!(v["needed"], false);
    assert!(rpc.seen.lock().unwrap().is_empty());
    assert!(okx.seen.lock().unwrap().is_empty());
    assert_eq!(execute_counts(&backend), (0, 0));
}
#[tokio::test]
async fn failed_fresh_quote_keeps_confirmed_approval_in_the_result_and_journal() {
    let (d, a, api, _w, _rpc, _okx, backend, mut c) = setup(0, "bad-fresh-quote");
    let mut g = guard(d.path(), &a);
    let result = swap(&mut c, d.path(), &a, &api, Some(&mut g)).await;
    g.finish(&result).unwrap();
    let out = result.unwrap();
    assert_eq!(out["status"], "SWAP_NOT_SUBMITTED");
    assert_eq!(out["autoApproval"]["confirmed"], true);
    assert_eq!(out["swapSubmissionAttempted"], false);
    assert_eq!(execute_counts(&backend), (1, 0));
    let record: Value = serde_json::from_slice(
        &std::fs::read(d.path().join(".baw-rs-operations/auto-test.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["progress"]["stage"], "approval_confirmed");
    assert_eq!(record["progress"]["approvalTxHash"], approval_hash());
}
#[tokio::test]
async fn dangerous_approval_preview_never_executes_or_resets_to_zero() {
    let (d, a, api, w, _rpc, _okx, _backend, _c) = setup(100, "success");
    let mut p = approval_preview();
    p["simulationResult"]["allowanceChanges"][0]["amount"] = json!((AMOUNT + 1).to_string());
    let (backend, mut c) = backend(w, p);
    let mut g = guard(d.path(), &a);
    assert!(swap(&mut c, d.path(), &a, &api, Some(&mut g))
        .await
        .is_err());
    assert_eq!(execute_counts(&backend), (0, 0));
}

#[test]
fn swap_allows_only_bounded_consumption_not_new_grants() {
    let amount = BigUint::from(AMOUNT);
    let good = json!([{"owner":FROM,"contractAddress":INPUT,"spender":SPENDER,
        "allowanceBefore":AMOUNT.to_string(),"allowanceAfter":"0"}]);
    assert!(approval::validate_consumption(&good, FROM, INPUT, SPENDER, &amount).is_ok());
    for (field, value) in [
        ("spender", json!(ROUTER)),
        ("contractAddress", json!(OUTPUT)),
        ("allowanceAfter", json!((AMOUNT + 1).to_string())),
        ("allowanceBefore", json!((AMOUNT + 1).to_string())),
    ] {
        let mut rows = good.clone();
        rows[0][field] = value;
        assert!(
            approval::validate_consumption(&rows, FROM, INPUT, SPENDER, &amount).is_err(),
            "{field}"
        );
    }
}

#[test]
fn rpc_configuration_refuses_credentials_in_userinfo_and_nonsecure_urls() {
    for bad in [
        "http://example.com",
        "https://user:pass@example.com",
        "file:///tmp/rpc",
        "https://rpc.example/#fragment",
    ] {
        assert!(validate_rpc_url(bad).is_err());
    }
    assert!(validate_rpc_url("https://rpc.example/v2/private-api-key").is_ok());
}
