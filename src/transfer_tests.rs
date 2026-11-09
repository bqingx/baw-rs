//! Transfer regressions retained after removal of Binance market execution.
use super::*;
use std::io::{Read, Write};
fn server(
    data: Vec<Value>,
) -> (
    Client,
    std::sync::mpsc::Receiver<(String, Value)>,
    std::thread::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        for d in data {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length = header
                        .lines()
                        .find_map(|s| s.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let (header, body) = request.split_once("\r\n\r\n").unwrap();
            tx.send((
                header.lines().next().unwrap().into(),
                if body.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(body).unwrap()
                },
            ))
            .unwrap();
            if d["__disconnect"] == true || d.get("__http_status").is_some() {
                if let Some(status) = d["__http_status"].as_u64() {
                    stream.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
                }
                drop(stream);
                listener.set_nonblocking(true).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(150));
                assert!(listener.accept().is_err(), "uncertain transfer was retried");
                continue;
            }
            let body = json!({"code":"000000","data":d}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
    });
    let client = Client::test_client(
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        },
        &origin,
    )
    .unwrap();
    (client, rx, thread)
}
fn args() -> SendArgs {
    SendArgs {
        binance_chain_id: "56".into(),
        amount: Some("1.00".into()),
        max: false,
        recipient: "0x2222222222222222222222222222222222222222".into(),
        token_address: "0x1111111111111111111111111111111111111111".into(),
        gas_level: "HIGH".into(),
    }
}
#[test]
fn rwa_rounding_clamp_and_oversell_are_exact() {
    assert_eq!(
        sell_amount("1", "3", "0.3333333333333333335", 18).unwrap(),
        "0.3333333333333333335"
    );
    assert_eq!(
        sell_amount("1", "3", "1", 18).unwrap(),
        "0.333333333333333334"
    );
    assert_eq!(sell_amount("1", "3", "1", 6).unwrap(), "0.333334");
    assert_eq!(sell_amount("1.00", "1", "2", 6).unwrap(), "1.00");
    assert!(sell_amount("1", "3", "0.333333333333333333", 18).is_err());
    assert!(sell_amount("3", "1", "2", 18).is_err());
}
#[tokio::test]
async fn internal_failure_is_sanitized_and_not_submitted() {
    let (mut client, rx, thread) = server(vec![
        json!([]),
        json!({"code":"123","message":"private-backend-message","txHash":"not-proof"}),
    ]);
    let error = send(&mut client, &args()).await.unwrap_err();
    thread.join().unwrap();
    assert_eq!(rx.try_iter().count(), 2);
    assert!(error.contains("transfer rejected"));
    assert!(!error.contains("private-backend-message"));
}
#[test]
fn send_intent_preserves_input_and_requires_exactly_one_quantity() {
    let mut a = args();
    assert_eq!(send_payload(&a).unwrap()["args"]["amount"], "1.00");
    a.max = true;
    assert!(send_payload(&a).is_err());
    a.amount = None;
    assert!(send_payload(&a).is_ok());
    a.max = false;
    assert!(send_payload(&a).is_err());
}
#[tokio::test]
async fn fee_level_and_rwa_transfer_handling_are_preserved() {
    for mode in [0, 1, 2] {
        let mut a = args();
        let mut replies = Vec::new();
        if mode == 2 {
            a.max = true;
            a.amount = None;
        } else {
            replies.push(if mode==1 {json!([{"chainId":"56","contractAddress":a.token_address,"multiplier":"3","type":1}])} else {json!([])});
            if mode == 1 {
                replies.push(json!({"tokenList":[{"binanceChainId":"56","contractAddress":a.token_address,"balance":"1","decimals":6}]}));
            }
        }
        replies.push(json!({"txHash":"tx","status":"PROCESSING"}));
        let count = replies.len();
        let (mut client, rx, thread) = server(replies);
        let out = send(&mut client, &a).await.unwrap();
        assert_eq!(out["status"], "SUBMITTED");
        let requests: Vec<_> = (0..count).map(|_| rx.recv().unwrap()).collect();
        let (header, body) = requests.last().unwrap();
        assert!(header.starts_with(&format!("POST {A}/transfer ")));
        assert_eq!(body["feeLevel"], "HIGH");
        assert!(body.get("gasLevel").is_none());
        assert_eq!(body["recipientAddress"], a.recipient);
        if mode == 2 {
            assert_eq!(body["isMax"], true);
            assert!(body.get("amount").is_none());
        } else {
            assert_eq!(body["amount"], if mode == 1 { "0.333334" } else { "1.00" });
        }
        thread.join().unwrap();
    }
}
#[tokio::test]
async fn recipient_confirmation_is_not_reported_as_sent() {
    let mut a = args();
    a.max = true;
    a.amount = None;
    let (mut client, rx, thread) =
        server(vec![json!({"status":"PENDING_CONFIRMATION","orderId":"x"})]);
    let out = send(&mut client, &a).await.unwrap();
    assert_eq!(out["status"], "PENDING_CONFIRMATION");
    assert!(out.get("txHash").is_none());
    rx.recv().unwrap();
    thread.join().unwrap();
}
#[tokio::test]
async fn uncertain_transfers_are_never_automatically_retried() {
    for failure in [json!({"__disconnect":true}), json!({"__http_status":503})] {
        let mut a = args();
        a.max = true;
        a.amount = None;
        let (mut client, rx, thread) = server(vec![failure]);
        let out = send(&mut client, &a).await.unwrap();
        assert_eq!(out["status"], "UNCERTAIN");
        assert_eq!(out["retrySafe"], false);
        rx.recv().unwrap();
        thread.join().unwrap();
    }
}
