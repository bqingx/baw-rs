use super::*;
use crate::{client::Client, session::Credentials};
use std::os::unix::fs::PermissionsExt;
use std::{
    io::{BufRead, Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
use tokio::io::AsyncReadExt;
fn wallet() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn buy() -> Buy {
    serde_json::from_value(json!({"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"1","minAmountOut":"0","allowZeroMinOut":true})).unwrap()
}
#[tokio::test]
async fn recorded_binance_preview_shape_is_accepted_and_simulation_code_is_enforced() {
    // Sanitized structure of a real preview response recorded in this conversation;
    // all addresses/amounts are synthetic. Binance does not echo top-level from/chain.
    for (code, succeeds) in [("000000000", true), ("351805", false)] {
        let dir = wallet();
        let p = json!({
            "requestId":format!("recorded-{code}"), "expiresAt":4102444800000u64,
            "parsedTx":{"contractAddress":buy().core,"action":"0x995ee6da"},
            "simulationResult":{
                "simulationCode":code,"simulationErrorDetail":null,
                "authorityChanges":[],"allowanceChanges":[],
                "balanceChanges":[{"owner":buy().from,"contractAddress":buy().token,"change":"123"}]
            },"risks":{"riskDetails":[]},"requireConfirmation":false
        });
        let mut replies = vec![p];
        if succeeds {
            replies.push(json!({"orderId":"recorded-order"}));
        }
        let mut b = backend(replies, 0);
        let output = process(
            &mut b.client,
            dir.path(),
            &request(code).to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(output["success"], succeeds, "{output}");
        b.handle.join().unwrap();
        assert_eq!(b.seen.load(Ordering::SeqCst), if succeeds { 2 } else { 1 });
    }
}

#[tokio::test]
async fn framing_error_drains_inflight_execute_and_emits_its_response() {
    let mut b = backend(
        vec![
            preview("drain"),
            json!({"orderId":"drained","__body_delay_ms":80}),
        ],
        0,
    );
    let dir = wallet();
    let (mut input_writer, input_reader) = tokio::io::duplex(131072);
    let (mut output_reader, output_writer) = tokio::io::duplex(65536);
    let seen = b.seen.clone();
    let producer = async move {
        input_writer
            .write_all(format!("{}\n", request("drain")).as_bytes())
            .await
            .unwrap();
        while seen.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        // Break framing only after execute has reached the server.
        input_writer.write_all(&vec![b'x'; 65537]).await.unwrap();
    };
    let collect = async {
        let mut s = String::new();
        output_reader.read_to_string(&mut s).await.unwrap();
        s
    };
    let (result, _, output) = tokio::join!(
        run_stream(
            &mut b.client,
            dir.path(),
            input_reader,
            output_writer,
            || Ok(())
        ),
        producer,
        collect
    );
    assert!(result.unwrap_err().contains("frame"));
    let row: Value =
        serde_json::from_str(output.trim()).expect("inflight response must be drained");
    assert_eq!(row["success"], true, "{row}");
    b.handle.join().unwrap();
    let journal: Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".baw-rs-operations/drain.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(journal["state"], "RESPONSE_RECEIVED");
}

#[tokio::test]
async fn unrecognized_execute_object_is_unknown_not_success() {
    let mut b = backend(
        vec![preview("unknown-ack"), json!({"unexpected":"payload"})],
        0,
    );
    let dir = wallet();
    let row = process(
        &mut b.client,
        dir.path(),
        &request("unknown-ack").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(row["success"], false, "{row}");
    b.handle.join().unwrap();
    let journal: Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".baw-rs-operations/unknown-ack.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(journal["state"], "FAILED_OR_UNKNOWN");
}

#[tokio::test]
async fn every_simulation_row_requires_affirmative_success() {
    for bad in [
        json!({}),
        json!({"status":"UNKNOWN"}),
        json!({"simulationCode":"351805"}),
    ] {
        let mut p = preview("partial");
        p["simulationResults"] = json!([{"success":true},bad]);
        let mut b = backend(vec![p], 0);
        let dir = wallet();
        let row = process(
            &mut b.client,
            dir.path(),
            &request("partial").to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(row["success"], false, "{row}");
        assert!(
            row["timings"]["execute"]["request_ms"].is_null(),
            "must reject before execute: {row}"
        );
        b.handle.join().unwrap();
        assert_eq!(b.seen.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn pinned_state_is_rechecked_after_preview_before_execute() {
    let mut b = backend(vec![preview("changed")], 0);
    let dir = wallet();
    let input = format!("{}\n", request("changed"));
    let count = AtomicUsize::new(0);
    let result = run_stream(
        &mut b.client,
        dir.path(),
        input.as_bytes(),
        tokio::io::sink(),
        || {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err("session changed during preview".into())
            }
        },
    )
    .await;
    // The line carries the failure; framing/worker transport itself remains valid.
    result.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    b.handle.join().unwrap();
    assert_eq!(b.seen.load(Ordering::SeqCst), 1);
}

fn request(id: &str) -> Value {
    json!({"operationId":id,"confirm":true,"skipPrechecks":true,"buy":buy()})
}
fn preview(id: &str) -> Value {
    json!({"requestId":id,"from":buy().from,"binanceChainId":"56","expiresAt":4102444800u64,"simulationResult":{"success":true,"authorityChanges":["nested"],"allowanceChanges":["retained"]}})
}
struct Backend {
    client: Client,
    origin: String,
    requests: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
    accepts: Arc<AtomicUsize>,
    seen: Arc<AtomicUsize>,
    handle: std::thread::JoinHandle<()>,
}
// A real HTTP/1.1 keep-alive server; only test code can override production origin.
fn backend(replies: Vec<Value>, delay_ms: u64) -> Backend {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let accepts = Arc::new(AtomicUsize::new(0));
    let count = accepts.clone();
    let seen = Arc::new(AtomicUsize::new(0));
    let requests = seen.clone();
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_thread = captured.clone();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        count.fetch_add(1, Ordering::SeqCst);
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream.set_nodelay(true).unwrap();
        let mut reader = std::io::BufReader::new(stream);
        for reply in replies {
            let mut length = 0;
            let mut path = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    let (stream, _) = listener.accept().unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    stream.set_nodelay(true).unwrap();
                    reader = std::io::BufReader::new(stream);
                    continue;
                }
                if line.starts_with("POST ") || line.starts_with("GET ") {
                    path = line.split_whitespace().nth(1).unwrap().to_owned();
                }
                if line == "\r\n" {
                    break;
                }
                if let Some(s) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = s.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            captured_thread
                .lock()
                .unwrap()
                .push((path, serde_json::from_slice(&body).unwrap_or(Value::Null)));
            requests.fetch_add(1, Ordering::SeqCst);
            if reply["__disconnect"] == true {
                drop(reader);
                std::thread::sleep(std::time::Duration::from_millis(100));
                listener.set_nonblocking(true).unwrap();
                while listener.accept().is_ok() {
                    count.fetch_add(1, Ordering::SeqCst);
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let body = json!({"code":"000000","data":reply}).to_string();
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .unwrap();
            reader.get_mut().flush().unwrap();
            if let Some(delay) = reply["__body_delay_ms"].as_u64() {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            reader.get_mut().write_all(body.as_bytes()).unwrap();
            reader.get_mut().flush().unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        while listener.accept().is_ok() {
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    let client = Client::test_client(
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic".into()),
            session_id: zeroize::Zeroizing::new("synthetic".into()),
        },
        &origin,
    )
    .unwrap();
    Backend {
        client,
        origin,
        requests: captured,
        accepts,
        seen,
        handle,
    }
}
#[tokio::test]
async fn persistent_jsonl_reuses_one_tcp_connection_and_measures_queue() {
    let mut server = backend(
        vec![
            preview("a"),
            json!({"orderId":"a"}),
            preview("b"),
            json!({"orderId":"b"}),
        ],
        20,
    );
    let dir = wallet();
    let input = format!("{}\n{}\n", request("a"), request("b"));
    let (mut output_reader, output_writer) = tokio::io::duplex(65536);
    let producer = async {
        let mut output = String::new();
        output_reader.read_to_string(&mut output).await.unwrap();
        output
    };
    let worker = run_stream(
        &mut server.client,
        dir.path(),
        input.as_bytes(),
        output_writer,
        || Ok(()),
    );
    let (result, output) = tokio::join!(worker, producer);
    result.unwrap();
    server.handle.join().unwrap();
    let rows: Vec<Value> = output
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["success"], true, "{output}");
    assert_eq!(rows[1]["success"], true, "{output}");
    assert_eq!(server.accepts.load(Ordering::SeqCst), 1);
    assert_eq!(server.seen.load(Ordering::SeqCst), 4);
    assert!(
        rows[1]["timings"]["queue_ms"].as_f64().unwrap() >= 30.0,
        "{output}"
    );
    assert!(
        rows[0]["timings"]["preview"]["request_ms"]
            .as_f64()
            .unwrap()
            >= 15.0
    );
    assert_eq!(
        rows[0]["data"]["preview"]["simulationResult"]["authorityChanges"],
        json!(["nested"])
    );
    assert!(rows[0]["timings"]["encode_ms"].is_number());
    assert!(rows[0]["timings"]["local_prep_ms"].is_number());
    let requests = server.requests.lock().unwrap();
    for (i, (path, body)) in requests.iter().enumerate() {
        assert!(
            path.ends_with(if i % 2 == 0 {
                "/contract-call/preview"
            } else {
                "/contract-call/execute"
            }),
            "unexpected quote/RPC/precheck: {path}"
        );
        if i % 2 == 0 {
            assert_eq!(*body, build(&buy()).unwrap());
        } else {
            assert_eq!(*body, json!({"requestId":if i == 1 {"a"} else {"b"}}));
        }
    }
    assert!(std::fs::read_dir(dir.path().join("signing-previews"))
        .unwrap()
        .all(|e| e.unwrap().path().extension().unwrap() == "consumed"));
    println!("MOCK_ONLY accepts=1 requests=4 output={output}");
}
#[tokio::test]
async fn cold_clients_need_two_sockets_warm_client_needs_one() {
    for cold in [true, false] {
        let mut server = backend(
            vec![
                preview("c1"),
                json!({"orderId":"c1"}),
                preview("c2"),
                json!({"orderId":"c2"}),
            ],
            5,
        );
        let dir = wallet();
        let one = process(
            &mut server.client,
            dir.path(),
            &request("c1").to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(one["success"], true, "{one}");
        if cold {
            server.client = Client::test_client(
                Credentials {
                    client_id: zeroize::Zeroizing::new("synthetic".into()),
                    session_id: zeroize::Zeroizing::new("synthetic".into()),
                },
                &server.origin,
            )
            .unwrap();
        }
        let two = process(
            &mut server.client,
            dir.path(),
            &request("c2").to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(two["success"], true, "{two}");
        server.handle.join().unwrap();
        assert_eq!(
            server.accepts.load(Ordering::SeqCst),
            if cold { 2 } else { 1 }
        );
        assert_eq!(server.seen.load(Ordering::SeqCst), 4);
        println!(
            "COLD_WARM_MOCK {}",
            json!({"coldClientPerBuy":cold,"acceptedSockets":server.accepts.load(Ordering::SeqCst),"requests":4,"timings":[one["timings"].clone(),two["timings"].clone()]})
        );
    }
}
#[tokio::test]
async fn fast_path_ignores_poisoned_disk_preview_and_retains_only_consumed_marker() {
    use sha2::{Digest, Sha256};
    let mut server = backend(vec![preview("poison"), json!({"orderId":"poison"})], 0);
    let dir = wallet();
    let cache = dir.path().join("signing-previews");
    std::fs::create_dir(&cache).unwrap();
    std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = cache.join(format!("{:x}.json", Sha256::digest(b"poison")));
    std::fs::write(&path, "invalid cache sentinel: never read or overwrite").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let row = process(
        &mut server.client,
        dir.path(),
        &request("poison").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(row["success"], true, "{row}");
    server.handle.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "invalid cache sentinel: never read or overwrite"
    );
    assert_eq!(std::fs::read_dir(cache).unwrap().count(), 2);
}
#[tokio::test]
async fn failed_simulation_or_expiry_never_reaches_execute() {
    for key in ["simulation", "expiry"] {
        let mut p = preview(key);
        if key == "simulation" {
            p["simulationResult"]["success"] = json!(false);
        } else {
            p["expiresAt"] = json!(1);
        }
        let mut server = backend(vec![p], 0);
        let dir = wallet();
        let row = process(
            &mut server.client,
            dir.path(),
            &request(key).to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(row["success"], false, "{row}");
        server.handle.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].0.ends_with("/preview"));
    }
}
#[tokio::test]
async fn malformed_execute_is_unknown_and_both_operation_and_request_replay_are_blocked() {
    let mut server = backend(vec![preview("same"), Value::Null, preview("same")], 0);
    let dir = wallet();
    let first = process(
        &mut server.client,
        dir.path(),
        &request("one").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(
        first["success"], false,
        "malformed execute must not be success: {first}"
    );
    assert_eq!(
        first["preview"]["simulationResult"]["authorityChanges"],
        json!(["nested"]),
        "error output must retain preview review data"
    );
    let retry = process(
        &mut server.client,
        dir.path(),
        &request("one").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(retry["success"], false);
    let new_id = process(
        &mut server.client,
        dir.path(),
        &request("two").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(new_id["success"], false);
    server.handle.join().unwrap();
    assert_eq!(server.seen.load(Ordering::SeqCst), 3);
    let row: Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".baw-rs-operations/one.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(row["state"], "FAILED_OR_UNKNOWN");
}
#[test]
fn zero_minimum_requires_separate_explicit_opt_in() {
    let mut v = serde_json::to_value(buy()).unwrap();
    v.as_object_mut().unwrap().remove("allowZeroMinOut");
    let a: Buy = serde_json::from_value(v.clone()).unwrap();
    assert!(
        build(&a).is_err(),
        "zero minimum must not silently disable price protection"
    );
    v["allowZeroMinOut"] = json!(true);
    assert!(build(&serde_json::from_value(v).unwrap()).is_ok());
}
#[test]
fn full_width_boundaries_and_strict_schema() {
    let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let mut a = buy();
    a.budget = max.into();
    a.min_amount_out = format!("0x{}", "f".repeat(64));
    let tx = build(&a).unwrap();
    let data = tx["inputData"].as_str().unwrap();
    assert_eq!(&data[74..138], "f".repeat(64));
    assert_eq!(&data[138..202], "f".repeat(64));
    assert_eq!(&data[202..266], "0".repeat(64));
    assert_eq!(&data[266..330], format!("{:064x}", 160));
    assert_eq!(&data[330..394], "0".repeat(64));
    for bad in [
        "",
        "-1",
        "+1",
        " 1",
        "1.0",
        "1e18",
        "0x",
        "0X1",
        "１２",
        "115792089237316195423570985008687907853269984665640564039457584007913129639936",
    ] {
        a.min_amount_out = bad.into();
        assert!(build(&a).is_err(), "{bad}");
    }
    a = buy();
    a.budget = "0".into();
    assert!(build(&a).is_err());
    for bad in [
        "0x0000000000000000000000000000000000000000",
        "0x111",
        "0xZZ11111111111111111111111111111111111111",
        " 0x1111111111111111111111111111111111111111",
    ] {
        a = buy();
        a.core = bad.into();
        assert!(build(&a).is_err());
    }
    let mut v = serde_json::to_value(buy()).unwrap();
    v.as_object_mut().unwrap().remove("minAmountOut");
    assert!(serde_json::from_value::<Buy>(v).is_err());
    let mut v = request("x");
    v["sessionId"] = json!("not-allowed");
    assert!(serde_json::from_value::<Request>(v).is_err());
}
#[tokio::test]
async fn malformed_lines_do_not_gain_consent_or_block_next_request() {
    let mut server = backend(vec![preview("good"), json!({"orderId":"good"})], 0);
    let dir = wallet();
    let mut missing = request("missing");
    missing.as_object_mut().unwrap().remove("confirm");
    let mut numeric = request("numeric");
    numeric["buy"]["budget"] = json!(1);
    let mut unknown = request("unknown");
    unknown["sessionId"] = json!("secret-not-reflected");
    let input = format!(
        "{{bad\n{}\n{}\n{}\n{}\n",
        missing,
        numeric,
        unknown,
        request("good")
    );
    let (mut reader, writer) = tokio::io::duplex(65536);
    let collect = async {
        let mut s = String::new();
        reader.read_to_string(&mut s).await.unwrap();
        s
    };
    let (result, output) = tokio::join!(
        run_stream(
            &mut server.client,
            dir.path(),
            input.as_bytes(),
            writer,
            || Ok(())
        ),
        collect
    );
    result.unwrap();
    server.handle.join().unwrap();
    let rows: Vec<Value> = output
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 5);
    assert!(rows[..4].iter().all(|v| v["success"] == false));
    assert_eq!(rows[4]["success"], true);
    assert!(!output.contains("secret-not-reflected"));
    assert_eq!(server.seen.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn transport_limits_and_state_revalidation_fail_closed() {
    let credentials = || Credentials {
        client_id: zeroize::Zeroizing::new("synthetic".into()),
        session_id: zeroize::Zeroizing::new("synthetic".into()),
    };
    let mut client = Client::test_client(credentials(), "http://127.0.0.1:1").unwrap();
    let dir = wallet();
    for input in ["x".repeat(65537), "{}".into()] {
        let err = run_stream(
            &mut client,
            dir.path(),
            input.as_bytes(),
            tokio::io::sink(),
            || Ok(()),
        )
        .await
        .unwrap_err();
        assert!(err.contains("frame"));
    }
    let input = format!("{}\n", request("x"));
    assert!(run_stream(
        &mut client,
        dir.path(),
        input.as_bytes(),
        tokio::io::sink(),
        || Err("session changed".into())
    )
    .await
    .unwrap_err()
    .contains("session changed"));
    assert!(!dir.path().join(".baw-rs-operations").exists());
}
#[test]
fn pinned_stamp_rejects_symlink_permissions_and_detects_replacement() {
    let dir = wallet();
    let path = dir.path().join("synthetic");
    std::fs::write(&path, "one").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let first = stamp(&path).unwrap();
    let second = dir.path().join("replacement");
    std::fs::write(&second, "two").unwrap();
    std::fs::set_permissions(&second, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::rename(&second, &path).unwrap();
    assert_ne!(first, stamp(&path).unwrap());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(stamp(&path).is_err());
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(stamp(&link).is_err());
}
#[tokio::test]
#[ignore = "local mock benchmark, not Binance speed; run with --release --ignored --nocapture"]
async fn benchmark_fast_mock() {
    for delay_ms in [0, 5] {
        let n = 100;
        let mut replies = Vec::new();
        for i in 0..n {
            replies.push(preview(&format!("b{i}")));
            replies.push(json!({"orderId":format!("b{i}")}));
        }
        let mut server = backend(replies, delay_ms);
        let dir = wallet();
        let mut samples = Vec::new();
        for i in 0..n {
            let row = process(
                &mut server.client,
                dir.path(),
                &request(&format!("b{i}")).to_string(),
                Instant::now(),
            )
            .await;
            assert_eq!(row["success"], true, "{row}");
            samples.push(row["timings"].clone());
        }
        server.handle.join().unwrap();
        assert_eq!(server.accepts.load(Ordering::SeqCst), 1);
        assert_eq!(server.seen.load(Ordering::SeqCst), 2 * n);
        println!(
            "MOCK_BENCH {}",
            json!({"mockDelayPerHttpMs":delay_ms,"scope":"loopback HTTP/1.1, native resident handler, durable journal + consumed markers, NOT Binance speed, no TLS","tcpAccepts":server.accepts.load(Ordering::SeqCst),"httpRequests":server.seen.load(Ordering::SeqCst),"orders":n,"samples":samples})
        );
    }
}
#[tokio::test]
async fn uncertain_execute_is_not_retried_and_keeps_durable_unknown_state() {
    let mut server = backend(vec![preview("uncertain"), json!({"__disconnect":true})], 0);
    let dir = wallet();
    let req = request("uncertain").to_string();
    let row = process(&mut server.client, dir.path(), &req, Instant::now()).await;
    assert_eq!(row["success"], false);
    assert!(row["preview"].is_object());
    assert_eq!(
        process(&mut server.client, dir.path(), &req, Instant::now()).await["success"],
        false
    );
    server.handle.join().unwrap();
    assert_eq!(server.accepts.load(Ordering::SeqCst), 1);
    assert_eq!(server.seen.load(Ordering::SeqCst), 2);
    let state: Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".baw-rs-operations/uncertain.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["state"], "FAILED_OR_UNKNOWN");
}
#[tokio::test]
async fn body_time_is_not_misreported_as_backend_processing_or_request_latency() {
    let mut p = preview("slow-body");
    p["__body_delay_ms"] = json!(25);
    let mut server = backend(vec![p, json!({"orderId":"body","__body_delay_ms":25})], 0);
    let dir = wallet();
    let row = process(
        &mut server.client,
        dir.path(),
        &request("body").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(row["success"], true, "{row}");
    for key in ["preview", "execute"] {
        assert!(row["timings"][key]["body_ms"].as_f64().unwrap() >= 20.0);
        assert!(
            row["timings"][key]["full_roundtrip_ms"]
                .as_f64()
                .unwrap_or(0.0)
                >= 20.0,
            "full HTTP roundtrip must include body drain: {row}"
        );
    }
    assert!(row["timings"].get("backendProcessingMs").is_none());
    server.handle.join().unwrap();
}
#[tokio::test]
async fn concurrent_requests_keep_skip_prechecks_local() {
    let mut fast = backend(vec![preview("skip"), json!({"orderId":"fast"})], 10);
    let gates = || {
        vec![
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":buy().from}]}]}]),
            json!([]),
        ]
    };
    let mut replies = gates();
    replies.push(preview("strict"));
    replies.extend(gates());
    replies.push(json!({"orderId":"strict"}));
    let mut strict = backend(replies, 0);
    let a = wallet();
    let b = wallet();
    let fast_req = request("skip").to_string();
    let mut strict_req = request("strict");
    strict_req["skipPrechecks"] = json!(false);
    let strict_req = strict_req.to_string();
    let (x, y) = tokio::join!(
        process(&mut fast.client, a.path(), &fast_req, Instant::now()),
        process(&mut strict.client, b.path(), &strict_req, Instant::now())
    );
    assert_eq!(x["success"], true, "{x}");
    assert_eq!(y["success"], true, "{y}");
    fast.handle.join().unwrap();
    strict.handle.join().unwrap();
    assert_eq!(fast.seen.load(Ordering::SeqCst), 2);
    assert_eq!(strict.seen.load(Ordering::SeqCst), 8);
}
#[tokio::test]
async fn backend_echo_must_match_exact_transaction_intent() {
    for bad in [
        json!({"to":"0x4444444444444444444444444444444444444444"}),
        json!({"tx":{"to":"0x4444444444444444444444444444444444444444"}}),
        json!({"transaction":{"value":"999"}}),
        json!({"unsignedTx":{"inputData":"0xdeadbeef"}}),
        json!({"parsedTx":{"contractAddress":"0x4444444444444444444444444444444444444444"}}),
    ] {
        let mut p = preview("binding");
        p.as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        let mut server = backend(vec![p], 0);
        let dir = wallet();
        let row = process(
            &mut server.client,
            dir.path(),
            &request("binding").to_string(),
            Instant::now(),
        )
        .await;
        assert_eq!(
            row["success"], false,
            "mismatched backend intent must fail before execute: {row}"
        );
        server.handle.join().unwrap();
        assert_eq!(server.seen.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn explicit_consent_and_replay_fail_before_extra_network() {
    let mut server = backend(vec![preview("one"), json!({"orderId":"ok"})], 0);
    let dir = wallet();
    let mut no = request("no");
    no["confirm"] = json!(false);
    let denied = process(
        &mut server.client,
        dir.path(),
        &no.to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(denied["success"], false);
    assert!(!dir.path().join(".baw-rs-operations").exists());
    let first = process(
        &mut server.client,
        dir.path(),
        &request("one").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(first["success"], true, "{first}");
    server.handle.join().unwrap();
    let again = process(
        &mut server.client,
        dir.path(),
        &request("one").to_string(),
        Instant::now(),
    )
    .await;
    assert_eq!(again["success"], false);
    assert_eq!(server.seen.load(Ordering::SeqCst), 2);
    let row: Value = serde_json::from_slice(
        &std::fs::read(dir.path().join(".baw-rs-operations/one.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(row["state"], "RESPONSE_RECEIVED");
}
