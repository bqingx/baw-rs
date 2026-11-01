use super::super::native_events::Control;
use super::super::native_events_tests::config;
use super::*;
use futures_util::{SinkExt, StreamExt};
struct SourceFixture {
    rpc_url: String,
    ws_url: String,
    events: mpsc::Sender<Value>,
    rpc_requests: Arc<std::sync::Mutex<Vec<Value>>>,
    rpc_delay_ms: Arc<std::sync::atomic::AtomicU64>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for SourceFixture {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}
impl SourceFixture {
    async fn new() -> Self {
        Self::protocol("logs", BTreeMap::new(), false, "0x38").await
    }
    async fn protocol(
        mode: &str,
        transactions: BTreeMap<String, Value>,
        reject: bool,
        ws_chain: &str,
    ) -> Self {
        let mode = mode.to_owned();
        let ws_chain = ws_chain.to_owned();
        let transactions = Arc::new(transactions);
        let rpc_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = rpc_requests.clone();
        let rpc_delay_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let delay = rpc_delay_ms.clone();
        let rpc = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let rpc_url = format!("http://{}", rpc.local_addr().unwrap());
        let ws_url = format!("ws://{}", ws.local_addr().unwrap());
        let (events, mut rx) = mpsc::channel::<Value>(32);
        let wt = tokio::spawn(async move {
            let (stream, _) = ws.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            if mode != "logs" {
                let req: Value =
                    serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap())
                        .unwrap();
                assert_eq!(req["method"], "eth_chainId");
                ws.send(tokio_tungstenite::tungstenite::Message::Text(
                    json!({"jsonrpc":"2.0","id":req["id"],"result":ws_chain})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
                if ws_chain != "0x38" {
                    return;
                }
            }
            let req: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(req["method"], "eth_subscribe");
            let params = match mode.as_str() {
                "logs" => json!(["logs", config()["filter"]]),
                "hashes" => json!(["newPendingTransactions"]),
                "full" => json!(["newPendingTransactions", true]),
                _ => unreachable!(),
            };
            assert_eq!(req["params"], params);
            if reject {
                ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"unsupported pending subscription"}}).to_string().into())).await.unwrap();
                return;
            }
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                json!({"jsonrpc":"2.0","id":1,"result":"synthetic-sub"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            while let Some(event) = rx.recv().await {
                if event.get("__close") == Some(&json!(true)) {
                    let _ = ws.close(None).await;
                    break;
                }
                let text = event.get("__raw").map(|v|v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string()))
                    .unwrap_or_else(||json!({"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"synthetic-sub","result":event}}).to_string());
                if ws
                    .send(tokio_tungstenite::tungstenite::Message::Text(text.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let rt = tokio::spawn(async move {
            while let Ok((mut s, _)) = rpc.accept().await {
                let transactions = transactions.clone();
                let captured = captured.clone();
                let delay = delay.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut buf = [0; 4096];
                    let req: Value = loop {
                        let n = s.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let h = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                            let len = h
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length: "))
                                .unwrap()
                                .parse::<usize>()
                                .unwrap();
                            if bytes.len() >= i + 4 + len {
                                break serde_json::from_slice(&bytes[i + 4..i + 4 + len]).unwrap();
                            }
                        }
                    };
                    captured.lock().unwrap().push(req.clone());
                    tokio::time::sleep(Duration::from_millis(
                        delay.load(std::sync::atomic::Ordering::SeqCst),
                    ))
                    .await;
                    let result = match req["method"].as_str().unwrap() {
                        "eth_getTransactionByHash" => transactions
                            .get(req["params"][0].as_str().unwrap())
                            .cloned()
                            .unwrap_or(Value::Null),
                        "eth_chainId" => transactions
                            .get("__chain")
                            .cloned()
                            .unwrap_or(json!("0x38")),
                        "eth_blockNumber" => json!("0x1"),
                        "eth_getLogs" => json!([]),
                        "eth_call" => json!(format!("0x{:064x}", 9)),
                        _ => panic!("unexpected RPC {req}"),
                    };
                    let b = json!({"jsonrpc":"2.0","id":req["id"],"result":result}).to_string();
                    let _=s.write_all(format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{b}",b.len()).as_bytes()).await;
                });
            }
        });
        Self {
            rpc_url,
            ws_url,
            events,
            rpc_requests,
            rpc_delay_ms,
            tasks: vec![wt, rt],
        }
    }
}
async fn backend_until(backend: &mut Mock, path: &str) -> (String, oneshot::Sender<Reply>) {
    loop {
        let (req, reply) = backend.next().await;
        if req.contains(path) {
            return (req, reply);
        }
        if req.contains("settings/query") {
            reply
                .send((
                    json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
                    None,
                ))
                .unwrap();
        } else if req.contains("mpc-wallet/list") {
            reply.send((json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}]),None)).unwrap();
        } else if req.contains("networks/active") {
            reply.send((json!([]), None)).unwrap();
        } else {
            panic!("unexpected backend path {req}");
        }
    }
}
async fn status_is(a: &Arc<Wallet>, expected: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let s = a.native_control(Control::Status {}).await.unwrap();
            if s["status"] == expected {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
async fn arm_pending(
    a: &Arc<Wallet>,
    backend: &mut Mock,
    source: &SourceFixture,
    mode: &str,
    valid_for_ms: Option<u64>,
) -> u64 {
    let mut c = super::super::native_events_tests::pending_config(mode);
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    if let Some(ms) = valid_for_ms {
        c["valid_for_ms"] = json!(ms);
    }
    a.native_control(Control::Load { config: c }).await.unwrap();
    arm_common(a, backend).await
}
async fn arm_common(a: &Arc<Wallet>, backend: &mut Mock) -> u64 {
    let w = a.clone();
    let arm = tokio::spawn(async move {
        w.native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true,
        })
        .await
    });
    for _ in 0..3 {
        let (req, reply) = backend.next().await;
        let value = if req.contains("settings/query") {
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
        } else if req.contains("mpc-wallet/list") {
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])
        } else {
            assert!(req.contains("networks/active"));
            json!([])
        };
        reply.send((value, None)).unwrap();
    }
    arm.await.unwrap().unwrap()["resolved_deadline_ms"]
        .as_u64()
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_hash_fetches_real_fields_and_fires_once() {
    use super::super::native_events_tests::pending_transaction;
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let e = pending_transaction(0xbb);
    let source = SourceFixture::protocol(
        "hashes",
        BTreeMap::from([(e["hash"].as_str().unwrap().into(), e.clone())]),
        false,
        "0x38",
    )
    .await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "hashes", None).await;
    status_is(&a, "READY").await;
    source.events.send(e["hash"].clone()).await.unwrap();
    source.events.send(e["hash"].clone()).await.unwrap();
    let (req, reply) = backend_until(&mut backend, "contract-call/preview").await;
    assert!(req.contains("0x12345678"));
    // Trigger evidence is the RPC transaction, not an invented mined log.
    let status = a.native_control(Control::Status {}).await.unwrap();
    assert_eq!(status["detail"]["event"], e);
    assert!(status["detail"]["event"].get("topics").is_none());
    reply
        .send((executor_tests::valid_preview("pending-preview"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"pending-accepted"}), None))
        .unwrap();
    let s = status_is(&a, "SUCCESS").await;
    assert!(s["operation_id"].as_str().unwrap().starts_with("pnd-"));
    a.native_drain().await;
    let requests = source.rpc_requests.lock().unwrap().clone();
    assert_eq!(
        requests
            .iter()
            .filter(|v| v["method"] == "eth_getTransactionByHash")
            .count(),
        1
    );
    assert!(requests
        .iter()
        .all(|v| v["method"] != "eth_getLogs" && v["method"] != "eth_blockNumber"));
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_full_objects_skip_mined_and_nonmatches_then_accept_replacement() {
    use super::super::native_events_tests::pending_transaction;
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "full", None).await;
    status_is(&a, "READY").await;
    for (byte, key, val) in [
        (
            1,
            "from",
            json!("0x4444444444444444444444444444444444444444"),
        ),
        (2, "to", json!("0x4444444444444444444444444444444444444444")),
        (3, "input", json!(format!("0xabcdef02{:064x}", 17))),
        (4, "input", json!(format!("0xabcdef01{:064x}", 18))),
        (5, "blockHash", json!(format!("0x{}", "aa".repeat(32)))),
        (6, "blockNumber", json!("0x1")),
    ] {
        let mut e = pending_transaction(byte);
        e[key] = val;
        source.events.send(e.clone()).await.unwrap();
        source.events.send(e).await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(
        a.native_control(Control::Status {}).await.unwrap()["status"],
        "READY"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
            .await
            .is_err()
    );
    // Same sender/nonce as earlier nonmatching tx: only actual replacement fields may trigger.
    let replacement = pending_transaction(7);
    source.events.send(replacement.clone()).await.unwrap();
    source.events.send(replacement).await.unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    reply
        .send((executor_tests::valid_preview("full-pending"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"full-accepted"}), None))
        .unwrap();
    status_is(&a, "SUCCESS").await;
    a.native_drain().await;
    assert!(source
        .rpc_requests
        .lock()
        .unwrap()
        .iter()
        .all(|v| v["method"] == "eth_chainId"));
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_hash_null_and_mined_are_not_triggers_or_retried() {
    use super::super::native_events_tests::pending_transaction;
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let null = pending_transaction(1);
    let mut mined = pending_transaction(2);
    mined["blockHash"] = json!(format!("0x{}", "aa".repeat(32)));
    mined["blockNumber"] = json!("0x1");
    let source = SourceFixture::protocol(
        "hashes",
        BTreeMap::from([(mined["hash"].as_str().unwrap().into(), mined.clone())]),
        false,
        "0x38",
    )
    .await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "hashes", None).await;
    status_is(&a, "READY").await;
    for e in [null, mined] {
        source.events.send(e["hash"].clone()).await.unwrap();
        source.events.send(e["hash"].clone()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(
        a.native_control(Control::Status {}).await.unwrap()["status"],
        "READY"
    );
    a.native_control(Control::Disarm {
        task_id: "claim-1".into(),
    })
    .await
    .unwrap();
    a.native_drain().await;
    assert_eq!(
        source
            .rpc_requests
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["method"] == "eth_getTransactionByHash")
            .count(),
        2
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_rejected_subscription_never_falls_back() {
    for mode in ["hashes", "full"] {
        let root = tempfile::tempdir().unwrap();
        let mut backend = Mock::new().await;
        let source = SourceFixture::protocol(mode, BTreeMap::new(), true, "0x38").await;
        let a = wallet(
            root.path(),
            "a",
            "instance-a",
            A,
            Some("cookie-a"),
            &backend.origin,
        );
        arm_pending(&a, &mut backend, &source, mode, None).await;
        let s = status_is(&a, "FAILED").await;
        a.native_drain().await;
        assert!(s["detail"]["error"].as_str().unwrap().contains("rejected"));
        assert_eq!(s["detail"]["submission_attempted"], false);
        assert!(a.check_reservation().is_ok());
        assert!(source
            .rpc_requests
            .lock()
            .unwrap()
            .iter()
            .all(|v| v["method"] == "eth_chainId"));
        assert!(
            tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
                .await
                .is_err()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_wrong_chain_or_lookup_hash_fails_closed() {
    use super::super::native_events_tests::pending_transaction;
    for case in ["ws", "http", "lookup"] {
        let root = tempfile::tempdir().unwrap();
        let mut backend = Mock::new().await;
        let e = pending_transaction(1);
        let other = pending_transaction(2);
        let rows = match case {
            "http" => BTreeMap::from([("__chain".into(), json!("0x1"))]),
            "lookup" => BTreeMap::from([(e["hash"].as_str().unwrap().into(), other)]),
            _ => BTreeMap::new(),
        };
        let source = SourceFixture::protocol(
            "hashes",
            rows,
            false,
            if case == "ws" { "0x1" } else { "0x38" },
        )
        .await;
        let a = wallet(
            root.path(),
            "a",
            "instance-a",
            A,
            Some("cookie-a"),
            &backend.origin,
        );
        // HTTP mismatch is detected during arm's existing readiness stage.
        if case == "http" {
            let mut c = super::super::native_events_tests::pending_config("hashes");
            c["rpc_url"] = json!(source.rpc_url);
            c["ws_url"] = json!(source.ws_url);
            a.native_control(Control::Load { config: c }).await.unwrap();
            let w = a.clone();
            let arm = tokio::spawn(async move {
                w.native_control(Control::Arm {
                    task_id: "claim-1".into(),
                    confirm: true,
                })
                .await
            });
            for _ in 0..3 {
                let (req, reply) = backend.next().await;
                let val = if req.contains("settings/query") {
                    json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
                } else if req.contains("mpc-wallet/list") {
                    json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])
                } else {
                    json!([])
                };
                reply.send((val, None)).unwrap();
            }
            assert!(arm.await.unwrap().unwrap_err().contains("chainId"));
        } else {
            arm_pending(&a, &mut backend, &source, "hashes", None).await;
            if case == "lookup" {
                status_is(&a, "READY").await;
                source.events.send(e["hash"].clone()).await.unwrap();
            }
        }
        let s = status_is(&a, "FAILED").await;
        a.native_drain().await;
        assert_eq!(s["detail"]["submission_attempted"], false);
        assert!(a.check_reservation().is_ok());
        assert!(
            tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
                .await
                .is_err()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_malformed_or_wrong_mode_notifications_fail_closed() {
    use super::super::native_events_tests::pending_transaction;
    let mut malformed = pending_transaction(1);
    malformed.as_object_mut().unwrap().remove("blockNumber");
    let mut wrong_chain = pending_transaction(2);
    wrong_chain["chainId"] = json!("0x1");
    for (mode, event) in [
        ("hashes", json!("0x1")),
        ("hashes", pending_transaction(1)),
        ("full", pending_transaction(1)["hash"].clone()),
        ("full", malformed),
        ("full", wrong_chain),
        ("full", json!({"__raw":"not JSON"})),
        (
            "full",
            json!({"__raw":{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"wrong-id","result":pending_transaction(1)}}}),
        ),
        ("full", json!({"__close":true})),
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut backend = Mock::new().await;
        let source = SourceFixture::protocol(mode, BTreeMap::new(), false, "0x38").await;
        let a = wallet(
            root.path(),
            "a",
            "instance-a",
            A,
            Some("cookie-a"),
            &backend.origin,
        );
        arm_pending(&a, &mut backend, &source, mode, None).await;
        status_is(&a, "READY").await;
        source.events.send(event).await.unwrap();
        let s = status_is(&a, "FAILED").await;
        a.native_drain().await;
        assert_eq!(s["detail"]["submission_attempted"], false);
        assert!(a.check_reservation().is_ok());
        assert!(
            tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
                .await
                .is_err()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_ingress_overload_stops_instead_of_unbounded_fetches() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("hashes", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "hashes", None).await;
    status_is(&a, "READY").await;
    source
        .rpc_delay_ms
        .store(1000, std::sync::atomic::Ordering::SeqCst);
    for n in 0..16 {
        let _ = source.events.send(json!(format!("0x{n:064x}"))).await;
    }
    let s = status_is(&a, "FAILED").await;
    a.native_drain().await;
    assert!(s["detail"]["error"].as_str().unwrap().contains("overflow"));
    assert!(
        source
            .rpc_requests
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["method"] == "eth_getTransactionByHash")
            .count()
            <= 1
    );
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn native_disarm_requires_old_worker_to_drain_before_new_load() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "full", None).await;
    status_is(&a, "READY").await;
    a.native_control(Control::Disarm {
        task_id: "claim-1".into(),
    })
    .await
    .unwrap();
    let mut next = super::super::native_events_tests::pending_config("full");
    next["task_id"] = json!("claim-2");
    let result = a
        .native_control(Control::Load {
            config: next.clone(),
        })
        .await;
    assert!(result.is_err(), "new load overlapped stale source worker");
    assert!(result.unwrap_err().contains("draining"));
    a.native_drain().await;
    let s = a
        .native_control(Control::Load { config: next })
        .await
        .unwrap();
    assert_eq!(s["task_id"], "claim-2");
    assert_eq!(s["status"], "LOADED");
    assert!(s["resolved_deadline_ms"].is_null());
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_duration_starts_at_first_arm_and_preparation_cannot_renew_it() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let mut c = super::super::native_events_tests::pending_config("full");
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    c["valid_for_ms"] = json!(1500);
    let loaded = a.native_control(Control::Load { config: c }).await.unwrap();
    assert!(loaded["resolved_deadline_ms"].is_null());
    tokio::time::sleep(Duration::from_millis(1600)).await;
    let arm_start = chrono::Utc::now().timestamp_millis() as u64;
    let w = a.clone();
    let arm = tokio::spawn(async move {
        w.native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true,
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(
        !arm.is_finished(),
        "loading time consumed validity, or duration was not resolved at arm"
    );
    let (req, reply) = backend.next().await;
    assert!(req.contains("settings/query"));
    let preparing = a.native_control(Control::Status {}).await.unwrap();
    let resolved = preparing["resolved_deadline_ms"]
        .as_u64()
        .expect("arm did not store a resolved deadline");
    assert!(resolved >= arm_start + 1500 && resolved <= arm_start + 1600);
    tokio::time::sleep(Duration::from_millis(1550)).await;
    reply
        .send((
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            None,
        ))
        .unwrap();
    for _ in 0..2 {
        let (req, reply) = backend.next().await;
        let v = if req.contains("mpc-wallet/list") {
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])
        } else {
            assert!(req.contains("networks/active"));
            json!([])
        };
        reply.send((v, None)).unwrap();
    }
    assert!(
        arm.await.unwrap().is_err(),
        "preparation renewed or ignored elapsed validity"
    );
    let s = a.native_control(Control::Status {}).await.unwrap();
    assert_eq!(s["status"], "FAILED");
    assert_eq!(s["resolved_deadline_ms"], resolved);
    assert!(a.check_reservation().is_ok());
    assert!(a
        .native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true
        })
        .await
        .is_err());
    a.native_control(Control::Disarm {
        task_id: "claim-1".into(),
    })
    .await
    .unwrap();
    assert!(a
        .native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true
        })
        .await
        .is_err());
    assert_eq!(
        a.native_control(Control::Status {}).await.unwrap()["resolved_deadline_ms"],
        resolved
    );
    let disk: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join("native-event-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(disk["current"]["resolved_deadline_ms"], resolved);
    assert!(disk["current"]["config"].get("deadline_ms").is_none());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_deadline_or_disarm_while_waiting_releases_wallet() {
    for disarm in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut backend = Mock::new().await;
        let source = SourceFixture::protocol("hashes", BTreeMap::new(), false, "0x38").await;
        let a = wallet(
            root.path(),
            "a",
            "instance-a",
            A,
            Some("cookie-a"),
            &backend.origin,
        );
        arm_pending(&a, &mut backend, &source, "hashes", Some(3000)).await;
        status_is(&a, "READY").await;
        if disarm {
            a.native_control(Control::Disarm {
                task_id: "claim-1".into(),
            })
            .await
            .unwrap();
        }
        status_is(&a, if disarm { "DISARMED" } else { "FAILED" }).await;
        a.native_drain().await;
        let s = a.native_control(Control::Status {}).await.unwrap();
        assert_eq!(s["detail"]["submission_attempted"], false);
        assert!(a.check_reservation().is_ok());
        let _ = source
            .events
            .send(super::super::native_events_tests::pending_transaction(1)["hash"].clone())
            .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
                .await
                .is_err()
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_deadline_is_checked_after_last_async_precheck() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let deadline = arm_pending(&a, &mut backend, &source, "full", Some(5000)).await;
    status_is(&a, "READY").await;
    source
        .events
        .send(super::super::native_events_tests::pending_transaction(1))
        .await
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    // Let the first precheck cache expire while the preview is held. The last gate must
    // still enforce the original activation deadline after its asynchronous settings read.
    tokio::time::sleep(Duration::from_millis(3100)).await;
    reply
        .send((executor_tests::valid_preview("late-precheck"), None))
        .unwrap();
    let (req, reply) = backend.next().await;
    assert!(req.contains("settings/query"));
    let wait = deadline.saturating_sub(chrono::Utc::now().timestamp_millis() as u64) + 50;
    tokio::time::sleep(Duration::from_millis(wait)).await;
    reply
        .send((
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            None,
        ))
        .unwrap();
    // Address reads may still be completed, but there must never be an execute POST.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if a.native_control(Control::Status {}).await.unwrap()["status"] == "FAILED" {break;}
            if let Ok(Some((req,reply)))=tokio::time::timeout(Duration::from_millis(5),backend.requests.recv()).await {
                assert!(!req.contains("contract-call/execute"));
                let v=if req.contains("mpc-wallet/list"){json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])}else{assert!(req.contains("networks/active"));json!([])};
                reply.send((v,None)).unwrap();
            }
        }
    }).await.unwrap();
    a.native_drain().await;
    let s = a.native_control(Control::Status {}).await.unwrap();
    assert_eq!(s["status"], "FAILED");
    assert_eq!(s["detail"]["submission_attempted"], false);
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_submitted_result_survives_deadline_and_cannot_be_disarmed() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let deadline = arm_pending(&a, &mut backend, &source, "full", Some(4000)).await;
    status_is(&a, "READY").await;
    source
        .events
        .send(super::super::native_events_tests::pending_transaction(1))
        .await
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    reply
        .send((executor_tests::valid_preview("before-expiry"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    let disk: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join("native-event-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        disk["current"]["status"], "EXECUTING",
        "in-flight execute must be durable before a crash"
    );
    let mut recovered = super::super::native_events::Book::default();
    recovered.restore(&a.entry.dir).unwrap();
    assert_eq!(recovered.status()["status"], "UNKNOWN");
    assert_eq!(recovered.status()["resolved_deadline_ms"], deadline);
    assert!(a
        .native_control(Control::Disarm {
            task_id: "claim-1".into()
        })
        .await
        .unwrap_err()
        .contains("draining"));
    let wait = deadline.saturating_sub(chrono::Utc::now().timestamp_millis() as u64) + 50;
    tokio::time::sleep(Duration::from_millis(wait)).await;
    reply
        .send((json!({"orderId":"ack-after-deadline"}), None))
        .unwrap();
    let s = status_is(&a, "SUCCESS").await;
    a.native_drain().await;
    assert_eq!(
        s["detail"]["result"]["execution"]["orderId"],
        "ack-after-deadline"
    );
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_long_wait_revalidates_developer_mode_before_preview() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "full", Some(86_400_000)).await;
    status_is(&a, "READY").await;
    // The pre-arm cache must not remain authorization for the full task lifetime.
    tokio::time::sleep(Duration::from_millis(3100)).await;
    source
        .events
        .send(super::super::native_events_tests::pending_transaction(1))
        .await
        .unwrap();
    let (req, reply) = backend.next().await;
    assert!(req.contains("settings/query"));
    reply
        .send((json!({"devMode":{"enabled":false,"expiresAt":1}}), None))
        .unwrap();
    let s = status_is(&a, "FAILED").await;
    a.native_drain().await;
    assert!(s["detail"]["error"]
        .as_str()
        .unwrap()
        .contains("developer mode"));
    assert_eq!(s["detail"]["submission_attempted"], false);
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_session_replacement_invalidates_waiting_task() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "full", None).await;
    status_is(&a, "READY").await;
    // Synthetic fixture only: changing the session stamp must invalidate the arm-time binding.
    let path = a.entry.dir.join("session.json");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b' ');
    std::fs::write(&path, bytes).unwrap();
    source
        .events
        .send(super::super::native_events_tests::pending_transaction(1))
        .await
        .unwrap();
    let s = status_is(&a, "FAILED").await;
    a.native_drain().await;
    assert!(s["detail"]["error"]
        .as_str()
        .unwrap()
        .contains("binding changed"));
    assert_eq!(s["detail"]["submission_attempted"], false);
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_ws_fixed_event_reuses_resident_fire_and_is_one_shot() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let mut c = config();
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    a.native_control(Control::Load { config: c }).await.unwrap();
    let w = a.clone();
    let arm = tokio::spawn(async move {
        w.native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true,
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(
        !arm.is_finished(),
        "arm returned without wallet readiness checks"
    );
    loop {
        let (req, reply) = backend.next().await;
        if req.contains("settings/query") {
            reply
                .send((
                    json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
                    None,
                ))
                .unwrap();
        } else if req.contains("mpc-wallet/list") {
            reply.send((json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}]),None)).unwrap();
        } else if req.contains("networks/active") {
            reply.send((json!([]), None)).unwrap();
            break;
        } else {
            panic!("unexpected pre-event call {req}");
        }
    }
    arm.await.unwrap().unwrap();
    status_is(&a, "READY").await;
    assert!(a.check_reservation().is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), backend.requests.recv())
            .await
            .is_err()
    );
    let e = super::super::native_events_tests::event();
    source.events.send(e.clone()).await.unwrap();
    source.events.send(e).await.unwrap();
    let (req, reply) = backend_until(&mut backend, "contract-call/preview").await;
    assert!(req.contains("0x12345678"));
    reply
        .send((executor_tests::valid_preview("native-preview"), None))
        .unwrap();
    let (_req, reply) = backend_until(&mut backend, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"native-accepted"}), None))
        .unwrap();
    let s = status_is(&a, "SUCCESS").await;
    assert_eq!(
        s["detail"]["success_scope"],
        "backend_acknowledged_not_chain_confirmation"
    );
    assert!(s["operation_id"].as_str().unwrap().starts_with("evt-"));
    assert!(s["detail"]["metrics"]["action_build_ms"].is_number());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
            .await
            .is_err()
    );
    assert!(a
        .native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true
        })
        .await
        .is_err());
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_shutdown_disarms_waiter_and_does_not_leave_source_owned() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let mut c = config();
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    a.native_control(Control::Load { config: c }).await.unwrap();
    let w = a.clone();
    let arm = tokio::spawn(async move {
        w.native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true,
        })
        .await
    });
    for _ in 0..3 {
        let (req, reply) = backend.next().await;
        let value = if req.contains("settings/query") {
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
        } else if req.contains("mpc-wallet/list") {
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])
        } else {
            json!([])
        };
        reply.send((value, None)).unwrap();
    }
    arm.await.unwrap().unwrap();
    status_is(&a, "READY").await;
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a.clone())])).await;
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    assert_eq!(
        a.native_control(Control::Status {}).await.unwrap()["status"],
        "DISARMED"
    );
    assert!(a.check_reservation().is_ok());
    let _ = source
        .events
        .send(super::super::native_events_tests::event())
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn native_legacy_controls_block_replay_but_allow_unique_relative_tasks() {
    use std::os::unix::fs::PermissionsExt;
    for status in [
        "LOADED",
        "READY",
        "ACCEPTED",
        "EXECUTING",
        "SUCCESS",
        "FAILED",
        "UNKNOWN",
        "DISARMED",
    ] {
        let root = tempfile::tempdir().unwrap();
        let backend = Mock::new().await;
        let a = wallet(
            root.path(),
            "a",
            "instance-a",
            A,
            Some("cookie-a"),
            &backend.origin,
        );
        let path = a.entry.dir.join("native-event-state.json");
        let old = super::super::native_events_tests::legacy_state(status);
        std::fs::write(&path, old.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        a.native_control(Control::Status {}).await.unwrap();
        assert!(a
            .native_control(Control::Arm {
                task_id: "claim-1".into(),
                confirm: true
            })
            .await
            .is_err());
        assert!(a
            .native_control(Control::Load { config: config() })
            .await
            .unwrap_err()
            .contains("already used"));
        a.native_control(Control::Disarm {
            task_id: "claim-1".into(),
        })
        .await
        .unwrap();
        let mut reused = config();
        reused["task_id"] = json!("old-success");
        assert!(a
            .native_control(Control::Load { config: reused })
            .await
            .unwrap_err()
            .contains("already used"));
        let mut next = config();
        next["task_id"] = json!("fresh-relative");
        let loaded = a
            .native_control(Control::Load { config: next })
            .await
            .unwrap();
        assert_eq!(loaded["status"], "LOADED");
        assert!(loaded["resolved_deadline_ms"].is_null());
        a.native_control(Control::Disarm {
            task_id: "fresh-relative".into(),
        })
        .await
        .unwrap();
        let disk: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(disk["legacy_records"]["claim-1"], old["current"]);
        assert!(a.check_reservation().is_ok());
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_route_load_delay_and_build_timeout_keep_first_arm_budget_and_zero_execute() {
    use super::super::native_events_tests::{
        pending_config, pending_transaction, route_config, synthetic_route_key,
    };
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let key = synthetic_route_key(root.path());
    let mut c = pending_config("full");
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    c["valid_for_ms"] = json!(5000);
    let mut r = route_config(&key);
    r["timeout_ms"] = json!(10000);
    r["max_wait_ms"] = json!(10000);
    c["action"] = json!({"kind":"route", "config":r});
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let loaded = a.native_control(Control::Load { config: c }).await.unwrap();
    assert!(loaded["resolved_deadline_ms"].is_null());
    a.native.lock().await.route_test_origin(origin);
    // Longer than the entire task duration: load must consume none of it.
    tokio::time::sleep(Duration::from_millis(5100)).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
    let before_arm = chrono::Utc::now().timestamp_millis() as u64;
    let w = a.clone();
    let arm = tokio::spawn(async move {
        w.native_control(Control::Arm {
            task_id: "claim-1".into(),
            confirm: true,
        })
        .await
    });
    for _ in 0..3 {
        let (req, reply) = backend.next().await;
        let v = if req.contains("settings/query") {
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
        } else if req.contains("mpc-wallet/list") {
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"0x1111111111111111111111111111111111111111"}]}]}])
        } else {
            assert!(req.contains("networks/active"));
            json!([])
        };
        reply.send((v, None)).unwrap();
    }
    let armed = arm.await.unwrap().unwrap();
    let deadline = armed["resolved_deadline_ms"].as_u64().unwrap();
    assert!(deadline >= before_arm + 5000 && deadline <= before_arm + 5500);
    status_is(&a, "READY").await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    source.events.send(pending_transaction(1)).await.unwrap();
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("/api/v6/dex/aggregator/swap"));
    // Hold the route response through original task expiry, not a fresh trigger-time window.
    let s = status_is(&a, "FAILED").await;
    assert_eq!(s["resolved_deadline_ms"], deadline);
    assert_eq!(s["detail"]["submission_attempted"], false);
    assert!(s["detail"]["error"].as_str().unwrap().contains("deadline"));
    assert!((chrono::Utc::now().timestamp_millis() as u64) < deadline + 300);
    a.native_drain().await;
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err(),
        "route timeout reached preview/execute"
    );
    let disk: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join("native-event-state.json")).unwrap(),
    )
    .unwrap();
    assert!(disk["current"]["config"]["action"]["config"]
        .get("deadline_unix_ms")
        .is_none());
}
#[tokio::test]
async fn native_load_disarm_is_durable_and_never_reserves_unarmed_wallet() {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let c = config();
    let loaded = a
        .native_control(Control::Load { config: c.clone() })
        .await
        .unwrap();
    assert_eq!(loaded["status"], "LOADED");
    assert!(a.check_reservation().is_ok());
    let d = a
        .native_control(Control::Disarm {
            task_id: "claim-1".into(),
        })
        .await
        .unwrap();
    assert_eq!(d["status"], "DISARMED");
    assert!(a.native_control(Control::Load { config: c }).await.is_err());
    assert_eq!(
        a.native_control(Control::Status {}).await.unwrap()["status"],
        "DISARMED"
    );
    let persisted = std::fs::read_to_string(a.entry.dir.join("native-event-state.json")).unwrap();
    assert!(persisted.contains("DISARMED"));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_selector_only_task_accepts_dynamic_calldata() {
    use super::super::native_events_tests::pending_config;
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let mut c = pending_config("full");
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    // No ABI word is pinned: any calldata carrying the pinned selector is a trigger, including
    // dynamic calldata that is not a whole number of 32-byte words.
    c["source"]["args"] = json!([]);
    a.native_control(Control::Load { config: c }).await.unwrap();
    arm_common(&a, &mut backend).await;
    status_is(&a, "READY").await;
    let mut dynamic = super::super::native_events_tests::pending_transaction(0xaa);
    dynamic["input"] = json!(format!("0xabcdef01{:064x}{:064x}deadbeef", 0x20u64, 4u64));
    source.events.send(dynamic).await.unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    reply
        .send((executor_tests::valid_preview("dynamic-calldata"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"dynamic-accepted"}), None))
        .unwrap();
    let s = status_is(&a, "SUCCESS").await;
    a.native_drain().await;
    assert!(s["operation_id"].as_str().unwrap().starts_with("pnd-"));
    assert!(a.check_reservation().is_ok());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_pending_malformed_configured_word_is_a_nonmatch_then_next_fires_once() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::protocol("full", BTreeMap::new(), false, "0x38").await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    arm_pending(&a, &mut backend, &source, "full", None).await;
    status_is(&a, "READY").await;
    // One mempool transaction with a matching selector and unpinnable calldata must never abort a
    // task that may legitimately wait for hours.
    for (byte, input) in [
        (0xaau8, format!("0xabcdef01{:064x}ff", 17)),
        (0xbbu8, "0xabcdef01".to_string()),
    ] {
        let mut bad = super::super::native_events_tests::pending_transaction(byte);
        bad["input"] = json!(input);
        source.events.send(bad).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            a.native_control(Control::Status {}).await.unwrap()["status"],
            "READY",
            "a non-matching transaction ended the task"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err()
    );
    let good = super::super::native_events_tests::pending_transaction(7);
    source.events.send(good.clone()).await.unwrap();
    source.events.send(good).await.unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    reply
        .send((executor_tests::valid_preview("after-nonmatch"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"after-nonmatch-accepted"}), None))
        .unwrap();
    status_is(&a, "SUCCESS").await;
    a.native_drain().await;
    assert!(a.check_reservation().is_ok());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), backend.requests.recv())
            .await
            .is_err(),
        "the trigger fired more than once"
    );
}
/// A fire that already reached the execute POST is submitted. Losing the terminal state write
/// afterwards must not rewrite that evidence as "never sent", so this drives the task to the
/// execute POST and only then makes the state directory unpersistable.
async fn submitted_fire_with_unpersistable_state_dir(
    execute_reply: Value,
    expected: &str,
) -> (Value, Value) {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let mut backend = Mock::new().await;
    let source = SourceFixture::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &backend.origin,
    );
    let mut c = config();
    c["rpc_url"] = json!(source.rpc_url);
    c["ws_url"] = json!(source.ws_url);
    a.native_control(Control::Load { config: c }).await.unwrap();
    arm_common(&a, &mut backend).await;
    status_is(&a, "READY").await;
    source
        .events
        .send(super::super::native_events_tests::event())
        .await
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/preview").await;
    reply
        .send((executor_tests::valid_preview("persist-failure"), None))
        .unwrap();
    let (_, reply) = backend_until(&mut backend, "contract-call/execute").await;
    // The execute POST has left the process: from here on the operation counts as submitted.
    std::fs::set_permissions(&a.entry.dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    reply.send((execute_reply, None)).unwrap();
    let s = status_is(&a, expected).await;
    a.native_drain().await;
    assert!(
        a.check_reservation().is_ok(),
        "a terminal persist failure left the wallet reserved"
    );
    let disk: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join("native-event-state.json")).unwrap(),
    )
    .unwrap();
    (s, disk)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_persist_failure_after_submit_keeps_success_evidence() {
    let (s, disk) = submitted_fire_with_unpersistable_state_dir(
        json!({"orderId":"ack-before-persist-failure"}),
        "SUCCESS",
    )
    .await;
    let detail = &s["detail"];
    assert_eq!(
        detail["persistence_failed"], true,
        "terminal persist failure not recorded: {detail}"
    );
    assert_eq!(
        detail["submission_attempted"], true,
        "submitted fire described as unsent: {detail}"
    );
    assert_eq!(
        detail["result"]["execution"]["orderId"],
        "ack-before-persist-failure"
    );
    assert_eq!(
        detail["success_scope"], "backend_acknowledged_not_chain_confirmation",
        "success scope dropped: {detail}"
    );
    assert!(detail["metrics"]["action_build_ms"].is_number(), "{detail}");
    assert!(
        detail.get("route").is_some(),
        "route evidence dropped: {detail}"
    );
    assert_eq!(
        detail["retry"], "never; reconcile operation journal",
        "missing reconcile instruction: {detail}"
    );
    assert!(
        detail["error"].as_str().unwrap().contains("directory"),
        "persistence error not merged: {detail}"
    );
    assert_eq!(
        disk["current"]["status"], "EXECUTING",
        "on-disk copy was expected to stay stale for reconciliation"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_persist_failure_after_submit_keeps_unknown_evidence() {
    let (s, _) = submitted_fire_with_unpersistable_state_dir(
        json!({"code":"1200001","message":"synthetic execute failure"}),
        "UNKNOWN",
    )
    .await;
    let detail = &s["detail"];
    assert_eq!(
        detail["persistence_failed"], true,
        "terminal persist failure not recorded: {detail}"
    );
    assert_eq!(
        detail["submission_attempted"], true,
        "submitted fire described as unsent: {detail}"
    );
    assert!(
        detail["error"].as_str().unwrap().contains("1200001"),
        "the real execute error was replaced by the persistence error: {detail}"
    );
    assert_eq!(detail["retry"], "never; reconcile operation journal");
    assert!(!detail
        .to_string()
        .contains("\"submission_attempted\":false"));
}
