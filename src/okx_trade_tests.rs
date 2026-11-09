use super::*;
use crate::trade::Provider;

#[path = "okx_approval_tests.rs"]
mod auto_approve;
#[path = "okx_pool_tests.rs"]
mod pool_tests;
use std::{
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
const FROM: &str = "0x5555555555555555555555555555555555555555";
const INPUT: &str = "0x1111111111111111111111111111111111111111";
const OUTPUT: &str = "0x2222222222222222222222222222222222222222";
const ROUTER: &str = "0x3333333333333333333333333333333333333333";
const SPENDER: &str = "0x4444444444444444444444444444444444444444";
fn private(path: &Path, content: &Value) {
    std::fs::write(path, content.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn fixture() -> (tempfile::TempDir, SwapArgs, Okx) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    private(
        &dir.path().join("credentials.json"),
        &json!({"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass"}),
    );
    private(
        &dir.path().join("profile.json"),
        &json!({"credential_file":dir.path().join("credentials.json"),"chain_index":"56", "expected_spender":SPENDER,"allowed_router_addresses":[ROUTER]}),
    );
    let q = QuoteArgs {
        provider: Provider::Okx,
        okx_config: Some(dir.path().join("profile.json")),
        from_token_decimals: Some(6),
        binance_chain_id: "56".into(),
        from_token: INPUT.into(),
        to_token: OUTPUT.into(),
        from_token_qty: "1.25".into(),
        slippage: "1".into(),
        max_from_tax_percent: "0".into(),
        max_to_tax_percent: "0".into(),
    };
    let api = Okx::load(&q).unwrap();
    let a = SwapArgs {
        quote: q,
        mev: "false".into(),
        gas_level: "MEDIUM".into(),
        acknowledge_token_risk: true,
        okx_min_output_units: Some("9800".into()),
        auto_approve: false,
    };
    (dir, a, api)
}
fn quoted() -> Value {
    json!({"chainIndex":"56","swapMode":"exactIn","fromTokenAmount":"1250000","toTokenAmount":"9900","priceImpactPercent":"0.1",
        "fromToken":{"tokenContractAddress":INPUT,"decimal":"6","isHoneyPot":false,"taxRate":"0","tokenSymbol":"IN"},
        "toToken":{"tokenContractAddress":OUTPUT,"decimal":"6","isHoneyPot":false,"taxRate":"0","tokenSymbol":"OUT"}})
}
fn response() -> Value {
    json!({"code":"0","data":[{"routerResult":quoted(),"tx":{"from":FROM,"to":ROUTER,"value":"0","gas":"100000","data":"0x12345678","minReceiveAmount":"9801","chainId":"56"}}]})
}
fn preview() -> Value {
    json!({"requestId":"okx-synthetic-preview","requireConfirmation":false,"risks":{"addresses":{},"riskDetails":[],"riskBehaviors":[]},
        "parsedTx":{"contractAddress":ROUTER},"simulationResult":{"simulationCode":"000000000","authorityChanges":[],"allowanceChanges":[],
        "balanceChanges":[{"owner":FROM,"contractAddress":INPUT,"tokenType":"Erc20","change":"-1250000"},
            {"owner":FROM,"contractAddress":OUTPUT,"tokenType":"Erc20","change":"9900"}]}})
}
#[test]
fn exact_amount_conversion_never_rounds_or_uses_floats() {
    assert_eq!(
        amount_units("0001.25", 6, true).unwrap().to_string(),
        "1250000"
    );
    assert_eq!(
        amount_units("9007199254740993.01", 18, true)
            .unwrap()
            .to_string(),
        "9007199254740993010000000000000000"
    );
    for bad in ["1e6", "-1", "+1", "1.0000001", "1.", ".1", " 1", "0"] {
        assert!(amount_units(bad, 6, true).is_err(), "{bad}");
    }
    assert!(amount_units("1", 37, true).is_err());
    assert_eq!(display_units(&BigUint::from(9900u32), 6), "0.0099");
}
#[test]
fn provider_options_are_explicit_and_fail_closed() {
    let (_d, mut a, _) = fixture();
    assert!(validate_quote_options(&a.quote).is_ok());
    assert!(validate_swap_options(&a).is_ok());
    a.quote.slippage = "auto".into();
    assert!(validate_quote_options(&a.quote).is_err());
    a.quote.slippage = "1".into();
    a.mev = "true".into();
    assert!(validate_swap_options(&a).is_err());
    a.mev = "false".into();
    a.okx_min_output_units = None;
    assert!(validate_swap_options(&a).is_ok());
    for bad in ["0", "", "-1", "+1", "1.0", "1e3", " 1", "1 "] {
        a.okx_min_output_units = Some(bad.into());
        assert!(
            validate_swap_options(&a).is_err(),
            "accepted fixed floor {bad:?}"
        );
    }
    a.okx_min_output_units = Some("1".into());
    assert!(validate_swap_options(&a).is_ok());
    a.quote.binance_chain_id = "CT_501".into();
    assert!(validate_quote_options(&a.quote).is_err());
}
#[test]
fn quote_binds_chain_tokens_decimals_amount_risk_and_impact() {
    let (_d, a, api) = fixture();
    let valid = quoted();
    assert_eq!(
        api.validate_quote(&a.quote, &valid).unwrap()["toCoinAmount"],
        "0.0099"
    );
    for (pointer, value) in [
        ("/chainIndex", json!("1")),
        ("/swapMode", json!("exactOut")),
        ("/fromTokenAmount", json!("1")),
        ("/fromToken/decimal", json!("18")),
        ("/toToken/tokenContractAddress", json!(INPUT)),
        ("/toToken/isHoneyPot", json!(true)),
        ("/toToken/taxRate", json!("0.02")),
        ("/priceImpactPercent", json!("-3")),
        ("/priceImpactPercent", Value::Null),
        ("/toTokenAmount", json!("0")),
    ] {
        let mut q = valid.clone();
        *q.pointer_mut(pointer).unwrap() = value;
        assert!(api.validate_quote(&a.quote, &q).is_err(), "{pointer}");
    }
}
#[test]
fn private_profile_rejects_insecure_files_links_and_unknown_fields() {
    let (d, a, _) = fixture();
    let path = a.quote.okx_config.as_ref().unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Okx::load(&a.quote).is_err());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = d.path().join("alias");
    std::os::unix::fs::symlink(path, &link).unwrap();
    assert!(private_read(&link).is_err());
    std::fs::hard_link(path, d.path().join("hardlink")).unwrap();
    assert!(Okx::load(&a.quote).is_err());
    std::fs::remove_file(d.path().join("hardlink")).unwrap();
    let mut profile: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    profile["origin"] = json!("https://untrusted.invalid");
    private(path, &profile);
    assert!(Okx::load(&a.quote).is_err());
}
#[test]
fn quote_and_swap_never_implicitly_accept_missing_pins() {
    let (_d, a, mut api) = fixture();
    api.profile.allowed_router_addresses.clear();
    assert!(api.validate_quote(&a.quote, &quoted()).is_ok());
    assert!(api.route_config(&a.quote, FROM).is_err());
}
#[test]
fn preview_only_accepts_bounded_own_asset_deltas_without_authority_changes() {
    let (_d, a, _) = fixture();
    let good = preview();
    let minimum = BigUint::from(9801u32);
    assert!(validate_preview(&good, &a.quote, FROM, &minimum, ROUTER, SPENDER).is_ok());
    for (pointer, value) in [
        ("/parsedTx/contractAddress", json!(SPENDER)),
        (
            "/simulationResult/balanceChanges/0/change",
            json!("-1250001"),
        ),
        ("/simulationResult/balanceChanges/1/change", json!("9800")),
        ("/simulationResult/balanceChanges/1/owner", json!(ROUTER)),
        (
            "/simulationResult/balanceChanges/1/tokenType",
            json!("Erc721"),
        ),
        (
            "/simulationResult/authorityChanges",
            json!([{"spender":SPENDER}]),
        ),
        (
            "/simulationResult/allowanceChanges",
            json!([{"spender":SPENDER}]),
        ),
        ("/risks/riskDetails", json!([{"code":"RISK"}])),
        ("/simulationResult/simulationCode", json!("FAIL")),
    ] {
        let mut p = good.clone();
        *p.pointer_mut(pointer).unwrap() = value;
        assert!(
            validate_preview(&p, &a.quote, FROM, &minimum, ROUTER, SPENDER).is_err(),
            "{pointer}"
        );
    }
    let mut p = good;
    p["simulationResult"]["extra"] = json!({"risks":{"riskDetails":[{"code":"HIDDEN"}]}});
    assert!(validate_preview(&p, &a.quote, FROM, &minimum, ROUTER, SPENDER).is_err());
}

struct Server {
    origin: String,
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
fn serve(handler: impl Fn(&str) -> (u16, Value) + Send + 'static) -> Server {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let requests = seen.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let len = h
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            requests.lock().unwrap().push(request.clone());
            let (status, value) = handler(&request);
            let body = value.to_string();
            stream.write_all(format!("HTTP/1.1 {status} mock\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
        }
    });
    Server {
        origin,
        seen,
        stop,
        thread: Some(thread),
    }
}
fn binance(preview: Value, execute_fails: bool) -> (Server, Client) {
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
            preview.clone()
        } else if line.contains("contract-call/execute") {
            if execute_fails {
                return (503, json!({"message":"synthetic secret must never escape"}));
            }
            json!({"status":"BROADCASTED","orderId":"mock-order","txHash":format!("0x{}","a".repeat(64))})
        } else {
            panic!("unexpected mock Binance request: {line}")
        };
        (200, json!({"code":"000000","data":value}))
    });
    let credentials = crate::session::Credentials {
        client_id: zeroize::Zeroizing::new("synthetic-okx-client".into()),
        session_id: zeroize::Zeroizing::new("synthetic-okx-session".into()),
    };
    let client = Client::test_client(credentials, &server.origin).unwrap();
    (server, client)
}
fn okx(response: Value) -> Server {
    serve(move |request| {
        let line = request.lines().next().unwrap();
        if line.contains("/approve-transaction?") {
            (
                200,
                json!({"code":"0","data":[{"dexContractAddress":SPENDER}]}),
            )
        } else if line.contains("/quote?") {
            (200, json!({"code":"0","data":[quoted()]}))
        } else if line.contains("/swap?") {
            (200, response.clone())
        } else {
            panic!("unexpected mock OKX request")
        }
    })
}
#[test]
fn okx_is_the_only_provider_variant() {
    assert_eq!(
        <Provider as clap::ValueEnum>::value_variants(),
        &[Provider::Okx]
    );
}

