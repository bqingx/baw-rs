use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_drains_accepted_trigger_but_denies_new_mutations() {
    let (_root, mut mock, _a, path, server) = prepared_fixture("draining").await;
    let mut idle = UnixStream::connect(&path).await.unwrap();
    idle.write_all(b"{\"op\":\"ping\",\"id\":\"idle\"}\n")
        .await
        .unwrap();
    read_frame(&mut idle, MAX_RESPONSE).await.unwrap();
    let p = path.clone();
    let mut job = tokio::spawn(async move { exchange(&p, trigger("draining")).await });
    let reply = until_call(&mut mock, "contract-call/preview").await;
    send(&path, Request::Stop).await.unwrap();
    let _ = idle
        .write_all(format!("{}\n{}\n", prepare("after-stop"), trigger("after-stop")).as_bytes())
        .await;
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_millis(200), idle.read_to_end(&mut bytes))
        .await
        .expect("post-stop socket must close")
        .ok();
    assert!(
        bytes.is_empty(),
        "post-stop mutation dispatched: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        !server.is_finished(),
        "accepted trigger was cancelled by stop"
    );
    reply.send((valid_preview("drain-preview"), None)).unwrap();
    let (result, executes) = finish_trigger(&mut mock, &mut job, valid_preview("unused")).await;
    assert_eq!(executes, 1);
    assert_eq!(result["success"], true, "{result}");
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn policy_uint256_hex_and_decimal_amounts_are_equivalent() {
    let decimal = "340282366920938463463374607431768211456";
    let hex = "0x100000000000000000000000000000000";
    let policy = crate::policy::Policy::parse(
        &json!({"version":1,"chainId":"56","to":TARGET,"valueWei":decimal}).to_string(),
    )
    .unwrap();
    for value in [decimal, hex] {
        policy
            .check_intent(&crate::policy::Intent {
                chain_id: "56",
                to: TARGET,
                value,
                input_data: Some("0x"),
                gas_limit: None,
            })
            .unwrap();
    }
}

#[tokio::test]
async fn shutdown_closes_persistent_sessions_and_rejects_subsequent_frames() {
    let root = tempfile::tempdir().unwrap();
    let (path, server) = start(root.path(), BTreeMap::new()).await;
    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"op\":\"ping\",\"id\":\"before\"}\n")
        .await
        .unwrap();
    assert!(
        serde_json::from_slice::<Value>(&read_frame(&mut stream, MAX_RESPONSE).await.unwrap())
            .unwrap()["success"]
            == true
    );
    send(&path, Request::Stop).await.unwrap();
    let _ = stream
        .write_all(b"{\"op\":\"ping\",\"id\":\"after\"}\n")
        .await;
    let mut bytes = Vec::new();
    let closed =
        tokio::time::timeout(Duration::from_millis(200), stream.read_to_end(&mut bytes)).await;
    assert!(
        closed.is_ok(),
        "old persistent session remained open after stop"
    );
    assert!(
        !String::from_utf8_lossy(&bytes).contains("\"success\":true"),
        "post-stop frame executed"
    );
    tokio::time::timeout(Duration::from_millis(200), server)
        .await
        .expect("shutdown held by idle session")
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn contradictory_or_malformed_gas_echoes_never_execute() {
    for bad in [
        json!({"gasLimit":1000000}),
        json!({"transaction":{"gasLimit":1000000}}),
        json!({"gasLimit":"malformed"}),
    ] {
        let (_root, mut mock, _a, path, server) = prepared_fixture("gas-echo").await;
        let p = path.clone();
        let mut job = tokio::spawn(async move { exchange(&p, trigger("gas-echo")).await });
        let mut preview = valid_preview("gas-echo-preview");
        for (key, value) in bad.as_object().unwrap() {
            preview[key] = value.clone();
        }
        let (result, executes) = finish_trigger(&mut mock, &mut job, preview).await;
        assert_eq!(executes, 0, "invalid gas echo must not execute: {result}");
        assert_eq!(result["success"], false);
        send(&path, Request::Stop).await.unwrap();
        server.await.unwrap().unwrap();
    }
}

