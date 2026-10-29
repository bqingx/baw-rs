//! Wallet operations the JavaScript CLI exposes beyond query: speed-up, cancel,
//! resolve-domain. Endpoints and bodies are mined from the installed `baw` 1.12.0
//! bundle; nothing here invents a path or a field.
use crate::client::{Client, API};
use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};

#[derive(Args, Debug)]
pub struct SpeedUpArgs {
    /// Transaction hash of the pending transaction (0x-prefixed, 66 characters)
    #[arg(value_name = "txHash")]
    pub tx_hash: String,
    /// Gas bump level: LOW (×1.1), HIGH (×1.25)
    #[arg(long = "level", value_name = "level", default_value = "HIGH")]
    pub level: String,
    /// Binance chain ID (optional, auto-detected from txHash)
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
}

/// Leaf is `wallet cancel`; the spelling is declared once, on the `cli.rs` variant
/// (`#[command(name = "cancel")]`). Do not add an alias here — clap rejects a name and
/// alias that collide.
#[derive(Args, Debug)]
pub struct CancelTxArgs {
    /// Transaction hash of the pending transaction (0x-prefixed, 66 characters)
    #[arg(value_name = "txHash")]
    pub tx_hash: String,
    /// Binance chain ID (optional, auto-detected from txHash)
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct ResolveDomainArgs {
    /// Domain name to resolve, e.g. alice.eth / alice.bnb
    #[arg(long = "name", value_name = "name")]
    pub name: String,
    /// Binance chain ID, e.g. 56 (BSC)
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: String,
}

/// `wallet speed-up` and `wallet cancel` are the only writing leaves here;
/// `resolve-domain` is a read. `dispatch.rs` keys consent/journaling off this split.
pub async fn speed_up(client: &mut Client, args: &SpeedUpArgs) -> Result<Value, String> {
    let body = speed_up_payload(args)?;
    session(client)?;
    // dist/index.js: `static SPEEDUP_ENDPOINT="/bapi/defi/v1/public/wallet-direct/agent-wallet/order/speedup"`
    // and `speedup({originalTxHash:e.originalTxHash,binanceChainId:e.binanceChainId,level:e.level})`
    // with `{method:"POST",body:e}`.
    let data = client
        .request(Method::POST, &format!("{API}/order/speedup"), Some(body))
        .await?;
    // `if(r.status==="SUBMIT_FAIL")throw new m(d.BROADCAST_FAILED,r.failReason||"Speedup transaction broadcast failed")`
    // The backend's `failReason` is never echoed; errors are static strings.
    submitted(&data, "speed-up")
}

pub async fn cancel(client: &mut Client, args: &CancelTxArgs) -> Result<Value, String> {
    let body = cancel_payload(args)?;
    session(client)?;
    // dist/index.js: `static CANCEL_ENDPOINT="/bapi/defi/v1/public/wallet-direct/agent-wallet/order/cancel"`
    // and `cancel({originalTxHash:e.originalTxHash,binanceChainId:e.binanceChainId})`
    // with `{method:"POST",body:e}`.
    let data = client
        .request(Method::POST, &format!("{API}/order/cancel"), Some(body))
        .await?;
    // `if(r.status==="SUBMIT_FAIL")throw new m(d.BROADCAST_FAILED,r.failReason||"Cancel transaction broadcast failed")`
    submitted(&data, "cancel")
}

pub async fn resolve_domain(
    client: &mut Client,
    args: &ResolveDomainArgs,
) -> Result<Value, String> {
    let body = resolve_payload(args)?;
    session(client)?;
    // dist/index.js: `static RESOLVE_ENDPOINT="/bapi/defi/v1/public/wallet-direct/agent-wallet/domain/resolve"`
    // and `resolve({name:e.name,binanceChainId:e.binanceChainId})` with `{method:"POST",body:e}`.
    let data = client
        .request(Method::POST, &format!("{API}/domain/resolve"), Some(body))
        .await?;
    // The JS `--json` path deliberately reshapes the answer:
    // `g({name:l.name,binanceChainId:l.binanceChainId,address:l.address,isFallback:l.isFallback,addressType:l.addressType})`.
    // We match that projection, and additionally fail closed when no address came back:
    // a resolved domain is only useful as a `wallet send` recipient.
    let mut out = serde_json::Map::new();
    for key in [
        "name",
        "binanceChainId",
        "address",
        "isFallback",
        "addressType",
    ] {
        if let Some(value) = data.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    let address = out
        .get("address")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("domain resolution returned no address")?;
    if address.len() > 128 {
        return Err("domain resolution returned an invalid address".into());
    }
    Ok(Value::Object(out))
}

/// Deterministic intent digest input for `wallet speed-up`: chain, tx hash and the
/// normalized gas bump level, and nothing sourced from a response.
pub fn speed_up_payload(args: &SpeedUpArgs) -> Result<Value, String> {
    let level = level(&args.level)?;
    let mut body = json!({
        "originalTxHash": tx_hash(&args.tx_hash)?,
        "level": level,
    });
    if let Some(chain) = &args.binance_chain_id {
        body["binanceChainId"] = json!(chain_id(chain)?);
    }
    Ok(body)
}

/// Deterministic intent digest input for `wallet cancel`.
pub fn cancel_payload(args: &CancelTxArgs) -> Result<Value, String> {
    let mut body = json!({ "originalTxHash": tx_hash(&args.tx_hash)? });
    if let Some(chain) = &args.binance_chain_id {
        body["binanceChainId"] = json!(chain_id(chain)?);
    }
    Ok(body)
}

fn resolve_payload(args: &ResolveDomainArgs) -> Result<Value, String> {
    Ok(json!({
        "name": domain(&args.name)?,
        "binanceChainId": chain_id(&args.binance_chain_id)?,
    }))
}

fn session(client: &Client) -> Result<(), String> {
    if client.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}

/// `function Xt(t){return Fa.test(t)}` with `Fa=/^0x[a-fA-F0-9]{64}$/`; the CLI's
/// message is "Invalid txHash format. Expected 0x-prefixed hex string (66 characters)."
fn tx_hash(value: &str) -> Result<&str, String> {
    let valid = value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(
            "invalid txHash format; expected 0x-prefixed hex string (66 characters)".into(),
        );
    }
    Ok(value)
}

/// Both leaves pass `binanceChainId` straight through (auto-detected from the tx hash
/// when omitted), so only an empty, padded or control-character value is refused here.
fn chain_id(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 32
        || value.chars().any(char::is_control)
    {
        return Err("invalid binanceChainId".into());
    }
    Ok(value)
}

/// `let s=n.level.toUpperCase();if(!["LOW","HIGH"].includes(s))throw new m(d.INVALID_PARAMS,"Invalid level. Supported: LOW, HIGH.")`
fn level(value: &str) -> Result<&'static str, String> {
    match value.to_ascii_uppercase().as_str() {
        "LOW" => Ok("LOW"),
        "HIGH" => Ok("HIGH"),
        _ => Err("invalid level; supported: LOW, HIGH".into()),
    }
}

