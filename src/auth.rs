//! Native asynchronous login; explicit directory and instance only.
use crate::{auth_store::Store, client::Client, session::Credentials};
use serde_json::Value;
use std::path::Path;
#[derive(clap::Subcommand, Debug, Clone)]
pub enum AuthCommand {
    Signin,
    Verify {
        #[arg(long = "qrCodeId")]
        qr_code_id: String,
    },
    Signout,
}
pub fn bootstrap(dir: &Path, instance: &str, command: &AuthCommand) -> Result<Credentials, String> {
    Store::open(dir, matches!(command, AuthCommand::Signin))?
        .credentials(instance, matches!(command, AuthCommand::Signin))
}
const API: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet/login";
#[derive(serde::Serialize, serde::Deserialize)]
struct Pending {
    client: String,
    cookie: String,
    baseline: String,
    qr_code_id: String,
    expire_at: u64,
}
impl Drop for Pending {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.client.zeroize();
        self.cookie.zeroize();
    }
}
fn unix_ms() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "invalid system clock".to_owned())
        .and_then(|v| u64::try_from(v.as_millis()).map_err(|_| "invalid system clock".to_owned()))
}
fn pending_read(store: &Store, instance: &str) -> Result<Pending, String> {
    let bytes = store
        .read(c"auth-pending")?
        .ok_or("no pending authentication; run signin first")?;
    let encoded = std::str::from_utf8(&bytes).map_err(|_| "invalid pending authentication")?;
    let text = crate::session::decrypt(encoded, &crate::session::derive_key(instance))?;
    serde_json::from_str(&text).map_err(|_| "invalid pending authentication".into())
}
fn pending_write(store: &Store, instance: &str, pending: &Pending) -> Result<(), String> {
    let text = zeroize::Zeroizing::new(
        serde_json::to_string(pending).map_err(|_| "cannot serialize pending authentication")?,
    );
    let encoded = crate::auth_store::encrypt(&text, &crate::session::derive_key(instance))?;
    store.write(c"auth-pending", encoded.as_bytes())
}
pub async fn run(
    client: &mut Client,
    dir: &Path,
    instance: &str,
    command: &AuthCommand,
) -> Result<Value, String> {
    use reqwest::Method;
    use serde_json::json;
    let store = Store::open(dir, false)?;
    let stored = store.credentials(instance, false)?;
    if stored.client_id.as_str() != client.credentials().client_id.as_str()
        || stored.session_id.as_str() != client.credentials().session_id.as_str()
    {
        return Err("session changed since bootstrap; restart command".into());
    }
    match command {
        AuthCommand::Signin => {
            if !stored.session_id.is_empty() {
                let existing = client
                    .request(Method::GET, &format!("{API}/query"), None)
                    .await;
                if let Ok(data) = existing {
                    if data["connectionStatus"] == "CONNECTED" {
                        if data["walletCreateStatus"] == "CREATED" {
                            return Ok(json!({"status":"ALREADY_CONNECTED"}));
                        }
                        return Err(
                            "wallet creation is still pending; verify existing login".into()
                        );
                    }
                }
            }
            if store.read(c"auth-pending")?.is_some() {
                let pending = pending_read(&store, instance)?;
                if pending.expire_at > unix_ms()? {
                    return Err(
                        "authentication already pending; verify existing QR or signout".into(),
                    );
                }
            }
            client.begin_signin();
            use k256::elliptic_curve::sec1::ToEncodedPoint;
            let secret = k256::SecretKey::random(&mut rand::rngs::OsRng);
            let public = secret.public_key().to_encoded_point(true);
            let stripped = crate::auth_store::hex(&public.as_bytes()[1..]);
            drop(secret);
            let started = unix_ms()?;
            let baseline = store.baseline()?;
            let data=client.request(Method::POST,API,Some(json!({"isFirstLogin":true,"os":if cfg!(target_os="macos"){"macOS"}else if cfg!(target_os="windows"){"Windows"}else{"Linux"},"publicKeyHex":stripped}))).await?;
            let qr = &data["qrInfo"];
            if data["connectionStatus"] != "UNCONNECTED" {
                return Err("invalid login response".into());
            }
            let qr_id = qr["qrCodeId"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("invalid QR response")?;
            let url = qr["qrCodeUrl"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("invalid QR response")?;
            let expiry = qr["expireAt"]
                .as_u64()
                .or_else(|| qr["expireAt"].as_str().and_then(|s| s.parse().ok()))
                .ok_or("invalid QR expiration")?;
            pending_write(
                &store,
                instance,
                &Pending {
                    client: stored.client_id.to_string(),
                    cookie: client.credentials().session_id.to_string(),
                    baseline,
                    qr_code_id: qr_id.to_owned(),
                    expire_at: expiry.min(started.saturating_add(300000)),
                },
            )?;
            Ok(
                json!({"qrCodeId":qr_id,"expireAt":qr["expireAt"],"urlForWeb":url,"pairingCode":format!("{}{}",&stripped[..3],&stripped[stripped.len()-3..])}),
            )
        }
        AuthCommand::Verify { qr_code_id } => {
            let pending = pending_read(&store, instance)?;
            if pending.expire_at <= unix_ms()? {
                return Err("QR authentication expired; run signin again".into());
            }
            if pending.qr_code_id != *qr_code_id
                || pending.client != stored.client_id.as_str()
                || pending.baseline != store.baseline()?
            {
                return Err(
                    "pending authentication does not match selected wallet/session/QR".into(),
                );
            }
            // Pre-auth the server has not issued a session cookie yet; an empty
            // pending cookie is expected and must not abort the QR poll.
            if !pending.cookie.is_empty() {
                client.set_session(zeroize::Zeroizing::new(pending.cookie.clone()))?;
            }
            let remaining = pending.expire_at.saturating_sub(unix_ms()?);
            let deadline = tokio::time::Instant::now()
                + std::time::Duration::from_millis(remaining.min(300000));
            poll_until_created(client, qr_code_id, deadline).await?;
            if client.credentials().session_id.is_empty() {
                return Err("confirmed login omitted session cookie".into());
            }
            store.save_cas(
                instance,
                &pending.client,
                &client.credentials().session_id,
                &pending.baseline,
            )?;
            store.remove(c"auth-pending")?;
            Ok(json!({"status":"SUCCESS"}))
        }
        AuthCommand::Signout => {
            let baseline = store.baseline()?;
            let remote = client
                .request(Method::POST, &format!("{API}/logout"), None)
                .await;
            client.invalidate();
            store
                .save_cas(instance, &stored.client_id, "", &baseline)
                .map_err(|_| "local cleanup failed; server logout may have completed".to_owned())?;
            store.remove(c"auth-pending").map_err(|_|"local session cleared but pending cleanup failed; server logout may have completed".to_owned())?;
            if remote.is_err() {
                return Err("local session cleared; server logout uncertain".into());
            }
            Ok(json!({"status":"LOGGED_OUT"}))
        }
    }
}
async fn poll_until_created(
    client: &mut Client,
    qr_code_id: &str,
    deadline: tokio::time::Instant,
) -> Result<(), String> {
    use reqwest::Method;
    use serde_json::json;
    let polling = async {
        loop {
            let data = client
                .request(
                    Method::POST,
                    &format!("{API}/confirm"),
                    Some(json!({"qrCodeId":qr_code_id})),
                )
                .await?;
            if data["connectionStatus"] == "CONNECTED" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        loop {
            let data = client
                .request(Method::GET, &format!("{API}/query"), None)
                .await?;
            if data["connectionStatus"] == "CONNECTED" && data["walletCreateStatus"] == "CREATED" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        Ok(())
    };
    tokio::time::timeout_at(deadline, polling)
        .await
        .map_err(|_| "QR authentication expired or five-minute polling limit reached".to_owned())?
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{SystemTime, UNIX_EPOCH},
    };
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    fn server(
        replies: Vec<(Value, Option<&str>)>,
    ) -> (
        String,
        std::sync::mpsc::Receiver<String>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let replies: Vec<_> = replies
            .into_iter()
            .map(|(v, c)| {
                let body = if v.get("code").is_some() {
                    v
                } else {
                    json!({"code":"000000","data":v})
                }
                .to_string();
                format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n{}\r\n{}",
                    body.len(),
                    c.map(|c| format!("Set-Cookie: agentSessionId={c}; Path=/\r\n"))
                        .unwrap_or_default(),
                    body
                )
            })
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let n = headers
                            .lines()
                            .find_map(|x| x.strip_prefix("content-length: "))
                            .and_then(|x| x.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + n {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        (origin, rx, thread)
    }
    #[tokio::test]
    async fn signed_out_native_file_loads_as_unconnected_without_network() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let credentials = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        let store = Store::open(&dir, false).unwrap();
        store.save("test", &credentials.client_id, "").unwrap();
        drop(store);
        let bytes = std::fs::read(dir.join("session.json")).unwrap();
        let loaded = crate::session::load(&dir, "test").unwrap();
        assert!(loaded.session_id.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client = Client::test_client(
            loaded,
            &format!("http://{}", listener.local_addr().unwrap()),
        )
        .unwrap();
        assert_eq!(
            client
                .execute(&crate::cli::WalletCommand::Status)
                .await
                .unwrap(),
            json!({"status":"UNCONNECTED"})
        );
        assert!(listener.accept().is_err());
        assert_eq!(bytes, std::fs::read(dir.join("session.json")).unwrap());
    }
    #[tokio::test]
    async fn expired_signin_can_start_new_auth_without_sending_expired_cookie() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let credentials = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        {
            let store = Store::open(&dir, false).unwrap();
            store
                .save("test", &credentials.client_id, "expired-token")
                .unwrap();
        }
        let credentials = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        let (origin, rx, thread) = server(vec![
            (
                json!({"code":"100001005","message":"private-expired-message"}),
                None,
            ),
            (
                json!({"connectionStatus":"UNCONNECTED","qrInfo":{"qrCodeId":"renewed-qr","qrCodeUrl":"https://example.invalid/renew","expireAt":now()+120000}}),
                Some("new-pending-token"),
            ),
        ]);
        let mut client = Client::test_client(credentials, &origin).unwrap();
        let out = run(&mut client, &dir, "test", &AuthCommand::Signin)
            .await
            .unwrap();
        assert_eq!(out["qrCodeId"], "renewed-qr");
        thread.join().unwrap();
        assert!(rx
            .recv()
            .unwrap()
            .to_lowercase()
            .contains("cookie: agentsessionid=expired-token"));
        assert!(!rx.recv().unwrap().to_lowercase().contains("cookie:"));
        assert!(!out.to_string().contains("private-expired-message"));
    }
    #[tokio::test]
    async fn signin_refuses_active_pending_without_replacing_qr_or_connecting() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        {
            let store = Store::open(&dir, false).unwrap();
            pending_write(
                &store,
                "test",
                &Pending {
                    client: c.client_id.to_string(),
                    cookie: "staged-token".into(),
                    baseline: store.baseline().unwrap(),
                    qr_code_id: "existing-qr".into(),
                    expire_at: now() + 120000,
                },
            )
            .unwrap();
        }
        let before = std::fs::read(dir.join("auth-pending")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client =
            Client::test_client(c, &format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            run(&mut client, &dir, "test", &AuthCommand::Signin),
        )
        .await;
        assert!(result.is_ok(), "active pending signin attempted network");
        assert!(result.unwrap().unwrap_err().contains("pending"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(before, std::fs::read(dir.join("auth-pending")).unwrap());
    }
    #[tokio::test]
    async fn failed_remote_signout_still_clears_local_and_explicitly_reports_uncertainty() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        {
            let store = Store::open(&dir, false).unwrap();
            store.save("test", &c.client_id, "logged-in-token").unwrap();
            pending_write(
                &store,
                "test",
                &Pending {
                    client: c.client_id.to_string(),
                    cookie: "pending-token".into(),
                    baseline: store.baseline().unwrap(),
                    qr_code_id: "qr".into(),
                    expire_at: now() + 10000,
                },
            )
            .unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let c = bootstrap(&dir, "test", &AuthCommand::Signout).unwrap();
        let mut client = Client::test_client(c, &origin).unwrap();
        let error = run(&mut client, &dir, "test", &AuthCommand::Signout)
            .await
            .unwrap_err();
        assert!(
            bootstrap(&dir, "test", &AuthCommand::Signout)
                .unwrap()
                .session_id
                .is_empty(),
            "remote failure prevented local cleanup"
        );
        assert!(
            error.contains("local session cleared") && error.contains("server logout uncertain")
        );
        assert!(client.credentials().session_id.is_empty());
        assert!(!dir.join("auth-pending").exists());
    }
    #[tokio::test]
    async fn signout_bodyless_remote_success_clears_local_but_preserves_identity() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        Store::open(&dir, false)
            .unwrap()
            .save("test", &c.client_id, "logged-in-token")
            .unwrap();
        let c = bootstrap(&dir, "test", &AuthCommand::Signout).unwrap();
        let id = c.client_id.clone();
        let (origin, requests, thread) = server(vec![(json!({}), None)]);
        let mut client = Client::test_client(c, &origin).unwrap();
        let result = run(&mut client, &dir, "test", &AuthCommand::Signout).await;
        assert_eq!(result.unwrap(), json!({"status":"LOGGED_OUT"}));
        let req = requests.recv().unwrap();
        thread.join().unwrap();
        assert!(req.starts_with(
            "POST /bapi/defi/v1/public/wallet-direct/agent-wallet/login/logout HTTP/1.1"
        ));
        assert!(req.ends_with("\r\n\r\n"));
        let c = bootstrap(&dir, "test", &AuthCommand::Signout).unwrap();
        assert!(c.session_id.is_empty());
        assert_eq!(*c.client_id, *id);
        assert!(client.credentials().session_id.is_empty());
    }
    #[tokio::test]
    async fn signin_persists_five_minute_cap_even_across_verify_restarts() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        let started = now();
        let expiry = started + 3600000;
        let (origin, requests, thread) = server(vec![(
            json!({"connectionStatus":"UNCONNECTED","qrInfo":{"qrCodeId":"qr","qrCodeUrl":"https://example.invalid/qr","expireAt":expiry.to_string()}}),
            Some("staged"),
        )]);
        let mut client = Client::test_client(c, &origin).unwrap();
        let out = run(&mut client, &dir, "test", &AuthCommand::Signin)
            .await
            .unwrap();
        requests.recv().unwrap();
        thread.join().unwrap();
        assert_eq!(out["expireAt"], expiry.to_string());
        let store = Store::open(&dir, false).unwrap();
        let p = pending_read(&store, "test").unwrap();
        assert!(
            p.expire_at <= now() + 300000 && p.expire_at >= started + 300000,
            "five-minute cap not persisted"
        );
    }
    #[tokio::test]
    async fn polling_deadline_cancels_inflight_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let c = Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        let mut client =
            Client::test_client(c, &format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(30);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            poll_until_created(&mut client, "qr", deadline),
        )
        .await;
        assert!(
            result.is_ok(),
            "poll deadline did not cancel stalled HTTP request"
        );
        assert!(result.unwrap().unwrap_err().contains("expired"));
    }
    #[tokio::test]
    async fn expired_verify_does_not_connect_or_commit() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        {
            let store = Store::open(&dir, false).unwrap();
            pending_write(
                &store,
                "test",
                &Pending {
                    client: c.client_id.to_string(),
                    cookie: "staged-token".into(),
                    baseline: store.baseline().unwrap(),
                    qr_code_id: "expired".into(),
                    expire_at: now() - 1,
                },
            )
            .unwrap();
        }
        let before = std::fs::read(dir.join("session.json")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client =
            Client::test_client(c, &format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            run(
                &mut client,
                &dir,
                "test",
                &AuthCommand::Verify {
                    qr_code_id: "expired".into(),
                },
            ),
        )
        .await;
        assert!(
            result.is_ok(),
            "expired verify attempted network instead of failing immediately"
        );
        assert!(result.unwrap().unwrap_err().contains("expired"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(before, std::fs::read(dir.join("session.json")).unwrap());
    }
    #[tokio::test]
    async fn connected_signin_only_queries_without_replacing_identity_or_persisting_rotation() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        Store::open(&dir, false)
            .unwrap()
            .save("test", &c.client_id, "original-token")
            .unwrap();
        let before = std::fs::read(dir.join("session.json")).unwrap();
        let (origin, requests, thread) = server(vec![(
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            Some("rotation-not-committed"),
        )]);
        let c = bootstrap(&dir, "test", &AuthCommand::Signin).unwrap();
        let id = c.client_id.clone();
        let mut client = Client::test_client(c, &origin).unwrap();
        let result = run(&mut client, &dir, "test", &AuthCommand::Signin).await;
        let request = requests.recv().unwrap();
        thread.join().unwrap();
        assert_eq!(result.unwrap(), json!({"status":"ALREADY_CONNECTED"}));
        assert!(request.starts_with(
            "GET /bapi/defi/v1/public/wallet-direct/agent-wallet/login/query HTTP/1.1"
        ));
        assert_eq!(*client.credentials().client_id, *id);
        assert_eq!(before, std::fs::read(dir.join("session.json")).unwrap());
        assert!(!dir.join("auth-pending").exists());
    }
    #[tokio::test]
    async fn signin_then_separate_verify_preserves_identity_and_commits_only_created_cookie() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("new-wallet");
        let expiry = now() + 120000;
        let (origin, requests, thread) = server(vec![
            (
                json!({"connectionStatus":"UNCONNECTED","qrInfo":{"qrCodeId":"qr-test","qrCodeUrl":"https://example.invalid/qr","expireAt":expiry}}),
                Some("login-pending"),
            ),
            (
                json!({"connectionStatus":"CONNECTED"}),
                Some("confirm-pending"),
            ),
            (
                json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
                Some("created-token"),
            ),
        ]);
        let c = bootstrap(&dir, "synthetic-instance", &AuthCommand::Signin).unwrap();
        let id = c.client_id.clone();
        let mut client = Client::test_client(c, &origin).unwrap();
        let out = run(
            &mut client,
            &dir,
            "synthetic-instance",
            &AuthCommand::Signin,
        )
        .await
        .unwrap();
        let req = requests.recv().unwrap();
        assert!(
            req.starts_with("POST /bapi/defi/v1/public/wallet-direct/agent-wallet/login HTTP/1.1")
        );
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["isFirstLogin"], true);
        assert_eq!(body["os"], "Linux");
        let public = body["publicKeyHex"].as_str().unwrap();
        assert_eq!(public.len(), 64);
        let compressed = crate::auth_store::hex(&[2]) + public;
        let sec1: Vec<u8> = (0..compressed.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&compressed[i..i + 2], 16).unwrap())
            .collect();
        assert!(k256::PublicKey::from_sec1_bytes(&sec1).is_ok());
        assert_eq!(
            out,
            json!({"qrCodeId":"qr-test","expireAt":expiry,"urlForWeb":"https://example.invalid/qr","pairingCode":format!("{}{}",&public[..3],&public[61..])})
        );
        let stored = bootstrap(
            &dir,
            "synthetic-instance",
            &AuthCommand::Verify {
                qr_code_id: "qr-test".into(),
            },
        )
        .unwrap();
        assert_eq!(*stored.client_id, *id);
        assert!(stored.session_id.is_empty());
        drop(client); // No process memory is needed by verify.
        let verify = AuthCommand::Verify {
            qr_code_id: "qr-test".into(),
        };
        let mut client = Client::test_client(stored, &origin).unwrap();
        assert_eq!(
            run(&mut client, &dir, "synthetic-instance", &verify)
                .await
                .unwrap(),
            json!({"status":"SUCCESS"})
        );
        assert!(requests
            .recv()
            .unwrap()
            .contains("agentSessionId=login-pending"));
        assert!(requests
            .recv()
            .unwrap()
            .contains("agentSessionId=confirm-pending"));
        thread.join().unwrap();
        let c = crate::session::load(&dir, "synthetic-instance").unwrap();
        assert_eq!(*c.client_id, *id);
        assert_eq!(c.session_id.as_str(), "created-token");
        assert!(!dir.join("auth-pending").exists());
    }
}
