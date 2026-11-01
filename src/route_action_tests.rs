use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn word(s: &str) -> String {
    format!("{:0>64}", s)
}
fn calldata(minimum: u64) -> String {
    format!(
        "0x38ed1739{}{}{}{}{}{}{}{}",
        word(&format!("{:x}", 10000000000000000001u64)),
        word(&format!("{minimum:x}")),
        word("a0"),
        word(&WALLET[2..]),
        word("ffffffffffffffff"),
        word("2"),
        word(&INPUT[2..]),
        word(&OUTPUT[2..])
    )
}
fn response(minimum: u64) -> Value {
    json!({"code":"0","data":[{"routerResult":{"chainIndex":"56","fromTokenAmount":"10000000000000000001",
      "toTokenAmount":minimum.saturating_add(1).to_string(),"fromToken":{"tokenContractAddress":INPUT},"toToken":{"tokenContractAddress":OUTPUT}},
      "tx":{"from":WALLET,"to":ROUTER,"value":"0","gas":"200000","gasPrice":"1000000000",
      "minReceiveAmount":minimum.to_string(),"data":calldata(minimum)}}]})
}
async fn server(
    replies: Vec<(u16, String, String)>,
) -> (
    String,
    tokio::sync::mpsc::UnboundedReceiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let h = tokio::spawn(async move {
        for (status, extra, body) in replies {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut data = Vec::new();
            loop {
                let mut b = [0; 1024];
                let n = s.read(&mut b).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&b[..n]);
                if data.windows(4).any(|b| b == b"\r\n\r\n") {
                    break;
                }
            }
            tx.send(String::from_utf8(data).unwrap()).ok();
            let r=format!("HTTP/1.1 {status} test\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",body.len());
            s.write_all(r.as_bytes()).await.ok();
        }
    });
    (format!("http://{addr}"), rx, h)
}

#[tokio::test]
async fn build_only_fetches_fresh_swap_and_authenticates_exact_query() {
    use base64::Engine;
    let (_f, c) = fixture();
    let (origin, mut rx, h) = server(vec![
        (200, String::new(), response(99).to_string()),
        (200, String::new(), response(98).to_string()),
    ])
    .await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(rx.try_recv().is_err());
    let call = b.build(&json!({"event":"first"})).await.unwrap();
    assert_eq!(call["input_data"], calldata(99));
    assert_eq!(call["gas_limit"], 200000);
    assert_eq!(call["route_validation"]["recipient_verified"], true);
    let req = rx.recv().await.unwrap();
    let first = req.lines().next().unwrap();
    let target = first.split_whitespace().nth(1).unwrap();
    assert!(target.starts_with("/api/v6/dex/aggregator/swap?"));
    let params: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(target.split_once('?').unwrap().1.as_bytes())
            .into_owned()
            .collect();
    assert_eq!(params["amount"], "10000000000000000001");
    assert_eq!(params["userWalletAddress"], WALLET);
    assert_eq!(params["swapMode"], "exactIn");
    assert_eq!(params["autoSlippage"], "false");
    assert_eq!(params["slippagePercent"], "0.5");
    let headers: std::collections::HashMap<_, _> = req
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    assert_eq!(headers["ok-access-key"], "synthetic-key");
    assert_eq!(headers["ok-access-passphrase"], "synthetic-pass");
    let timestamp = &headers["ok-access-timestamp"];
    assert!(chrono::DateTime::parse_from_rfc3339(timestamp).is_ok());
    let expected = base64::engine::general_purpose::STANDARD.encode(hmac_sha256(
        b"synthetic-secret",
        format!("{timestamp}GET{target}").as_bytes(),
    ));
    assert_eq!(headers["ok-access-sign"], expected);
    let second = b.build(&json!({"event":"second"})).await.unwrap();
    assert_eq!(second["input_data"], calldata(98));
    assert_ne!(second["input_data"], call["input_data"]);
    h.await.unwrap();
}

#[test]
fn decoded_v2_binds_amount_path_wallet_deadline_and_minimum() {
    let (_f, c) = fixture();
    let b = RouteActionBuilder::new(&c).unwrap();
    for (idx, replacement) in [
        (0, word("1")),
        (1, word("1")),
        (2, word("80")),
        (3, word(&ROUTER[2..])),
        (3, format!("1{}", &word(&WALLET[2..])[1..])),
        (4, word("1")),
        (5, word("1")),
        (6, word(&OUTPUT[2..])),
        (7, word(&INPUT[2..])),
    ] {
        let mut r = response(99);
        let mut data = calldata(99);
        data.replace_range(10 + idx * 64..10 + (idx + 1) * 64, &replacement);
        r["data"][0]["tx"]["data"] = json!(data);
        assert!(
            b.validate_response(&r).is_err(),
            "accepted invalid word {idx}"
        );
    }
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!(format!("{}00", calldata(99)));
    assert!(b.validate_response(&r).is_err());
    let mut r = response(99);
    r["data"][0]["tx"]
        .as_object_mut()
        .unwrap()
        .remove("minReceiveAmount");
    assert!(
        b.validate_response(&r).is_err(),
        "quote amount is not a minimum"
    );
    assert!(b.validate_response(&response(0)).is_err());
    let mut c = c;
    c["allow_zero_min_out"] = json!(true);
    assert!(RouteActionBuilder::new(&c)
        .unwrap()
        .validate_response(&response(0))
        .is_ok());
}

