use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
const FROM: &str = "0x1111111111111111111111111111111111111111";
fn args() -> ContractPreview {
    ContractPreview {
        binance_chain_id: "1".into(),
        from: FROM.into(),
        to: Some("0x2222222222222222222222222222222222222222".into()),
        value: Some("0".into()),
        input_data: Some("0x".into()),
        unsigned_tx: None,
        gas_limit: None,
    }
}
fn fixture(
    replies: Vec<Value>,
) -> (
    crate::client::Client,
    Arc<Mutex<Vec<String>>>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let handle = std::thread::spawn(move || {
        for value in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if bytes.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            seen.lock().unwrap().push(String::from_utf8(bytes).unwrap());
            if let Some(ms) = value["__delay_ms"].as_u64() {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            let body = if value.get("code").is_some() {
                value
            } else {
                json!({"code":"000000","data":value})
            }
            .to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
    });
    let credentials = crate::session::Credentials {
        client_id: zeroize::Zeroizing::new("synthetic-signing-client".into()),
        session_id: zeroize::Zeroizing::new("synthetic-session".into()),
    };
    (
        crate::client::Client::test_client(credentials, &origin).unwrap(),
        requests,
        handle,
    )
}
#[tokio::test]
async fn fire_keeps_preview_in_memory_and_preserves_nested_authority() {
    let dir = tempwallet();
    let mut p = preview();
    p["simulationResult"] =
        json!({"success":true,"authorityChanges":["nested"],"allowanceChanges":["allow"]});
    let mut replies = gates();
    replies.push(p);
    replies.extend(gates());
    replies.push(json!({"orderId":"done"}));
    let (mut client, _, thread) = fixture(replies);
    let a = args();
    let result = fire_contract(
        &mut client,
        dir.path(),
        &ContractFire {
            binance_chain_id: a.binance_chain_id,
            from: a.from,
            to: a.to.unwrap(),
            value: a.value,
            input_data: a.input_data,
            gas_limit: None,
            policy: None,
        },
    )
    .await
    .unwrap();
    thread.join().unwrap();
    let files: Vec<_> = std::fs::read_dir(dir.path().join("signing-previews"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    assert!(
        files.iter().all(|p| p.extension().unwrap() != "json"),
        "fire must never store preview JSON"
    );
    assert_eq!(
        result["preview"]["simulationResult"]["authorityChanges"],
        json!(["nested"])
    );
    assert!(
        result["timings"]["preview"]["request_ms"]
            .as_f64()
            .unwrap_or(0.0)
            > 0.0
    );
    assert!(result["timings"]["execute"]["body_ms"].is_number());
    assert!(result["timings"]["validation_ms"].is_number());
}
#[tokio::test]
async fn fast_fire_requires_simulation_and_backend_sender_binding() {
    for (p, expected) in [
        (json!({"requestId":"x","simulationSuccess":true}), "binding"),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1","success":true}),
            "simulation",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1"}),
            "simulation",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1","simulationSuccess":true,"simulationResult":{"expiresAt":1}}),
            "expired",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1","simulationResult":{"success":false}}),
            "failed",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"56","simulationSuccess":true}),
            "chain",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1","simulationResult":{"success":"true"}}),
            "simulation",
        ),
        (
            json!({"requestId":"x","from":FROM,"binanceChainId":"1","simulationSuccess":true,"simulationResult":{"from":"0x2222222222222222222222222222222222222222"}}),
            "sender",
        ),
    ] {
        let dir = tempwallet();
        let (mut client, _, thread) = fixture(vec![p]);
        let a = args();
        let err = memory_fire(
            &mut client,
            dir.path(),
            &ContractFire {
                binance_chain_id: a.binance_chain_id,
                from: a.from,
                to: a.to.unwrap(),
                value: a.value,
                input_data: a.input_data,
                gas_limit: None,
                policy: None,
            },
            true,
            &mut FireTimings::default(),
        )
        .await
        .unwrap_err();
        thread.join().unwrap();
        assert!(err.contains(expected), "expected {expected}, got {err}");
    }
}
#[tokio::test]
async fn dispatch_precheck_policy_is_request_local() {
    use clap::Parser;
    let dir = tempwallet();
    let cli = crate::cli::Cli::try_parse_from([
        "baw-rs",
        "--wallet-dir",
        dir.path().to_str().unwrap(),
        "--instance-id",
        "synthetic",
        "--skip-prechecks",
        "contract-call",
        "preview",
        "--binanceChainId",
        "1",
        "--from",
        FROM,
        "--to",
        "0x2222222222222222222222222222222222222222",
    ])
    .unwrap();
    let (mut client, seen, thread) = fixture(vec![preview()]);
    let result = crate::dispatch::execute(&mut client, &cli).await;
    thread.join().unwrap();
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(seen.lock().unwrap().len(), 1);
}
fn gates() -> Vec<Value> {
    vec![
        json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
        json!([{"subWalletList":[{"addresses":[{"binanceChainId":"1","address":FROM}]}]}]),
        json!([]),
    ]
}
fn preview() -> Value {
    json!({"requestId":"../unsafe/id","expiresAt":4102444800u64,"simulationSuccess":true,"risks":[{"level":"HIGH","detail":"review"}],"authorityChanges":[{"spender":"retain"}]})
}
#[tokio::test]
async fn warm_probe_never_crosses_wallet_clients() {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(v) = &self.0 {
                std::env::set_var("BAW_RS_WARM", v);
            } else {
                std::env::remove_var("BAW_RS_WARM");
            }
        }
    }
    let _restore = Restore(std::env::var("BAW_RS_WARM").ok());
    std::env::set_var("BAW_RS_WARM", "1");
    let (mut a, _, ta) = fixture(gates());
    let mut other = gates();
    let expected = "0x2222222222222222222222222222222222222222";
    other[1][0]["subWalletList"][0]["addresses"][0]["address"] = json!(expected);
    let (mut b, _, tb) = fixture(other);
    assert_eq!(check_wallet(&mut a, "1", None).await.unwrap().0, FROM);
    assert_eq!(check_wallet(&mut b, "1", None).await.unwrap().0, expected);
    ta.join().unwrap();
    tb.join().unwrap();
}
#[tokio::test]
async fn contract_preview_preserves_risks_binds_cache_and_executes_exact_review() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let mut replies = gates();
    replies.push(preview());
    replies.extend(gates());
    replies.push(json!({"orderId":"synthetic-order"}));
    let (mut client, seen, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(result["risks"], preview()["risks"]);
    assert_eq!(result["authorityChanges"], preview()["authorityChanges"]);
    let digest = result["previewSha256"].as_str().unwrap();
    assert_eq!(digest.len(), 64);
    let executed = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Execute {
            request_id: "../unsafe/id".into(),
        },
        Some(digest),
    )
    .await
    .unwrap();
    assert_eq!(executed, json!({"orderId":"synthetic-order"}));
    thread.join().unwrap();
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 8);
    assert!(requests[3].contains("/contract-call/preview "));
    assert!(requests[7].ends_with("{\"requestId\":\"../unsafe/id\"}"));
    use std::os::unix::fs::PermissionsExt;
    let cached = std::fs::read_dir(dir.path().join("signing-previews"))
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect::<Vec<_>>();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].file_name().to_str().unwrap().len(), 69);
    assert_eq!(
        cached[0].metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
}
#[test]
fn fire_is_a_mutation_but_needs_no_accepted_digest() {
    let fire = ContractCommand::Fire(ContractFire {
        binance_chain_id: "1".into(),
        from: FROM.into(),
        to: "0x2222222222222222222222222222222222222222".into(),
        value: None,
        input_data: Some("0x".into()),
        gas_limit: None,
        policy: None,
    });
    assert!(is_contract_mutating(&fire), "fire must require consent");
    assert!(
        !needs_accepted_digest(&fire),
        "fire has no reviewed preview to pin"
    );
}

