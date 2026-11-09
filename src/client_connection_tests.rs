use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn rejected_request_cannot_inherit_a_previous_execute_observation() {
    let mut client = Client::test_client(
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        },
        "http://127.0.0.1:9",
    )
    .unwrap();
    client.last_http = HttpTiming {
        endpoint: Some("contract_execute"),
        request_ms: Some(123.0),
        ..Default::default()
    };
    client.invalidate();
    assert!(client
        .request(
            Method::POST,
            &format!("{API}/contract-call/execute"),
            Some(json!({"requestId":"synthetic"}))
        )
        .await
        .is_err());
    assert!(client.last_http.endpoint.is_none());
    assert!(client
        .http_timing_for("contract_execute")
        .connection
        .is_none());
    assert!(client
        .http_timing_for("contract_execute")
        .request_ms
        .is_none());
    client.last_http = HttpTiming {
        endpoint: Some("other"),
        request_ms: Some(456.0),
        ..Default::default()
    };
    assert!(client
        .http_timing_for("contract_execute")
        .request_ms
        .is_none());
}

#[tokio::test]
async fn binance_client_reports_socket_reuse_without_serializing_credentials_or_addresses() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        for _ in 0..2 {
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut buffer = [0u8; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0, "reusable client closed the socket");
                request.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.contains("synthetic-secret-session"));
            assert!(request.starts_with(&format!("GET {API}/settings/query HTTP/1.1")));
            let body = r#"{"code":"000000","data":{"ok":true}}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    let mut client = Client::test_client(
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-private-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-secret-session".into()),
        },
        &origin,
    )
    .unwrap();
    let first = client.observe_readonly_settings().await.unwrap();
    let second = client.observe_readonly_settings().await.unwrap();
    client.begin_policy_snapshot();
    assert!(client.observe_readonly_settings().await.is_err());
    client.end_policy_snapshot();
    assert!(first["connection"]["socket_id"].is_u64());
    assert_eq!(
        first["connection"]["socket_id"],
        second["connection"]["socket_id"]
    );
    assert_eq!(
        first["connection"]["pool_id"],
        second["connection"]["pool_id"]
    );
    assert_eq!(second["connection"]["socket_seen_before"], true);
    let serialized = serde_json::to_string(&second).unwrap();
    for secret in [
        "synthetic-secret-session",
        "synthetic-private-client",
        "127.0.0.1",
        &origin,
        "agentSessionId",
        "cookie",
    ] {
        assert!(
            !serialized.contains(secret),
            "observation serialized private transport data"
        );
    }
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