#[test]
fn response_echoes_and_gas_units_are_fail_closed() {
    let (_f, c) = fixture();
    let b = RouteActionBuilder::new(&c).unwrap();
    let cases = [
        ("/data/0/routerResult/chainIndex", json!("1")),
        ("/data/0/routerResult/fromTokenAmount", json!("1")),
        (
            "/data/0/routerResult/fromToken/tokenContractAddress",
            json!(OUTPUT),
        ),
        (
            "/data/0/routerResult/toToken/tokenContractAddress",
            json!(INPUT),
        ),
        ("/data/0/tx/chainId", json!("1")),
        ("/data/0/tx/from", json!(ROUTER)),
        ("/data/0/tx/to", json!(WALLET)),
        ("/data/0/tx/gas", json!("200000.1")),
        ("/data/0/tx/gas", json!("05000")),
        ("/data/0/tx/gas", json!("500001")),
        ("/data/0/tx/gas", json!(0)),
        ("/data/0/tx/gasLimit", json!("200001")),
        ("/data/0/tx/gasPrice", json!("1e9")),
        ("/data/0/tx/maxFeePerGas", json!("1")),
        ("/data/0/tx/value", json!("1")),
        ("/data/0/tx/recipient", json!(ROUTER)),
        ("/data/0/approveAddress", json!(WALLET)),
        ("/data/0/tx/minReceiveAmount", json!(null)),
    ];
    for (pointer, value) in cases {
        let mut r = response(99);
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        r.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(b.validate_response(&r).is_err(), "accepted {pointer}");
    }
}

#[test]
fn opaque_allowlisted_calldata_requires_preview_and_never_claims_verified_recipient_or_minimum() {
    let (_f, mut c) = fixture();
    c["allowed_selectors"] = json!(["0x12345678"]);
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!("0x123456780000");
    r["data"][0]["tx"]["recipient"] = json!(WALLET);
    assert!(RouteActionBuilder::new(&c)
        .unwrap()
        .validate_response(&r)
        .is_err());
    c["require_runtime_preview"] = json!(true);
    let b = RouteActionBuilder::new(&c).unwrap();
    let call = b.validate_response(&r).unwrap();
    assert_eq!(call["route_validation"]["requires_runtime_preview"], true);
    assert_eq!(call["route_validation"]["recipient_verified"], false);
    assert_eq!(call["route_validation"]["calldata_min_out_verified"], false);
    assert!(!call["route_validation"]["limitations"]
        .as_array()
        .unwrap()
        .is_empty());
    r["data"][0]["tx"]
        .as_object_mut()
        .unwrap()
        .remove("minReceiveAmount");
    assert!(b.validate_response(&r).is_err());
}

#[tokio::test]
async fn retries_only_readonly_500_and_429_with_new_auth_each_time() {
    let (_f, c) = fixture();
    let (origin, mut rx, h) = server(vec![
        (500, String::new(), "synthetic-secret".into()),
        (429, "Retry-After: 0\r\n".into(), "rate limited".into()),
        (200, String::new(), response(99).to_string()),
    ])
    .await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(b.build(&json!({})).await.is_ok());
    h.await.unwrap();
    let mut stamps = Vec::new();
    for _ in 0..3 {
        let r = rx.recv().await.unwrap();
        assert!(r.starts_with("GET /api/v6/dex/aggregator/swap?"));
        stamps.push(
            r.lines()
                .find(|l| l.starts_with("ok-access-timestamp:"))
                .unwrap()
                .to_string(),
        );
    }
    assert_ne!(stamps[0], stamps[1]);
    assert_ne!(stamps[1], stamps[2]);
}

