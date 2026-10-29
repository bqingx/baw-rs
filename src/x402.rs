//! x402 payments: preview an HTTP 402 challenge, then sign the chosen option.
//!
//! Both leaves are POSTs under the fixed agent-wallet prefix. `preview` is a read
//! (it only describes options); `sign` is the mutation — it produces the replay
//! header value the caller attaches to the retried 402 request.
use crate::client::{Client, API};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// The installed CLI forwards `--paymentRequirements` verbatim (it accepts either a
/// base64 `PAYMENT-REQUIRED` header value or raw JSON), so we do not decode it; we
/// only bound it so a shell accident cannot push an unbounded body at the fixed API.
const MAX_REQUIREMENTS: usize = 64 * 1024;
const MAX_PAYMENT_ID: usize = 200;

#[derive(Subcommand, Debug)]
pub enum X402Command {
    /// Preview x402 payment options from a PaymentRequired response
    Preview(PreviewArgs),
    /// Sign a selected x402 payment option and return the replay header value
    Sign(SignArgs),
}

#[derive(Args, Debug)]
pub struct PreviewArgs {
    /// The PaymentRequired payload — base64 `PAYMENT-REQUIRED` header value or raw JSON (auto-detected)
    #[arg(long = "paymentRequirements", value_name = "value")]
    pub payment_requirements: String,
}

#[derive(Args, Debug)]
pub struct SignArgs {
    /// Payment id returned by x402-payment preview
    #[arg(long = "paymentId", value_name = "paymentId")]
    pub payment_id: String,
    /// Selected payment option index returned by x402-payment preview
    #[arg(long = "selectedIndex", value_name = "index")]
    pub selected_index: u32,
}

pub fn is_mutation(command: &X402Command) -> bool {
    matches!(command, X402Command::Sign(_))
}

/// Deterministic intent digest input: the payment id and the chosen option index,
/// nothing derived from the session or from a previous response body.
pub fn mutation_payload(command: &X402Command) -> Result<Value, String> {
    match command {
        X402Command::Sign(args) => {
            payment_id(&args.payment_id)?;
            Ok(json!({
                "paymentId": args.payment_id,
                "selectedIndex": args.selected_index,
            }))
        }
        X402Command::Preview(_) => Err("x402-payment preview is not a mutation".into()),
    }
}

pub async fn run(client: &mut Client, command: &X402Command) -> Result<Value, String> {
    match command {
        X402Command::Preview(args) => preview(client, args).await,
        X402Command::Sign(args) => sign(client, args).await,
    }
}

fn session(client: &Client) -> Result<(), String> {
    if client.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}

fn payment_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_PAYMENT_ID || !id.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("invalid paymentId".into());
    }
    Ok(())
}

/// `/usr/lib/node_modules/@binance/agentic-wallet/dist/index.js`:
/// `requiredOption("--paymentRequirements <value>","The PaymentRequired payload ...")`
/// — the value is required and forwarded byte-for-byte, including the newlines that
/// `... | base64` inserts; only an empty or control-character payload is refused.
fn requirements(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_REQUIREMENTS {
        return Err("invalid payment requirements payload".into());
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err("invalid payment requirements payload".into());
    }
    Ok(())
}

/// One-shot guard mirroring the consumed-preview marker in `src/signing.rs`: a signed
/// option is never re-signed, even after an error, because the retry could double-pay.
/// It is process-local — `x402::run` receives no wallet directory, so the durable
/// one-shot record is the parent-owned operation journal (a unique `--operation-id`
/// is never replayed). Reported as a known gap rather than guessed around.
fn consumed() -> &'static Mutex<HashSet<String>> {
    static CONSUMED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CONSUMED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn consume(key: String) -> Result<(), String> {
    let mut set = consumed()
        .lock()
        .map_err(|_| "payment signature guard unavailable")?;
    if !set.insert(key) {
        return Err("payment option already signed; never retry an uncertain signature".into());
    }
    Ok(())
}

fn consume_key(client: &Client, args: &SignArgs) -> String {
    let mut key = String::with_capacity(args.payment_id.len() + 64);
    key.push_str(client.credentials().client_id.as_str());
    key.push('\u{1f}');
    key.push_str(&args.payment_id);
    key.push('\u{1f}');
    key.push_str(&args.selected_index.to_string());
    key
}

async fn preview(client: &mut Client, args: &PreviewArgs) -> Result<Value, String> {
    requirements(&args.payment_requirements)?;
    session(client)?;
    // dist/index.js: `static PREVIEW_ENDPOINT="/bapi/defi/v1/public/wallet-direct/agent-wallet/x402-payment/preview"`
    // and `preview({paymentRequirements:e})` with `{method:"POST",body:e}`.
    // The response (`paymentId`, `options[]`) is returned losslessly; the text renderer
    // in the JS CLI only projects it for humans.
    client
        .request(
            Method::POST,
            &format!("{API}/x402-payment/preview"),
            Some(json!({ "paymentRequirements": args.payment_requirements })),
        )
        .await
}

