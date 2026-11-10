use super::*;
#[test]
fn leases_share_one_endpoint_and_release_it_on_last_drop() {
    let manager = Arc::new(SharedRpc::default());
    let a = manager.acquire("wss://example.com/a").unwrap();
    let b = manager.acquire("wss://example.com/a").unwrap();
    assert!(Arc::ptr_eq(&a.connection, &b.connection));
    assert!(manager.acquire("wss://example.com/b").is_err());
    drop(a);
    assert!(manager.inner.lock().unwrap().is_some());
    drop(b);
    assert!(manager.inner.lock().unwrap().is_none());
    assert!(manager.acquire("wss://example.com/b").is_ok());
}
#[test]
fn rejects_insecure_or_ambiguous_urls() {
    let manager = Arc::new(SharedRpc::default());
    for url in [
        "https://example.com",
        "ws://example.com",
        "wss://u:p@example.com",
        "wss://example.com/#key",
    ] {
        assert!(manager.acquire(url).is_err());
    }
}
#[tokio::test]
async fn concurrent_reads_share_one_socket_and_coalesce_identical_reads() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        socket
            .send(Message::Text(
                json!({"jsonrpc":"2.0","id":request["id"],"result":"0x38"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), socket.next())
                .await
                .is_err()
        );
    });
    let manager = Arc::new(SharedRpc::default());
    let a = manager.acquire(&url).unwrap();
    let b = manager.acquire(&url).unwrap();
    let (ra, rb) = tokio::join!(
        a.read_observed("eth_chainId", json!([])),
        b.read("eth_chainId", json!([]))
    );
    let (value, timing) = ra.unwrap();
    assert_eq!(value, "0x38");
    assert!(!timing.cache_hit);
    assert_eq!(timing.throttle_wait_ms, 0.0);
    assert!(timing.total_ms >= timing.exchange_ms);
    assert_eq!(rb.unwrap(), "0x38");
    let (value, cached) = b.read_observed("eth_chainId", json!([])).await.unwrap();
    assert_eq!(value, "0x38");
    assert!(cached.cache_hit);
    assert_eq!(cached.exchange_ms, 0.0);
    assert_eq!(cached.throttle_wait_ms, 0.0);
    server.await.unwrap();
    assert!(a.read("eth_sendRawTransaction", json!([])).await.is_err());
    drop(a);
    drop(b);
    assert!(manager.inner.lock().unwrap().is_none());
}
#[tokio::test]
async fn notifications_and_reads_share_socket_and_identical_subscriptions_are_reference_counted() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let mut methods = Vec::new();
        while let Some(frame) = socket.next().await {
            let frame = frame.unwrap();
            if !frame.is_text() {
                continue;
            }
            let request: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            let method = request["method"].as_str().unwrap();
            methods.push(method.to_owned());
            let result = match method {
                "eth_subscribe" => json!("sub-head"),
                "eth_chainId" => json!("0x38"),
                "eth_blockNumber" => json!("0x10"),
                "eth_unsubscribe" => json!(true),
                _ => panic!("unexpected {method}"),
            };
            socket
                .send(Message::Text(
                    json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            if method == "eth_chainId" {
                socket.send(Message::Text(json!({"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"sub-head","result":{"number":"0x10"}}}).to_string().into())).await.unwrap();
            }
            if method == "eth_unsubscribe" {
                break;
            }
        }
        assert_eq!(
            methods,
            vec![
                "eth_subscribe",
                "eth_chainId",
                "eth_blockNumber",
                "eth_unsubscribe"
            ]
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let manager = Arc::new(SharedRpc::default());
    let lease = manager.acquire(&url).unwrap();
    let mut a = lease.subscribe(json!(["newHeads"])).await.unwrap();
    let mut b = lease.subscribe(json!(["newHeads"])).await.unwrap();
    let mut legacy = lease.subscribe(json!(["newHeads"])).await.unwrap();
    let before_ingress = Instant::now();
    assert_eq!(lease.read("eth_chainId", json!([])).await.unwrap(), "0x38");
    // The subsequent response is an actor barrier: it must parse the preceding
    // notification before completing this read, independent of socket scheduling.
    assert_eq!(
        lease.read("eth_blockNumber", json!([])).await.unwrap(),
        "0x10"
    );
    let buffered_at = Instant::now();
    tokio::time::sleep(Duration::from_millis(60)).await;
    let observed_a = a.recv_observed().await.unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    let observed_b = b.recv_observed().await.unwrap();
    assert_eq!(observed_a.value["number"], "0x10");
    assert_eq!(observed_a.value, observed_b.value);
    assert_eq!(observed_a.received_at, observed_b.received_at);
    assert!(observed_a.received_at >= before_ingress);
    assert!(observed_a.received_at <= buffered_at);
    assert_eq!(legacy.recv().await.unwrap()["number"], "0x10");
    drop(legacy);
    drop(a);
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(b);
    server.await.unwrap();
}
#[tokio::test]
async fn disconnected_subscription_fails_instead_of_reconnecting() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        socket
            .send(Message::Text(
                json!({"jsonrpc":"2.0","id":request["id"],"result":"s"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let manager = Arc::new(SharedRpc::default());
    let lease = manager.acquire(&url).unwrap();
    let mut sub = lease.subscribe(json!(["newHeads"])).await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(2), sub.recv())
        .await
        .unwrap()
        .is_err());
    server.await.unwrap();
}

#[test]
fn provider_error_text_is_not_forwarded() {
    let reason = rejection_reason(&json!({"code":-32600,"message":
        "Free tier permits a 10 block range; wss://example.com/PRIVATE-KEY"}));
    assert!(reason.contains("-32600"));
    assert!(reason.contains("10 blocks"));
    assert!(!reason.contains("PRIVATE-KEY"));
    assert!(!reason.contains("example.com"));
    let malformed = rejection_reason(&json!({"code":"PRIVATE-KEY","message":"PRIVATE-KEY"}));
    assert_eq!(malformed, "RPC server rejected request");
}

#[tokio::test]
async fn rejected_reads_identify_method_and_code_without_replaying() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        socket
            .send(Message::Text(
                json!({"jsonrpc":"2.0","id":request["id"],"error":
            {"code":-32600,"message":"up to a 10 block range; PRIVATE-KEY"}})
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_millis(150), socket.next()).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let manager = Arc::new(SharedRpc::default());
    let lease = manager.acquire(&url).unwrap();
    let error = lease.read("eth_getLogs", json!([{}])).await.unwrap_err();
    assert!(error.contains("eth_getLogs"));
    assert!(error.contains("-32600"));
    assert!(error.contains("10 blocks"));
    assert!(!error.contains("PRIVATE-KEY"));
    assert!(lease.read("eth_chainId", json!([])).await.is_err());
    server.await.unwrap();
}

#[tokio::test]
async fn observed_reads_separate_wire_queue_limiter_and_exchange() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (arrived_tx, arrived_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let (finish_tx, finish_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let mut arrived_tx = Some(arrived_tx);
        let mut release_rx = Some(release_rx);
        for expected in ["eth_chainId", "eth_blockNumber", "eth_getBlockByNumber"] {
            let request: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(request["method"], expected);
            if let Some(tx) = arrived_tx.take() {
                tx.send(()).unwrap();
                release_rx.take().unwrap().await.unwrap();
            }
            socket
                .send(Message::Text(
                    json!({"jsonrpc":"2.0","id":request["id"],"result":"0x38"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        // Keep the transport alive until the client has consumed all replies.
        finish_rx.await.unwrap();
    });
    let manager = Arc::new(SharedRpc::default());
    let lease = manager.acquire(&url).unwrap();
    let first = lease.read_observed("eth_chainId", json!([]));
    tokio::pin!(first);
    assert!(futures_util::poll!(&mut first).is_pending());
    arrived_rx.await.unwrap();
    let second = lease.read_observed("eth_blockNumber", json!([]));
    tokio::pin!(second);
    // The first read holds the wire lock until the controlled server reply.
    assert!(futures_util::poll!(&mut second).is_pending());
    tokio::time::sleep(Duration::from_millis(80)).await;
    release_tx.send(()).unwrap();
    let (first, second) = tokio::join!(first, second);
    let (_, first) = first.unwrap();
    let (_, second) = second.unwrap();
    assert!(first.exchange_ms >= 60.0);
    assert!(second.queue_wait_ms >= 60.0);
    assert_eq!(first.throttle_wait_ms, 0.0);
    for timing in [&first, &second] {
        assert!(!timing.cache_hit);
        assert!(
            timing.total_ms >= timing.queue_wait_ms + timing.throttle_wait_ms + timing.exchange_ms
        );
    }

    // Reset only the existing limiter's reference instant under the wire lock.
    // Poll immediately to enter its sleep without depending on network latency.
    lease.connection.wire.lock().await.last_request = Some(Instant::now());
    let limited = lease.read_observed("eth_getBlockByNumber", json!(["latest", false]));
    tokio::pin!(limited);
    assert!(futures_util::poll!(&mut limited).is_pending());
    let (_, limited) = limited.await.unwrap();
    assert!(!limited.cache_hit);
    assert!(limited.throttle_wait_ms >= 150.0);
    assert!(limited.total_ms >= limited.throttle_wait_ms + limited.exchange_ms);
    let serialized = serde_json::to_value(&limited).unwrap();
    assert_eq!(serialized.as_object().unwrap().len(), 5);
    assert!(serialized
        .as_object()
        .unwrap()
        .values()
        .all(|v| v.is_number() || v.is_boolean()));
    finish_tx.send(()).unwrap();
    server.await.unwrap();
}