/// The backend echoes `gasLimit: null` when it has no estimate yet (observed on a live preview).
/// Null means the same as absent, so it must not be treated as a malformed echo.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn null_gas_echo_is_absence_not_a_malformed_echo() {
    for echo in [json!({"gasLimit":null}), json!({"tx":{"gasLimit":null}})] {
        let (_root, mut mock, _a, path, server) = prepared_fixture("gas-null").await;
        let p = path.clone();
        let mut job = tokio::spawn(async move { exchange(&p, trigger("gas-null")).await });
        let mut preview = valid_preview("gas-null-preview");
        for (key, value) in echo.as_object().unwrap() {
            preview[key] = value.clone();
        }
        let (result, executes) = finish_trigger(&mut mock, &mut job, preview).await;
        assert_eq!(
            executes, 1,
            "null gas echo must not block execution: {result}"
        );
        assert_eq!(result["success"], true, "{result}");
        send(&path, Request::Stop).await.unwrap();
        server.await.unwrap().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hex_value_prepare_preserves_exact_call_and_policy_canonical_amount() {
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
    let mut value = prepare("hex");
    value["call"]["value"] = json!("0x01");
    let p = path.clone();
    let mut job = tokio::spawn(async move { exchange(&p, value).await });
    tokio::select! {
        result=&mut job=>panic!("hex value rejected before checks: {}",result.unwrap()),
        _=prepare_ready(&mut mock)=>{}
    }
    assert_eq!(job.await.unwrap()["success"], true);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn unsupported_total_fee_policy_is_rejected_not_silently_ignored() {
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
    let mut value = prepare("fee-cap");
    value["policy"] =
        json!({"version":1,"chainId":"56","to":TARGET,"valueWei":"0","maxTotalWei":"1"});
    let result = exchange(&path, value).await;
    assert_eq!(result["success"], false, "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("maxTotalWei"),
        "{result}"
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn existing_journal_prevents_resubmission_after_resident_restart() {
    let (_root, mut mock, a, path, server) = prepared_fixture("restart").await;
    let p = path.clone();
    let mut job = tokio::spawn(async move { exchange(&p, trigger("restart")).await });
    let (result, count) =
        finish_trigger(&mut mock, &mut job, valid_preview("restart-preview")).await;
    assert_eq!(result["success"], true);
    assert_eq!(count, 1);
    let entry = a.entry.clone();
    drop(a);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    std::fs::remove_file(&path).unwrap();
    let mut new = Wallet::new(entry).unwrap();
    new.origin = Some(mock.origin.clone());
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(serve_listener(
        listener,
        Arc::new(BTreeMap::from([("alice".into(), Arc::new(new))])),
    ));
    let p = path.clone();
    let job = tokio::spawn(async move { exchange(&p, prepare("restart")).await });
    prepare_ready(&mut mock).await;
    assert_eq!(job.await.unwrap()["success"], true);
    let result = exchange(&path, trigger("restart")).await;
    assert_eq!(result["success"], false);
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("never resubmitted"),
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
async fn invalid_simulation_or_intent_echo_never_executes() {
    for bad in [
        json!({"simulationSuccess":false}),
        json!({"to":SENDER}),
        json!({"value":"100"}),
        json!({"inputData":"0xdeadbeef"}),
        json!({"binanceChainId":"1"}),
        json!({"from":TARGET}),
        json!({"expiresAt":1}),
    ] {
        let (_root, mut mock, _a, path, server) = prepared_fixture("invalid").await;
        let p = path.clone();
        let mut job = tokio::spawn(async move { exchange(&p, trigger("invalid")).await });
        let mut preview = valid_preview("invalid-preview");
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
async fn oversized_or_unknown_fields_are_rejected_without_wallet_work() {
    let root = tempfile::tempdir().unwrap();
    let (path, server) = start(root.path(), BTreeMap::new()).await;
    let result = exchange(
        &path,
        json!({"op":"trigger","id":"x","name":"alice","operation_id":"x","confirm":true,"call":{}}),
    )
    .await;
    assert_eq!(result["success"], false);
    let mut stream = UnixStream::connect(&path).await.unwrap();
    let _ = stream.write_all(&vec![b'x'; MAX_FRAME + 2]).await;
    let bytes = read_frame(&mut stream, MAX_RESPONSE).await.unwrap();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["success"], false);
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