async fn sign(client: &mut Client, args: &SignArgs) -> Result<Value, String> {
    payment_id(&args.payment_id)?;
    session(client)?;
    // Consume before the network call: an accepted attempt stays consumed on error.
    consume(consume_key(client, args))?;
    // dist/index.js: `static SIGN_ENDPOINT="/bapi/defi/v1/public/wallet-direct/agent-wallet/x402-payment/sign"`
    // and `sign({paymentId:e,selectedIndex:n})` with `{method:"POST",body:e}`; the JS
    // action passes `Number(e.selectedIndex)`, so the index is sent as a JSON number.
    // The response carries the replay header (`paymentHeaderName`, `paymentHeaderValue`,
    // `signatureExpiresAt`) plus an optional `approveTxHash`; it is returned losslessly
    // and deliberately, exactly like `sign-message result` — never logged by this module.
    client
        .request(
            Method::POST,
            &format!("{API}/x402-payment/sign"),
            Some(json!({
                "paymentId": args.payment_id,
                "selectedIndex": args.selected_index,
            })),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Credentials;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

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

    fn preview_args(requirements: &str) -> PreviewArgs {
        PreviewArgs {
            payment_requirements: requirements.into(),
        }
    }

    fn sign_args(payment_id: &str, index: u32) -> SignArgs {
        SignArgs {
            payment_id: payment_id.into(),
            selected_index: index,
        }
    }

    #[tokio::test]
    async fn preview_posts_the_challenge_verbatim_to_the_preview_endpoint() {
        let payload = "eyJ4NDAyVmVyc2lvbiI6MiwKImFjY2VwdHMiOltdfQ==\n";
        let (base, requests, handle) = mock(vec![response(json!({
            "paymentId": "550e8400-e29b-41d4-a716-446655440000",
            "options": [{"index": 0, "status": "READY_TO_SIGN", "unknownNewField": true}],
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = run(&mut client, &X402Command::Preview(preview_args(payload)))
            .await
            .unwrap();
        assert_eq!(out["paymentId"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(out["options"][0]["unknownNewField"], true);
        let request = requests.recv().unwrap();
        assert!(request.starts_with(&format!("POST {API}/x402-payment/preview HTTP/1.1\r\n")));
        assert_eq!(body_of(&request), json!({"paymentRequirements": payload}));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn sign_posts_payment_id_and_numeric_index_and_returns_the_header() {
        let (base, requests, handle) = mock(vec![response(json!({
            "paymentHeaderName": "PAYMENT-SIGNATURE",
            "paymentHeaderValue": "synthetic-replay-header",
            "signatureExpiresAt": 1767225600,
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = run(&mut client, &X402Command::Sign(sign_args("payment-1", 1)))
            .await
            .unwrap();
        assert_eq!(out["paymentHeaderName"], "PAYMENT-SIGNATURE");
        assert_eq!(out["paymentHeaderValue"], "synthetic-replay-header");
        let request = requests.recv().unwrap();
        assert!(request.starts_with(&format!("POST {API}/x402-payment/sign HTTP/1.1\r\n")));
        assert_eq!(
            body_of(&request),
            json!({"paymentId":"payment-1","selectedIndex":1})
        );
        handle.join().unwrap();
    }

    #[test]
    fn only_sign_is_a_mutation_and_its_payload_is_deterministic() {
        let preview = X402Command::Preview(preview_args("{}"));
        let sign = X402Command::Sign(sign_args("payment-2", 0));
        assert!(!is_mutation(&preview));
        assert!(is_mutation(&sign));
        assert!(mutation_payload(&preview).is_err());
        assert_eq!(
            mutation_payload(&sign).unwrap(),
            json!({"paymentId":"payment-2","selectedIndex":0})
        );
        assert_eq!(
            mutation_payload(&sign).unwrap(),
            mutation_payload(&sign).unwrap()
        );
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let (base, requests, handle) =
            mock(vec![error_envelope("1200001", "secret-upstream-text")]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let error = run(&mut client, &X402Command::Sign(sign_args("payment-3", 0)))
            .await
            .unwrap_err();
        assert_eq!(error, "Binance API code 1200001");
        assert!(!error.contains("secret"));
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn invalid_arguments_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &base).unwrap();
        for command in [
            X402Command::Preview(preview_args("   ")),
            X402Command::Sign(sign_args("", 0)),
            X402Command::Sign(sign_args("bad id", 0)),
        ] {
            assert!(run(&mut client, &command).await.is_err());
        }
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[tokio::test]
    async fn a_signed_option_is_consumed_once_even_after_failure() {
        let (base, requests, handle) =
            mock(vec![error_envelope("1200002", "secret-upstream-text")]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let command = X402Command::Sign(sign_args("payment-4", 2));
        assert!(run(&mut client, &command).await.is_err());
        let error = run(&mut client, &command).await.unwrap_err();
        assert!(error.contains("already signed"), "{error}");
        requests.recv().unwrap();
        handle.join().unwrap();
    }
}
