use super::*;
#[path = "executor_validation_tests.rs"]
mod validation;

pub(super) const SENDER: &str = "0x1111111111111111111111111111111111111111";
const TARGET: &str = "0x2222222222222222222222222222222222222222";
fn prepare(id: &str) -> Value {
    json!({"op":"prepare","id":"p","name":"alice","operation_id":id,
        "deadline_ms":chrono::Utc::now().timestamp_millis()+60000,
        "call":{"binance_chain_id":"56","from":SENDER,"to":TARGET,"value":"0","input_data":"0x12345678","gas_limit":100000}})
}
async fn exchange(path: &Path, value: Value) -> Value {
    let mut stream = UnixStream::connect(path).await.unwrap();
    stream
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
    let bytes = read_frame(&mut stream, MAX_RESPONSE).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
async fn prepare_ready(mock: &mut Mock) {
    let (request, reply) = mock.next().await;
    assert!(request.contains("settings/query"));
    reply
        .send((
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            None,
        ))
        .unwrap();
    let (request, reply) = mock.next().await;
    assert!(request.contains("/mpc-wallet/list"));
    reply
        .send((
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":SENDER}]}]}]),
            None,
        ))
        .unwrap();
    let (request, reply) = mock.next().await;
    assert!(request.contains("/networks/active"));
    reply.send((json!([]), None)).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preparation_reserves_only_selected_wallet_and_disarms() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let b = wallet(
        root.path(),
        "b",
        "instance-b",
        B,
        Some("cookie-b"),
        &mock.origin,
    );
    let (path, server) = start(
        root.path(),
        BTreeMap::from([("alice".into(), a), ("bob".into(), b)]),
    )
    .await;
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, prepare("generic-1")).await });
    prepare_ready(&mut mock).await;
    let result = job.await.unwrap();
    assert_eq!(result["success"], true, "{result}");
    assert_eq!(result["data"]["status"], "PREPARED");
    let busy = tokio::time::timeout(
        Duration::from_millis(100),
        request(&path, "alice", &["wallet", "status"]),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap_err();
    assert!(busy.contains("BUSY"), "{busy}");
    let bob = request(&path, "bob", &["wallet", "status"]);
    let (http, reply) = mock.next().await;
    assert!(http.contains(B));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    assert_eq!(bob.await.unwrap().unwrap()["status"], "CONNECTED");
    let result = exchange(
        &path,
        json!({"op":"disarm","id":"d","name":"alice","operation_id":"generic-1"}),
    )
    .await;
    assert_eq!(result["data"]["status"], "DISARMED", "{result}");
    assert!(
        tokio::time::timeout(Duration::from_millis(30), mock.requests.recv())
            .await
            .is_err()
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

fn trigger(id: &str) -> Value {
    json!({"op":"trigger","id":"t","name":"alice","operation_id":id,"confirm":true})
}
pub(super) fn valid_preview(id: &str) -> Value {
    json!({"requestId":id,"from":SENDER,"binanceChainId":"56","simulationSuccess":true,"expiresAt":4102444800u64,"risks":{"riskDetails":[]}})
}
async fn until_call(mock: &mut Mock, suffix: &str) -> oneshot::Sender<Reply> {
    loop {
        let (request, reply) = mock.next().await;
        if request.contains(suffix) {
            return reply;
        }
        if request.contains("settings/query") {
            reply
                .send((
                    json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
                    None,
                ))
                .unwrap();
        } else if request.contains("mpc-wallet/list") {
            reply.send((json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":SENDER}]}]}]),None)).unwrap();
        } else if request.contains("networks/active") {
            reply.send((json!([]), None)).unwrap();
        } else {
            panic!(
                "unexpected synthetic HTTP path: {}",
                request.lines().next().unwrap()
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_trigger_submits_exact_generic_call_and_replays_result() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a.clone())])).await;
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, prepare("generic-fire")).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, trigger("generic-fire")).await });
    let reply = until_call(&mut mock, "contract-call/preview").await;
    reply
        .send((valid_preview("generic-preview"), None))
        .unwrap();
    let reply = until_call(&mut mock, "contract-call/execute").await;
    reply
        .send((json!({"orderId":"generic-order"}), None))
        .unwrap();
    let result = job.await.unwrap();
    assert_eq!(result["success"], true, "{result}");
    assert_eq!(result["data"]["execution"]["orderId"], "generic-order");
    let metrics = &result["data"]["http_metrics"];
    assert!(
        metrics["first_request_gap_ms"].as_f64().is_some(),
        "{result}"
    );
    assert!(metrics["network_wait_ms"].as_f64().unwrap() > 0.0);
    assert!(!metrics["response_to_next_request_ms"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(result["metrics"]["ingress_decode_ms"].as_f64().is_some());
    assert_eq!(result["metrics"]["queue_ms"], 0.0);
    let duplicate = exchange(&path, trigger("generic-fire")).await;
    assert_eq!(duplicate["data"], result["data"]);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), mock.requests.recv())
            .await
            .is_err()
    );
    let journal: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join(".baw-rs-operations/generic-fire.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(journal["state"], "RESPONSE_RECEIVED");
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

async fn finish_trigger(
    mock: &mut Mock,
    job: &mut tokio::task::JoinHandle<Value>,
    preview: Value,
) -> (Value, usize) {
    let mut executes = 0;
    loop {
        tokio::select! {
            result=&mut *job => return (result.unwrap(),executes),
            pair=mock.requests.recv() => {
                let (request,reply)=pair.unwrap();
                let data=if request.contains("settings/query") {json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})}
                else if request.contains("mpc-wallet/list") {json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":SENDER}]}]}])}
                else if request.contains("networks/active") {json!([])}
                else if request.contains("contract-call/preview") {preview.clone()}
                else if request.contains("contract-call/execute") {executes+=1; json!({"orderId":"test-order"})}
                else {panic!("unexpected path")};
                reply.send((data,None)).unwrap();
            }
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_default_risk_gate_rejects_unapproved_risk() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a)])).await;
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, prepare("risky")).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    let p = path.clone();
    let mut job = tokio::spawn(async move { exchange(&p, trigger("risky")).await });
    let mut preview = valid_preview("risky-preview");
    preview["risks"] = json!({"riskDetails":[{"code":"DANGER"}]});
    let (result, executes) = finish_trigger(&mut mock, &mut job, preview).await;
    assert_eq!(executes, 0, "risk must never execute: {result}");
    assert_eq!(result["success"], false);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_policy_allows_only_explicit_risk() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a)])).await;
    let mut value = prepare("allowed");
    value["policy"] =
        json!({"version":1,"chainId":"56","to":TARGET,"valueWei":"0","allowRisks":["ACCEPTED"]});
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, value).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    let p = path.clone();
    let mut job = tokio::spawn(async move { exchange(&p, trigger("allowed")).await });
    let mut preview = valid_preview("allowed-preview");
    preview["risks"] = json!({"riskDetails":[{"code":"ACCEPTED"}]});
    let (result, executes) = finish_trigger(&mut mock, &mut job, preview).await;
    assert_eq!(executes, 1, "{result}");
    assert_eq!(result["success"], true, "{result}");
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn idle_typed_connections_do_not_consume_legacy_slots() {
    let root = tempfile::tempdir().unwrap();
    let (path, server) = start(root.path(), BTreeMap::new()).await;
    let mut connections = Vec::new();
    for n in 0..64 {
        let mut stream = UnixStream::connect(&path).await.unwrap();
        stream
            .write_all(format!("{{\"op\":\"ping\",\"id\":\"{n}\"}}\n").as_bytes())
            .await
            .unwrap();
        let _ = read_frame(&mut stream, MAX_RESPONSE).await;
        connections.push(stream);
    }
    let result = send(&path, Request::Status).await;
    assert!(
        result.is_ok(),
        "legacy starved by idle typed clients: {result:?}"
    );
    drop(connections);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

async fn prepared_fixture(
    id: &str,
) -> (
    tempfile::TempDir,
    Mock,
    Arc<Wallet>,
    PathBuf,
    tokio::task::JoinHandle<Result<Value, String>>,
) {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a.clone())])).await;
    let p = path.clone();
    let value = prepare(id);
    let job = tokio::spawn(async move { exchange(&p, value).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    (root, mock, a, path, server)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_trigger_survives_disconnect_and_inflight_duplicates_are_busy() {
    let (_root, mut mock, a, path, server) = prepared_fixture("disconnect").await;
    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(format!("{}\n", trigger("disconnect")).as_bytes())
        .await
        .unwrap();
    let reply = until_call(&mut mock, "contract-call/preview").await;
    drop(stream);
    let busy = exchange(&path, trigger("disconnect")).await;
    assert!(
        busy["error"]["message"].as_str().unwrap().contains("BUSY"),
        "{busy}"
    );
    reply
        .send((valid_preview("disconnect-preview"), None))
        .unwrap();
    let reply = until_call(&mut mock, "contract-call/execute").await;
    reply.send((json!({"orderId":"survived"}), None)).unwrap();
    // Operation completion, not socket lifetime, controls the mutation.
    let mut result = Value::Null;
    for _ in 0..100 {
        result = exchange(&path, trigger("disconnect")).await;
        if result["success"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        result["data"]["execution"]["orderId"], "survived",
        "{result}"
    );
    let row: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join(".baw-rs-operations/disconnect.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(row["state"], "RESPONSE_RECEIVED");
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_execute_outcome_is_terminal_without_automatic_retry() {
    let (_root, mut mock, a, path, server) = prepared_fixture("unknown").await;
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, trigger("unknown")).await });
    until_call(&mut mock, "contract-call/preview")
        .await
        .send((valid_preview("unknown-preview"), None))
        .unwrap();
    // Drop the HTTP reply after observing submission: remote outcome is unknown.
    drop(until_call(&mut mock, "contract-call/execute").await);
    let result = job.await.unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(
        exchange(&path, trigger("unknown")).await["error"],
        result["error"]
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), mock.requests.recv())
            .await
            .is_err()
    );
    let row: Value = serde_json::from_slice(
        &std::fs::read(a.entry.dir.join(".baw-rs-operations/unknown.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(row["state"], "FAILED_OR_UNKNOWN");
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prepared_identity_and_session_changes_invalidate_without_http() {
    for identity in [true, false] {
        let (_root, mut mock, a, path, server) = prepared_fixture("changed").await;
        if identity {
            std::fs::write(&a.entry.instance_file, "instance-changed").unwrap();
        } else {
            std::fs::write(a.entry.dir.join("session.json"), b"{}").unwrap();
        }
        let result = exchange(&path, trigger("changed")).await;
        assert_eq!(result["success"], false);
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("changed"),
            "{result}"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), mock.requests.recv())
                .await
                .is_err()
        );
        send(&path, Request::Stop).await.unwrap();
        server.await.unwrap().unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deadline_or_identity_change_during_preview_prevents_execute() {
    for expire in [true, false] {
        let (_root, mut mock, a, path, server) = prepared_fixture("during").await;
        if expire {
            // Use the public disarm/reprepare path to establish a short real deadline.
            exchange(
                &path,
                json!({"op":"disarm","id":"d","name":"alice","operation_id":"during"}),
            )
            .await;
            let mut value = prepare("during-short");
            value["deadline_ms"] = json!(chrono::Utc::now().timestamp_millis() + 800);
            let p = path.clone();
            let job = tokio::spawn(async move { exchange(&p, value).await });
            prepare_ready(&mut mock).await;
            assert_eq!(job.await.unwrap()["success"], true);
        }
        let id = if expire { "during-short" } else { "during" };
        let p = path.clone();
        let mut job = tokio::spawn(async move { exchange(&p, trigger(id)).await });
        let reply = until_call(&mut mock, "contract-call/preview").await;
        if expire {
            tokio::time::sleep(Duration::from_millis(850)).await;
        } else {
            std::fs::write(&a.entry.instance_file, "instance-changed").unwrap();
        }
        reply.send((valid_preview("during-preview"), None)).unwrap();
        let (result, executes) = finish_trigger(&mut mock, &mut job, valid_preview("unused")).await;
        assert_eq!(executes, 0, "{result}");
        assert_eq!(result["success"], false);
        send(&path, Request::Stop).await.unwrap();
        server.await.unwrap().unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_buffered_trigger_never_executes_and_releases_reservation() {
    let (_root, mut mock, _a, path, server) = prepared_fixture("initial").await;
    exchange(
        &path,
        json!({"op":"disarm","id":"d","name":"alice","operation_id":"initial"}),
    )
    .await;
    let mut value = prepare("expired");
    value["deadline_ms"] = json!(chrono::Utc::now().timestamp_millis() + 800);
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, value).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    tokio::time::sleep(Duration::from_millis(850)).await;
    let result = exchange(&path, trigger("expired")).await;
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("EXPIRED"),
        "{result}"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), mock.requests.recv())
            .await
            .is_err()
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_preview_validation_fails_closed_on_malformed_or_nested_risks() {
    for bad in [
        json!({"risks":{"riskDetails":"malformed"}}),
        json!({"simulationResult":{"simulationSuccess":true,"simulation":{"simulationSuccess":true,"risks":{"riskDetails":[{"code":"NESTED"}]}}}}),
    ] {
        let (_root, mut mock, _a, path, server) = prepared_fixture("bad-risk").await;
        let p = path.clone();
        let mut job = tokio::spawn(async move { exchange(&p, trigger("bad-risk")).await });
        let mut preview = valid_preview("bad-risk-preview");
        for (key, value) in bad.as_object().unwrap() {
            preview[key] = value.clone();
        }
        let (result, executes) = finish_trigger(&mut mock, &mut job, preview).await;
        assert_eq!(executes, 0, "{result}");
        assert_eq!(result["success"], false);
        send(&path, Request::Stop).await.unwrap();
        server.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn trigger_requires_explicit_confirmation() {
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
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a)])).await;
    let result = exchange(
        &path,
        json!({"op":"trigger","id":"t","name":"alice","operation_id":"no-confirm","confirm":false}),
    )
    .await;
    assert_eq!(result["id"], "t");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("confirm"),
        "{result}"
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn persistent_frames_are_not_lost() {
    let root = tempfile::tempdir().unwrap();
    let (path, server) = start(root.path(), BTreeMap::new()).await;
    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"op\":\"ping\",\"id\":\"one\"}\n{\"op\":\"ping\",\"id\":\"two\"}\n")
        .await
        .unwrap();
    let mut reader = tokio::io::BufReader::new(stream);
    for id in ["one", "two"] {
        let bytes = tokio::time::timeout(
            Duration::from_secs(1),
            read_buffered_frame(&mut reader, MAX_RESPONSE),
        )
        .await
        .unwrap()
        .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["id"], id);
        assert_eq!(value["success"], true);
        assert_eq!(value["metrics"]["queue_policy"], "reject_busy");
        assert!(value["metrics"]["queue_scope"]
            .as_str()
            .unwrap()
            .contains("socket backlog excluded"));
    }
    drop(reader);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