#[tokio::test]
async fn rejects_redirects_without_forwarding_authentication() {
    let (_f, c) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dest = format!("http://{}/steal", listener.local_addr().unwrap());
    let (origin, mut rx, h) =
        server(vec![(302, format!("Location: {dest}\r\n"), String::new())]).await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(b.build(&json!({})).await.unwrap_err().contains("302"));
    h.await.unwrap();
    assert!(rx.recv().await.is_some());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_json_business_errors_and_4xx_never_retry_or_echo_server_secrets() {
    let (_f, c) = fixture();
    for (status, body) in [
        (200, "synthetic-secret not json".to_string()),
        (
            200,
            json!({"code":"51000","msg":"synthetic-secret","data":[]}).to_string(),
        ),
        (200, json!({"code":0,"data":[]}).to_string()),
        (200, json!({"code":"0","data":[{},{}]}).to_string()),
        (200, json!({"code":"0","data":[{"tx":null}]}).to_string()),
        (401, "synthetic-secret".into()),
    ] {
        let (origin, mut rx, h) = server(vec![(status, String::new(), body)]).await;
        let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
        let e = b.build(&json!({})).await.unwrap_err();
        assert!(!e.contains("synthetic"));
        assert!(
            !e.contains("request failed"),
            "retried invalid response: {e}"
        );
        h.await.unwrap();
        assert!(rx.recv().await.is_some());
        assert!(rx.try_recv().is_err());
    }
}

#[tokio::test]
async fn retries_are_bounded_and_retry_after_never_shortened() {
    let (_f, c) = fixture();
    let (origin, mut rx, h) = server(vec![(500, String::new(), String::new()); 3]).await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(b
        .build(&json!({}))
        .await
        .unwrap_err()
        .contains("retry budget exhausted"));
    h.await.unwrap();
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    assert_eq!(count, 3);
    let (origin, mut rx, h) =
        server(vec![(429, "Retry-After: 100\r\n".into(), String::new())]).await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(b.build(&json!({})).await.unwrap_err().contains("budget"));
    h.await.unwrap();
    assert!(rx.recv().await.is_some());
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn absolute_deadline_and_max_wait_cancel_retry_sleep_before_another_request() {
    let (_f, mut c) = fixture();
    c["deadline_unix_ms"] = json!(unix_ms().unwrap() - 1);
    let (origin, mut rx, h) = server(vec![(200, String::new(), response(99).to_string())]).await;
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    assert!(b.build(&json!({})).await.unwrap_err().contains("deadline"));
    assert!(rx.try_recv().is_err());
    h.abort();
    for absolute in [true, false] {
        c["deadline_unix_ms"] = json!(if absolute {
            unix_ms().unwrap() + 30
        } else {
            u64::MAX
        });
        c["max_wait_ms"] = json!(if absolute { 1000 } else { 30 });
        c["retry_backoff_ms"] = json!(200);
        let (origin, mut rx, h) = server(vec![
            (500, String::new(), String::new()),
            (200, String::new(), response(99).to_string()),
        ])
        .await;
        let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
        let start = tokio::time::Instant::now();
        assert!(b.build(&json!({})).await.is_err());
        assert!(start.elapsed() < Duration::from_millis(300));
        assert!(rx.recv().await.is_some());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
        h.abort();
    }
}

#[tokio::test]
async fn request_timeout_and_caller_cancellation_stop_work_without_hidden_background_retries() {
    let (_f, mut c) = fixture();
    c["timeout_ms"] = json!(30);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let b = RouteActionBuilder::new_with_origin(&c, &origin).unwrap();
    let event = json!({});
    let work = b.build(&event);
    let accept = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 8192];
        let _ = stream.read(&mut buffer).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let (result, _) = tokio::join!(work, accept);
    assert!(result.is_err());
    let event = json!({});
    let mut work = Box::pin(b.build(&event));
    tokio::select! { result=&mut work=>panic!("unexpected early result {result:?}"), accepted=listener.accept()=> { let (_socket,_)=accepted.unwrap(); } }
    drop(work);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn duration_bounds_prevent_instant_overflow_and_additional_signing_is_not_silently_dropped() {
    let (_f, c) = fixture();
    for key in [
        "timeout_ms",
        "connect_timeout_ms",
        "max_wait_ms",
        "min_request_interval_ms",
        "retry_backoff_ms",
    ] {
        let mut bad = c.clone();
        bad[key] = json!(u64::MAX);
        assert!(
            RouteActionBuilder::new(&bad).is_err(),
            "accepted unbounded {key}"
        );
    }
    let b = RouteActionBuilder::new(&c).unwrap();
    let mut r = response(99);
    r["data"][0]["tx"]["signatureData"] = json!(["0x1234"]);
    assert!(
        b.validate_response(&r).is_err(),
        "additional signing requirements lost"
    );
    let mut r = response(99);
    r["data"][0]["tx"]["slippagePercent"] = json!("50");
    assert!(b.validate_response(&r).is_err(), "slippage echo mismatch");
}

#[test]
fn spender_is_distinct_from_router_and_echoes_must_match_configured_approval_target() {
    let (_f, mut c) = fixture();
    c["expected_spender"] = json!(INPUT);
    c["require_runtime_preview"] = json!(true);
    c["allowed_selectors"] = json!(["0x12345678"]);
    let b = RouteActionBuilder::new(&c).unwrap();
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!("0x12345678");
    r["data"][0]["approveContract"] = json!(INPUT);
    assert!(b.validate_response(&r).is_ok());
    r["data"][0]["approveContract"] = json!(WALLET);
    assert!(b.validate_response(&r).is_err());
    c["allowed_selectors"] = json!(["0x38ed1739"]);
    assert!(
        RouteActionBuilder::new(&c)
            .unwrap()
            .validate_response(&response(99))
            .is_err(),
        "V2 router itself is the spender"
    );
}

#[test]
fn hard_minimum_must_honor_requested_slippage_without_treating_quote_as_calldata_minimum() {
    let (_f, c) = fixture();
    let b = RouteActionBuilder::new(&c).unwrap();
    let mut r = response(99);
    r["data"][0]["routerResult"]["toTokenAmount"] = json!("10000");
    assert!(
        b.validate_response(&r).is_err(),
        "accepted minimum far below requested slippage"
    );
    r["data"][0]["routerResult"]["toTokenAmount"] = json!("98");
    assert!(
        b.validate_response(&r).is_err(),
        "accepted minimum exceeding quote"
    );
}

#[test]
fn encoded_approval_metadata_cannot_hide_a_different_spender() {
    let (_f, c) = fixture();
    let b = RouteActionBuilder::new(&c).unwrap();
    let mut r = response(99);
    r["data"][0]["approveTransaction"] =
        json!(json!({"approveContract":WALLET,"approveTxCalldata":"0x"}).to_string());
    assert!(b.validate_response(&r).is_err());
    r["data"][0]["approveTransaction"] =
        json!(json!({"approveContract":ROUTER,"approveTxCalldata":"0x"}).to_string());
    assert!(b.validate_response(&r).is_ok());
    r["data"][0]["approveTransaction"] = json!("malformed synthetic-secret");
    assert!(b.validate_response(&r).is_err());
}

#[test]
fn hmac_rfc4231_vectors_include_long_keys() {
    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }
    assert_eq!(
        hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const ROUTER: &str = "0x2222222222222222222222222222222222222222";
const INPUT: &str = "0x3333333333333333333333333333333333333333";
const OUTPUT: &str = "0x4444444444444444444444444444444444444444";
fn fixture() -> (tempfile::NamedTempFile, Value) {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new_in(std::env::temp_dir()).unwrap();
    f.write_all(br#"{"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass"}"#).unwrap();
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    let c = json!({"chain_index":"56","binance_chain_id":"56","from_token_address":INPUT,
      "to_token_address":OUTPUT,"amount":"10000000000000000001","slippage_percent":"0.5",
      "user_wallet_address":WALLET,"expected_spender":ROUTER,"allowed_router_addresses":[ROUTER],
      "allowed_selectors":["0x38ed1739"],"max_value_wei":"0","max_gas_limit":500000,
      "deadline_unix_ms":u64::MAX,"timeout_ms":300,"connect_timeout_ms":100,"max_wait_ms":1500,
      "min_request_interval_ms":1,"max_retries":2,"retry_backoff_ms":5,"credential_file":f.path()});
    (f, c)
}
#[test]
fn strict_config_accepts_exact_integer_rejects_unknown_or_unsafe_fields() {
    let (_f, c) = fixture();
    assert!(RouteActionBuilder::new(&c).is_ok());
    for (key, value) in [
        ("unknown", json!(1)),
        ("amount", json!(1.5)),
        ("amount", json!("1e18")),
        ("amount", json!("0")),
        ("slippage_percent", json!("100")),
        ("chain_index", json!("501")),
        ("expected_spender", json!("not-an-address")),
        ("allowed_router_addresses", json!([])),
        ("allowed_selectors", json!(["0x123"])),
        ("max_wait_ms", json!(0)),
    ] {
        let mut bad = c.clone();
        bad[key] = value;
        assert!(RouteActionBuilder::new(&bad).is_err(), "accepted {key}");
    }
}

#[test]
fn credential_file_is_regular_owner_only_and_errors_do_not_echo_secrets() {
    let (f, c) = fixture();
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(RouteActionBuilder::new(&c).is_err());
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(f.path(),br#"{"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass","unexpected":true}"#).unwrap();
    let e = RouteActionBuilder::new(&c).err().unwrap();
    assert!(!e.contains("synthetic"));
    let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let link = dir.path().join("credentials");
    std::os::unix::fs::symlink(f.path(), &link).unwrap();
    let mut c = c;
    c["credential_file"] = json!(link);
    assert!(RouteActionBuilder::new(&c).is_err());
}