#[tokio::test]
async fn skip_prechecks_makes_no_reads_and_trusts_from() {
    let dir = tempwallet();
    let (mut client, seen, thread) = fixture(vec![preview()]);
    let result = request_precheck_policy(
        true,
        contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(ContractPreview {
                binance_chain_id: "1".into(),
                from: FROM.into(),
                to: Some("0x2222222222222222222222222222222222222222".into()),
                value: None,
                input_data: Some("0x".into()),
                unsigned_tx: None,
                gas_limit: None,
            }),
            None,
        ),
    )
    .await;
    thread.join().unwrap();
    assert!(result.is_ok(), "{result:?}");
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 1, "no settings read, no address read");
    assert!(requests[0].contains("/contract-call/preview "));
}

#[tokio::test]
async fn skip_prechecks_still_requires_an_explicit_sender() {
    let dir = tempwallet();
    let (mut client, seen, thread) = fixture(vec![]);
    let result = request_precheck_policy(
        true,
        contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(ContractPreview {
                binance_chain_id: "1".into(),
                from: String::new(),
                to: Some("0x2222222222222222222222222222222222222222".into()),
                value: None,
                input_data: Some("0x".into()),
                unsigned_tx: None,
                gas_limit: None,
            }),
            None,
        ),
    )
    .await;
    thread.join().unwrap();
    assert!(result.is_err(), "empty --from must fail before any network");
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn fire_previews_and_executes_in_one_call_without_a_policy() {
    let dir = tempwallet();
    let mut replies = gates();
    replies.push(preview());
    replies.extend(gates());
    replies.push(json!({"orderId":"synthetic-fire-order"}));
    let (mut client, seen, thread) = fixture(replies);
    let out = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Fire(ContractFire {
            binance_chain_id: "1".into(),
            from: FROM.into(),
            to: "0x2222222222222222222222222222222222222222".into(),
            value: Some("0".into()),
            input_data: Some("0x".into()),
            gas_limit: None,
            policy: None,
        }),
        None,
    )
    .await
    .unwrap();
    thread.join().unwrap();
    assert_eq!(out["execution"], json!({"orderId":"synthetic-fire-order"}));
    // The risks the caller was never shown are still returned, so they can be logged.
    assert_eq!(out["preview"]["risks"], preview()["risks"]);
    assert_eq!(
        out["preview"]["authorityChanges"],
        preview()["authorityChanges"]
    );
    assert_eq!(out["preview"]["previewSha256"].as_str().unwrap().len(), 64);
    let requests = seen.lock().unwrap();
    assert_eq!(
        requests.len(),
        8,
        "strict default: gates, preview, gates again, execute"
    );
    assert!(requests[3].contains("/contract-call/preview "));
    assert!(requests[7].contains("/contract-call/execute "));
}

