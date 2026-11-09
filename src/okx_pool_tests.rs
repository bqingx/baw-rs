use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn preparation_and_trigger_reuse_one_http_pool_without_sticking_credentials() {
    let (dir, args, _) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        for key in ["synthetic-key", "synthetic-replaced-api-key"] {
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut buffer = [0u8; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0, "client closed the reusable connection");
                bytes.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
            assert!(request.contains(&format!("ok-access-key: {key}")));
            assert!(!request.contains("agentclientid:"));
            assert!(!request.contains("cookie:"));
            let body = r#"{"code":"0","data":[]}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "second TCP connection opened"
        );
    });
    let credentials = || crate::session::Credentials {
        client_id: zeroize::Zeroizing::new("synthetic-wallet-client".into()),
        session_id: zeroize::Zeroizing::new("synthetic-wallet-session".into()),
    };
    let mut client = Client::test_client(credentials(), "http://127.0.0.1:9").unwrap();
    let transport = client.okx_transport().unwrap();
    let mut first = Okx::load_with_http(&args.quote, transport.clone()).unwrap();
    first.origin = origin.clone();
    let (_, first_observation) = first
        .get_observed("quote", &[("chainIndex", "56".into())])
        .await
        .unwrap();
    drop(first);
    private(
        &dir.path().join("credentials.json"),
        &json!({"api_key":"synthetic-replaced-api-key","secret_key":"synthetic-new-secret","passphrase":"synthetic-new-pass"}),
    );
    let mut second = Okx::load_with_http(&args.quote, client.okx_transport().unwrap()).unwrap();
    second.origin = origin;
    assert!(Arc::ptr_eq(&transport, &second.http));
    let (_, second_observation) = second
        .get_observed("quote", &[("chainIndex", "56".into())])
        .await
        .unwrap();
    let first_observation = serde_json::to_value(first_observation).unwrap();
    let second_observation = serde_json::to_value(second_observation).unwrap();
    assert_eq!(first_observation["pool_id"], second_observation["pool_id"]);
    assert!(first_observation["socket_id"].is_u64());
    assert_eq!(
        first_observation["socket_id"],
        second_observation["socket_id"]
    );
    assert_eq!(second_observation["socket_seen_before"], true);
    let mut other = Client::test_client(credentials(), "http://127.0.0.1:9").unwrap();
    assert!(
        !Arc::ptr_eq(&transport, &other.okx_transport().unwrap()),
        "HTTP pools crossed wallet clients"
    );
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn local_route_validation_does_not_reload_credentials_or_build_a_transport() {
    let (dir, args, api) = fixture();
    let config = api.route_config(&args.quote, FROM).unwrap();
    let raw = response();
    let validated = route_action::RouteActionBuilder::validate_only(&config, &raw).unwrap();
    std::fs::remove_file(dir.path().join("credentials.json")).unwrap();
    assert_eq!(
        route_action::RouteActionBuilder::validate_only(&config, &raw).unwrap(),
        validated
    );
    let mut bad = raw;
    bad["data"][0]["tx"]["to"] = json!(INPUT);
    assert!(route_action::RouteActionBuilder::validate_only(&config, &bad).is_err());
}