#[tokio::test]
async fn exhausted_binance_quota_blocks_okx_requests() {
    let (d, a, mut api) = fixture();
    let okx = okx(response());
    api.origin = okx.origin.clone();
    let backend = serve(|request| {
        let value = if request.lines().next().unwrap().contains("settings/query") {
            json!({"dailyLimit":"1","devMode":{"enabled":true,"expiresAt":4102444800u64}})
        } else {
            json!({"quotaUsed":"1"})
        };
        (200, json!({"code":"000000","data":value}))
    });
    let credentials = crate::session::Credentials {
        client_id: zeroize::Zeroizing::new("synthetic-okx-client".into()),
        session_id: zeroize::Zeroizing::new("synthetic-okx-session".into()),
    };
    let mut client = Client::test_client(credentials, &backend.origin).unwrap();
    let error = swap(&mut client, d.path(), &a, &api, None)
        .await
        .unwrap_err();
    assert!(error.contains("quota exhausted"));
    assert!(okx.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn signed_http_uses_exact_encoded_query_and_redacts_errors() {
    let (_d, a, mut api) = fixture();
    let server = okx(response());
    api.origin = server.origin.clone();
    let params = query(&a.quote).unwrap();
    let raw = api.get("quote", &params).await.unwrap();
    assert_eq!(raw["code"], "0");
    let request = server.seen.lock().unwrap()[0].clone();
    let target = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let header = |key: &str| {
        request
            .lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v.trim().to_string())
            })
            .unwrap()
    };
    use base64::Engine;
    let expected = base64::engine::general_purpose::STANDARD.encode(route_action::hmac_sha256(
        b"synthetic-secret",
        format!("{}GET{target}", header("ok-access-timestamp")).as_bytes(),
    ));
    assert_eq!(header("ok-access-sign"), expected);
    assert!(target.contains("amount=1250000"));
    let fail = serve(|_| (429, json!({"secret":"synthetic-secret"})));
    api.origin = fail.origin.clone();
    let error = api.get("quote", &params).await.unwrap_err();
    assert!(!error.contains("synthetic-secret"));
    assert_eq!(fail.seen.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn swap_uses_selected_binance_wallet_and_executes_once_after_strict_preview() {
    let (d, a, mut api) = fixture();
    let okx = okx(response());
    api.origin = okx.origin.clone();
    let (backend, mut client) = binance(preview(), false);
    let out = swap(&mut client, d.path(), &a, &api, None).await.unwrap();
    assert_eq!(out["provider"], "okx");
    assert_eq!(out["status"], "BROADCASTED");
    assert_eq!(out["retrySafe"], false);
    assert_eq!(out["onchainMinimumDecoded"], false);
    assert_eq!(out["onchainNetMinimumDecoded"], false);
    assert_eq!(out["calldataPolicy"], route_action::CALLDATA_POLICY);
    let seen = backend.seen.lock().unwrap();
    assert_eq!(
        seen.iter()
            .filter(|r| r.lines().next().unwrap().contains("/execute "))
            .count(),
        1
    );
    let preview_request = seen
        .iter()
        .find(|r| r.lines().next().unwrap().contains("/preview "))
        .unwrap();
    assert!(preview_request.contains(ROUTER));
    assert!(!seen.iter().any(|r| r.contains("place-order")));
    assert_eq!(okx.seen.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn swap_rejects_unbound_or_low_minimum_or_bad_router_before_any_preview() {
    for kind in ["minimum", "router", "value", "signature", "token"] {
        let (d, a, mut api) = fixture();
        let mut response = response();
        match kind {
            "minimum" => response["data"][0]["tx"]["minReceiveAmount"] = json!("1"),
            "router" => response["data"][0]["tx"]["to"] = json!(SPENDER),
            "value" => response["data"][0]["tx"]["value"] = json!("1"),
            "signature" => response["data"][0]["signatureData"] = json!(["0x1234"]),
            "token" => {
                response["data"][0]["routerResult"]["toToken"]["tokenContractAddress"] =
                    json!(INPUT)
            }
            _ => unreachable!(),
        }
        let okx = okx(response);
        api.origin = okx.origin.clone();
        let (backend, mut client) = binance(preview(), false);
        assert!(
            swap(&mut client, d.path(), &a, &api, None).await.is_err(),
            "{kind}"
        );
        assert!(
            !backend.seen.lock().unwrap().iter().any(|r| r
                .lines()
                .next()
                .unwrap()
                .contains("/preview ")),
            "{kind}"
        );
    }
}
#[tokio::test]
async fn bad_preview_never_executes_and_execute_failure_is_uncertain_without_retry() {
    for bad_preview in [true, false] {
        let (d, a, mut api) = fixture();
        let okx = okx(response());
        api.origin = okx.origin.clone();
        let mut p = preview();
        if bad_preview {
            p["simulationResult"]["balanceChanges"][1]["change"] = json!("1");
        }
        let (backend, mut client) = binance(p, !bad_preview);
        let result = swap(&mut client, d.path(), &a, &api, None).await;
        let count = backend
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.lines().next().unwrap().contains("/execute "))
            .count();
        if bad_preview {
            assert!(result.is_err());
            assert_eq!(count, 0);
        } else {
            let v = result.unwrap();
            assert_eq!(v["status"], "UNCERTAIN");
            assert_eq!(v["retrySafe"], false);
            assert_eq!(count, 1);
        }
    }
}