#[tokio::test]
async fn accepted_preview_is_consumed_even_when_wallet_revalidation_fails() {
    let dir = tempwallet();
    let mut replies = gates();
    replies.push(preview());
    replies.push(json!({"devMode":{"enabled":false,"expiresAt":4102444800u64}}));
    let (mut client, seen, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    let cmd = ContractCommand::Execute {
        request_id: "../unsafe/id".into(),
    };
    assert!(contract(
        &mut client,
        dir.path(),
        &cmd,
        result["previewSha256"].as_str()
    )
    .await
    .unwrap_err()
    .contains("disabled"));
    thread.join().unwrap();
    let error = contract(
        &mut client,
        dir.path(),
        &cmd,
        result["previewSha256"].as_str(),
    )
    .await
    .unwrap_err();
    assert!(error.contains("consumed"), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 5);
}
#[tokio::test]
async fn preview_without_server_expiry_uses_bounded_local_ttl() {
    let dir = tempwallet();
    let mut replies = gates();
    let mut response = preview();
    response.as_object_mut().unwrap().remove("expiresAt");
    replies.push(response.clone());
    let (mut client, _, thread) = fixture(replies);
    let before = now().unwrap();
    let out = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    thread.join().unwrap();
    assert_eq!(out["risks"], response["risks"]);
    let cached = Cache::open(dir.path())
        .unwrap()
        .read("../unsafe/id")
        .unwrap();
    assert!(cached.expires_at >= before + PREVIEW_TTL);
    assert!(cached.expires_at <= now().unwrap() + PREVIEW_TTL);
    assert_eq!(out["previewSha256"], cached.digest().unwrap());
}
#[tokio::test]
async fn preview_expiring_during_revalidation_never_executes() {
    let dir = tempwallet();
    let cache = Cache::open(dir.path()).unwrap();
    let record = Cached {
        version: 1,
        kind: "contract-call".into(),
        client_binding: sha(b"synthetic-signing-client"),
        wallet_device: cache.wallet_device,
        wallet_inode: cache.wallet_inode,
        chain: "1".into(),
        sender: FROM.into(),
        request_id: "expires-during-check".into(),
        expires_at: now().unwrap() + 2,
        input: contract_payload(&args()).unwrap(),
        output: preview(),
    };
    cache.store(&record).unwrap();
    let digest = record.digest().unwrap();
    let mut replies = gates();
    replies[0]["__delay_ms"] = json!(2200);
    let (mut client, seen, thread) = fixture(replies);
    let error = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Execute {
            request_id: "expires-during-check".into(),
        },
        Some(&digest),
    )
    .await
    .unwrap_err();
    thread.join().unwrap();
    assert!(error.contains("expired"), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 3);
    assert!(cache.check_unconsumed("expires-during-check").is_err());
}
#[tokio::test]
async fn cached_preview_digest_wallet_action_chain_and_expiry_fail_closed() {
    for case in ["digest", "client", "directory", "action", "chain", "expiry"] {
        let dir = tempwallet();
        let cache = Cache::open(dir.path()).unwrap();
        let mut output = preview();
        output["binanceChainId"] = json!("1");
        let mut record = Cached {
            version: 1,
            kind: "contract-call".into(),
            client_binding: sha(b"synthetic-signing-client"),
            wallet_device: cache.wallet_device,
            wallet_inode: cache.wallet_inode,
            chain: "1".into(),
            sender: FROM.into(),
            request_id: case.into(),
            expires_at: now().unwrap() + 300,
            input: contract_payload(&args()).unwrap(),
            output,
        };
        match case {
            "client" => record.client_binding = sha(b"another-client"),
            "directory" => record.wallet_inode += 1,
            "action" => record.kind = "sign-message".into(),
            "chain" => record.chain = "56".into(),
            "expiry" => record.expires_at = 1,
            _ => {}
        }
        cache.store(&record).unwrap();
        let digest = if case == "digest" {
            "0".repeat(64)
        } else {
            record.digest().unwrap()
        };
        let (mut client, seen, thread) = fixture(vec![]);
        thread.join().unwrap();
        let error = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Execute {
                request_id: case.into(),
            },
            Some(&digest),
        )
        .await
        .unwrap_err();
        assert!(!error.starts_with("HTTP"), "{case}: {error}");
        assert!(seen.lock().unwrap().is_empty());
    }
}
fn tempwallet() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(
        d.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    d
}
#[tokio::test]
async fn failed_execute_consumes_preview_and_never_posts_twice() {
    let dir = tempwallet();
    let mut replies = gates();
    replies.push(preview());
    replies.extend(gates());
    replies.push(json!({"code":"123456","message":"synthetic-private-error"}));
    let (mut client, seen, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    let digest = result["previewSha256"].as_str().unwrap();
    let cmd = ContractCommand::Execute {
        request_id: "../unsafe/id".into(),
    };
    assert!(contract(&mut client, dir.path(), &cmd, Some(digest))
        .await
        .is_err());
    thread.join().unwrap();
    let error = contract(&mut client, dir.path(), &cmd, Some(digest))
        .await
        .unwrap_err();
    assert!(error.contains("consumed"), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 8);
}
fn typed(chain: Value, signer: &str) -> Value {
    json!({"binanceChainId":"1","signType":"EIP712","message":json!({"jsonrpc":"2.0","method":"eth_signTypedData_v4","params":[signer,json!({"domain":{"chainId":chain},"types":{},"primaryType":"Permit","message":{}}).to_string()],"id":1}).to_string()})
}
#[tokio::test]
async fn message_preview_forwards_original_wrapper_and_executes_reviewed_authority_changes() {
    let dir = tempwallet();
    let input = typed(json!(1), FROM);
    let original = format!("  {}  ", input["message"].as_str().unwrap());
    let command = SignCommand::Preview(MessagePreview {
        binance_chain_id: "1".into(),
        message: original.clone(),
        sign_type: "EIP712".into(),
    });
    assert!(!is_message_mutating(&command));
    assert_eq!(message_intent(&command)["message"], original);
    let mut replies = gates();
    replies.push(preview());
    replies.extend(gates());
    replies.push(json!({"orderId":"msg-order"}));
    let (mut client, seen, thread) = fixture(replies);
    let result = message(&mut client, dir.path(), &command, None)
        .await
        .unwrap();
    let execute = SignCommand::Execute {
        request_id: "../unsafe/id".into(),
    };
    assert!(is_message_mutating(&execute));
    assert_eq!(
        message(
            &mut client,
            dir.path(),
            &execute,
            result["previewSha256"].as_str()
        )
        .await
        .unwrap()["orderId"],
        "msg-order"
    );
    thread.join().unwrap();
    let requests = seen.lock().unwrap();
    let body: Value = serde_json::from_str(requests[3].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["message"], original);
    assert_eq!(result["authorityChanges"], preview()["authorityChanges"]);
}
#[tokio::test]
async fn result_and_history_preserve_data_and_encode_original_query_names() {
    let dir = tempwallet();
    let (mut client, seen, thread) = fixture(vec![
        json!({"signature":"SYNTHETIC-NOT-A-SIGNATURE","risks":[1]}),
        json!({"rows":[],"nextToken":"next","authorityChanges":[1]}),
    ]);
    let result = message(
        &mut client,
        dir.path(),
        &SignCommand::Result {
            order_id: "order-test".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(result["signature"], "SYNTHETIC-NOT-A-SIGNATURE");
    let history = SignCommand::History(MessageHistory {
        binance_chain_id: Some("1,56".into()),
        limit: 7,
        next_token: Some("a&b=+".into()),
        start_time: Some(100),
        end_time: Some(200),
        sort_type: Some("DESC".into()),
    });
    assert!(!is_message_mutating(&history));
    let result = message(&mut client, dir.path(), &history, None)
        .await
        .unwrap();
    assert_eq!(result["authorityChanges"], json!([1]));
    thread.join().unwrap();
    let requests = seen.lock().unwrap();
    assert!(requests[0].ends_with("{\"orderId\":\"order-test\"}"));
    let line = requests[1].lines().next().unwrap();
    assert!(line.starts_with(&format!("GET {API}/sign-message/history?")));
    let url = url::Url::parse(&format!(
        "http://localhost{}",
        line.split_whitespace().nth(1).unwrap()
    ))
    .unwrap();
    let q = url
        .query_pairs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(q["binanceChainIds"], "1,56");
    assert_eq!(q["nextToken"], "a&b=+");
    assert_eq!(q["limit"], "7");
    assert_eq!(q.len(), 6);
}
#[tokio::test]
async fn risk_codes_are_preserved_not_mistaken_for_backend_failure() {
    let dir = tempwallet();
    let mut p = preview();
    p["risks"] = json!([{"code":"UNLIMITED_APPROVAL","status":"WARNING"}]);
    p["authorityChanges"] = json!([{"code":"GRANT_ROLE","spender":"synthetic"}]);
    let mut replies = gates();
    replies.push(p.clone());
    let (mut client, _, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(result["authorityChanges"], p["authorityChanges"]);
    thread.join().unwrap();
}
#[tokio::test]
async fn execute_inner_backend_failure_is_error_and_request_stays_consumed() {
    let dir = tempwallet();
    let mut replies = gates();
    replies.push(preview());
    replies.extend(gates());
    replies.push(json!({"code":"EXECUTION_FAILED","message":"synthetic-private-detail"}));
    // Explicit success envelope, but failed data.code must not become fake command success.
    let last = replies.len() - 1;
    replies[last] = json!({"code":"000000","data":{"code":"EXECUTION_FAILED","message":"synthetic-private-detail"}});
    let (mut client, seen, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    let command = ContractCommand::Execute {
        request_id: "../unsafe/id".into(),
    };
    let error = contract(
        &mut client,
        dir.path(),
        &command,
        result["previewSha256"].as_str(),
    )
    .await
    .unwrap_err();
    assert!(!error.contains("private-detail"));
    thread.join().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 8);
}
#[tokio::test]
async fn cache_unknown_field_tampering_is_rejected_before_network() {
    let dir = tempwallet();
    let mut replies = gates();
    replies.push(preview());
    let (mut client, _, thread) = fixture(replies);
    let result = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Preview(args()),
        None,
    )
    .await
    .unwrap();
    thread.join().unwrap();
    let path = dir
        .path()
        .join("signing-previews")
        .join(Cache::name("../unsafe/id").to_str().unwrap());
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v["unexpected"] = json!(true);
    std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    let error = contract(
        &mut client,
        dir.path(),
        &ContractCommand::Execute {
            request_id: "../unsafe/id".into(),
        },
        result["previewSha256"].as_str(),
    )
    .await
    .unwrap_err();
    assert!(error.contains("malformed"), "{error}");
}
#[test]
fn typed_wrapper_rejects_wrong_signer_domain_and_method() {
    assert!(validate_message(&typed(json!("0x1"), FROM), FROM).is_ok());
    assert!(validate_message(&typed(json!(56), FROM), FROM).is_err());
    assert!(validate_message(
        &typed(json!(1), "0x2222222222222222222222222222222222222222"),
        FROM
    )
    .is_err());
    let mut v = typed(json!(1), FROM);
    v["message"] = json!("{\"method\":\"personal_sign\"}");
    assert!(validate_message(&v, FROM).is_err());
}
#[test]
fn evm_input_validation_rejects_bare_hex_and_bad_gas() {
    let mut args = ContractPreview {
        binance_chain_id: "1".into(),
        from: "0x1111111111111111111111111111111111111111".into(),
        to: Some("0x2222222222222222222222222222222222222222".into()),
        value: Some("0x".into()),
        input_data: Some("0x12".into()),
        unsigned_tx: None,
        gas_limit: None,
    };
    assert!(contract_payload(&args).is_err());
    args.value = Some("0x00".into());
    assert!(contract_payload(&args).is_ok());
    args.input_data = Some("0x1".into());
    assert!(contract_payload(&args).is_err());
    args.input_data = Some("0x".into());
    args.gas_limit = Some(20999);
    assert!(contract_payload(&args).is_err());
    args.gas_limit = Some(15000001);
    assert!(contract_payload(&args).is_err());
    args.gas_limit = Some(21000);
    assert_eq!(contract_payload(&args).unwrap()["gasLimit"], json!(21000));
}