/// `if(!o||!o.includes("."))throw new m(d.INVALID_PARAMS,"Invalid domain name. Expected a name containing a dot, e.g. alice.eth")`
fn domain(value: &str) -> Result<&str, String> {
    if !value.contains('.')
        || value.len() > 200
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("invalid domain name; expected a name containing a dot, e.g. alice.eth".into());
    }
    Ok(value)
}

/// The JS `--json` path prints `{orderId, txHash, status}` for both leaves
/// (`p()?g({orderId:c.orderId,txHash:c.txHash,status:c.status})`), so submitted
/// transactions are reported as submitted, never as filled.
fn submitted(data: &Value, action: &str) -> Result<Value, String> {
    if data["status"] == "SUBMIT_FAIL" {
        return Err(format!("{action} transaction broadcast failed"));
    }
    let mut out = serde_json::Map::new();
    for key in ["orderId", "txHash", "status"] {
        if let Some(value) = data.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Credentials;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    const HASH: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn mock(responses: Vec<String>) -> (String, Receiver<String>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|l| l.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (base, rx, thread)
    }

    fn response(data: Value) -> String {
        let body = json!({"code":"000000","data":data}).to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn error_envelope(code: &str, message: &str) -> String {
        let body = json!({"code":code,"message":message,"data":null}).to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn credentials() -> Credentials {
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    fn body_of(request: &str) -> Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }

    fn speed_up_args(hash: &str, level: &str, chain: Option<&str>) -> SpeedUpArgs {
        SpeedUpArgs {
            tx_hash: hash.into(),
            level: level.into(),
            binance_chain_id: chain.map(str::to_owned),
        }
    }

    fn cancel_args(hash: &str, chain: Option<&str>) -> CancelTxArgs {
        CancelTxArgs {
            tx_hash: hash.into(),
            binance_chain_id: chain.map(str::to_owned),
        }
    }

    fn resolve_args(name: &str, chain: &str) -> ResolveDomainArgs {
        ResolveDomainArgs {
            name: name.into(),
            binance_chain_id: chain.into(),
        }
    }

    #[tokio::test]
    async fn speed_up_posts_normalized_level_and_reports_submitted_only() {
        let (base, requests, handle) = mock(vec![response(json!({
            "orderId": "order-1",
            "txHash": HASH_B,
            "status": "SUBMIT_SUCCESS",
            "filledAmount": "must.not.be.reported",
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = speed_up(&mut client, &speed_up_args(HASH, "low", Some("56")))
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({"orderId":"order-1","txHash":HASH_B,"status":"SUBMIT_SUCCESS"})
        );
        let request = requests.recv().unwrap();
        assert!(request.starts_with(&format!("POST {API}/order/speedup HTTP/1.1\r\n")));
        assert_eq!(
            body_of(&request),
            json!({"originalTxHash":HASH,"level":"LOW","binanceChainId":"56"})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn speed_up_omits_auto_detected_chain_and_defaults_to_high() {
        let (base, requests, handle) = mock(vec![response(
            json!({"orderId":"order-2","status":"SUBMIT_SUCCESS"}),
        )]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let args = SpeedUpArgs {
            tx_hash: HASH.into(),
            level: String::from("HIGH"),
            binance_chain_id: None,
        };
        let out = speed_up(&mut client, &args).await.unwrap();
        assert_eq!(out, json!({"orderId":"order-2","status":"SUBMIT_SUCCESS"}));
        let body = body_of(&requests.recv().unwrap());
        assert_eq!(body, json!({"originalTxHash":HASH,"level":"HIGH"}));
        assert!(body.get("binanceChainId").is_none());
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn cancel_posts_original_hash_and_chain_and_reports_submitted_only() {
        let (base, requests, handle) = mock(vec![response(json!({
            "orderId": "order-3",
            "txHash": HASH_B,
            "status": "SUBMIT_FAIL_PENDING",
            "failReason": "secret-upstream-reason",
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = cancel(&mut client, &cancel_args(HASH, Some("CT_501")))
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({"orderId":"order-3","txHash":HASH_B,"status":"SUBMIT_FAIL_PENDING"})
        );
        assert!(!out.to_string().contains("secret"));
        let request = requests.recv().unwrap();
        assert!(request.starts_with(&format!("POST {API}/order/cancel HTTP/1.1\r\n")));
        assert_eq!(
            body_of(&request),
            json!({"originalTxHash":HASH,"binanceChainId":"CT_501"})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn resolve_domain_posts_name_and_chain_and_projects_the_js_shape() {
        let (base, requests, handle) = mock(vec![response(json!({
            "name": "alice.eth",
            "binanceChainId": "56",
            "address": "0x52908400098527886e0f7030069857d2e4169ee7",
            "isFallback": false,
            "addressType": "EVM",
            "unprojected": "backend-extra-field",
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = resolve_domain(&mut client, &resolve_args("alice.eth", "56"))
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({
                "name": "alice.eth",
                "binanceChainId": "56",
                "address": "0x52908400098527886e0f7030069857d2e4169ee7",
                "isFallback": false,
                "addressType": "EVM",
            })
        );
        let request = requests.recv().unwrap();
        assert!(request.starts_with(&format!("POST {API}/domain/resolve HTTP/1.1\r\n")));
        assert_eq!(
            body_of(&request),
            json!({"name":"alice.eth","binanceChainId":"56"})
        );
        handle.join().unwrap();
    }

    #[test]
    fn mutation_classification_matches_dispatch() {
        // Mutations: speed-up and cancel each carry a deterministic intent payload.
        let speed = speed_up_payload(&speed_up_args(HASH, "high", Some("56"))).unwrap();
        assert_eq!(
            speed,
            json!({"originalTxHash":HASH,"level":"HIGH","binanceChainId":"56"})
        );
        assert_eq!(
            speed_up_payload(&speed_up_args(HASH, "high", Some("56"))).unwrap(),
            speed
        );
        let cancel_intent = cancel_payload(&cancel_args(HASH, None)).unwrap();
        assert_eq!(cancel_intent, json!({"originalTxHash":HASH}));
        // Reads: resolve-domain is read-only and produces no mutation payload at all;
        // `dispatch.rs` routes only SpeedUp/CancelTx through `require_consent`.
        assert!(resolve_payload(&resolve_args("alice.eth", "56")).is_ok());
        assert!(resolve_payload(&resolve_args("alice.eth", "56"))
            .unwrap()
            .get("originalTxHash")
            .is_none());
    }

    #[tokio::test]
    async fn submit_fail_is_a_static_sanitized_error() {
        for (action, reply) in [
            (
                "speed-up",
                json!({"status":"SUBMIT_FAIL","orderId":"order-4","failReason":"secret-upstream-reason"}),
            ),
            (
                "cancel",
                json!({"status":"SUBMIT_FAIL","orderId":"order-5","failReason":"secret-upstream-reason"}),
            ),
        ] {
            let (base, requests, handle) = mock(vec![response(reply)]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            let error = if action == "speed-up" {
                speed_up(&mut client, &speed_up_args(HASH, "HIGH", None))
                    .await
                    .unwrap_err()
            } else {
                cancel(&mut client, &cancel_args(HASH, None))
                    .await
                    .unwrap_err()
            };
            assert_eq!(error, format!("{action} transaction broadcast failed"));
            assert!(!error.contains("secret"));
            requests.recv().unwrap();
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let (base, requests, handle) =
            mock(vec![error_envelope("1200009", "secret-upstream-text")]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let error = resolve_domain(&mut client, &resolve_args("alice.eth", "56"))
            .await
            .unwrap_err();
        assert_eq!(error, "Binance API code 1200009");
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn invalid_arguments_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &base).unwrap();
        for hash in ["0xdeadbeef", "not-a-hash", "", HASH.trim_end_matches('a')] {
            assert!(cancel(&mut client, &cancel_args(hash, None)).await.is_err());
        }
        assert!(speed_up_payload(&speed_up_args(HASH, "MEDIUM", None)).is_err());
        assert!(speed_up_payload(&speed_up_args(HASH, "HIGH", Some(" 56"))).is_err());
        assert!(resolve_domain(&mut client, &resolve_args("alice", "56"))
            .await
            .is_err());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[tokio::test]
    async fn resolve_domain_without_address_fails_closed() {
        let (base, requests, handle) = mock(vec![response(json!({"name":"alice.eth"}))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            resolve_domain(&mut client, &resolve_args("alice.eth", "56"))
                .await
                .unwrap_err(),
            "domain resolution returned no address"
        );
        requests.recv().unwrap();
        handle.join().unwrap();
    }
}
