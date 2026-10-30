//! Synthetic loopback-only resident tests. No production origin is reachable here.
use super::*;
use std::os::unix::fs::MetadataExt;
use tokio::sync::{mpsc, oneshot};
type Reply = (Value, Option<String>);
struct Mock {
    origin: String,
    requests: mpsc::UnboundedReceiver<(String, oneshot::Sender<Reply>)>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Mock {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    loop {
                        let mut buf = [0; 4096];
                        let n = stream.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                            let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                            let len = h
                                .lines()
                                .find_map(|s| s.strip_prefix("content-length: "))
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + len {
                                break;
                            }
                        }
                    }
                    let (reply_tx, reply_rx) = oneshot::channel::<Reply>();
                    if tx
                        .send((String::from_utf8(bytes).unwrap(), reply_tx))
                        .is_err()
                    {
                        return;
                    }
                    if let Ok((data, cookie)) = reply_rx.await {
                        let body = if data.get("code").is_some() {
                            data
                        } else {
                            json!({"code":"000000","data":data})
                        }
                        .to_string();
                        let cookie = cookie
                            .map(|s| format!("Set-Cookie: agentSessionId={s}; Path=/\r\n"))
                            .unwrap_or_default();
                        let _=stream.write_all(format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n{cookie}\r\n{body}",body.len()).as_bytes()).await;
                    }
                });
            }
        });
        Self {
            origin,
            requests,
            task,
        }
    }
    async fn next(&mut self) -> (String, oneshot::Sender<Reply>) {
        tokio::time::timeout(Duration::from_secs(15), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }
}
fn save(dir: &Path, instance: &str, id: &str, cookie: &str) {
    let key = session::derive_key(instance);
    let bytes = serde_json::to_vec(
        &json!({"clientId":id,"sessionId":crate::auth_store::encrypt(cookie,&key).unwrap()}),
    )
    .unwrap();
    std::fs::write(dir.join("session.json"), bytes).unwrap();
    std::fs::set_permissions(
        dir.join("session.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}
fn wallet(
    root: &Path,
    name: &str,
    instance: &str,
    id: &str,
    cookie: Option<&str>,
    origin: &str,
) -> Arc<Wallet> {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let key = root.join(format!("{name}.key"));
    std::fs::write(&key, instance).unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    if let Some(cookie) = cookie {
        save(&dir, instance, id, cookie);
    }
    let meta = std::fs::metadata(&dir).unwrap();
    let mut w = Wallet::new(Entry {
        dir,
        instance_file: key,
        device: meta.dev(),
        inode: meta.ino(),
    })
    .unwrap();
    w.origin = Some(origin.into());
    Arc::new(w)
}
fn argv(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}
const A: &str = "11111111-1111-4111-8111-111111111111";
const B: &str = "22222222-2222-4222-8222-222222222222";
async fn start(
    root: &Path,
    wallets: BTreeMap<String, Arc<Wallet>>,
) -> (PathBuf, tokio::task::JoinHandle<Result<Value, String>>) {
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("test.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let task = tokio::spawn(serve_listener(listener, Arc::new(wallets)));
    (path, task)
}
fn request(
    path: &Path,
    name: &str,
    args: &[&str],
) -> tokio::task::JoinHandle<Result<Value, String>> {
    let path = path.to_owned();
    let name = name.to_owned();
    let argv = argv(args);
    tokio::spawn(async move { send(&path, Request::Run { name, argv }).await })
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_wallets_concurrent_isolated_and_same_wallet_fifo() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let b = wallet(
        root.path(),
        "b",
        "instance-b",
        B,
        Some("cookie-b"),
        &mock.origin,
    );
    let (path, server) = start(
        root.path(),
        BTreeMap::from([("alice".into(), a), ("bob".into(), b)]),
    )
    .await;
    let first = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "a-write-1",
            "auth",
            "signout",
        ],
    );
    let (one, reply1) = mock.next().await;
    assert!(one.contains(A));
    assert!(one.to_lowercase().contains("agentsessionid=cookie-a"));
    let second = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "a-write-2",
            "auth",
            "signout",
        ],
    );
    // Alice holds its write lock while Bob's independent client still completes.
    let bob = request(&path, "bob", &["wallet", "status"]);
    let (two, reply2) = mock.next().await;
    assert!(two.contains(B));
    assert!(!two.contains(A));
    assert!(two.to_lowercase().contains("agentsessionid=cookie-b"));
    reply2
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    assert_eq!(bob.await.unwrap().unwrap()["status"], "CONNECTED");
    assert!(!first.is_finished());
    assert!(!second.is_finished());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), mock.requests.recv())
            .await
            .is_err()
    );
    reply1.send((json!({}), None)).unwrap();
    assert_eq!(first.await.unwrap().unwrap()["status"], "LOGGED_OUT");
    let (three, reply3) = mock.next().await;
    assert!(three.contains("/login/logout"));
    assert!(!three.to_lowercase().contains("cookie:"));
    reply3.send((json!({}), None)).unwrap();
    assert_eq!(second.await.unwrap().unwrap()["status"], "LOGGED_OUT");
    assert_eq!(
        request(&path, "alice", &["wallet", "status"])
            .await
            .unwrap()
            .unwrap()["status"],
        "UNCONNECTED"
    );
    assert!(send(
        &path,
        Request::Run {
            name: "ALICE".into(),
            argv: argv(&["wallet", "status"])
        }
    )
    .await
    .unwrap_err()
    .contains("unknown wallet"));
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
fn legacy_client_only(w: &Wallet) -> Vec<u8> {
    let bytes = serde_json::to_vec(&json!({"clientId": A})).unwrap();
    let path = w.entry.dir.join("session.json");
    std::fs::write(&path, &bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_legacy_client_only_signin_then_verify() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(root.path(), "a", "instance-a", A, None, &mock.origin);
    let before = legacy_client_only(&a);
    let dir = a.entry.dir.clone();
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a)])).await;
    let signin = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "legacy-signin",
            "auth",
            "signin",
        ],
    );
    let (req, reply) = mock.next().await;
    assert!(req.contains(A));
    assert!(!req.to_lowercase().contains("cookie:"));
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 120000;
    reply.send((json!({"connectionStatus":"UNCONNECTED","qrInfo":{"qrCodeId":"legacy-qr","qrCodeUrl":"https://example.invalid/qr","expireAt":expiry}}), Some("legacy-pending".into()))).unwrap();
    assert_eq!(signin.await.unwrap().unwrap()["qrCodeId"], "legacy-qr");
    assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), before);
    assert!(dir.join("auth-pending").exists());
    let mut verify = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "legacy-verify",
            "auth",
            "verify",
            "--qrCodeId",
            "legacy-qr",
        ],
    );
    let (req, reply) = tokio::select! {
        req = mock.next() => req,
        result = &mut verify => panic!("legacy verify returned before confirmation request: {result:?}"),
    };
    assert!(req.contains("/login/confirm"));
    assert!(req.contains("legacy-qr"));
    assert!(req.contains(A));
    assert!(req.to_lowercase().contains("agentsessionid=legacy-pending"));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED"}),
            Some("legacy-confirmed".into()),
        ))
        .unwrap();
    let (req, reply) = mock.next().await;
    assert!(req.contains("/login/query"));
    assert!(req
        .to_lowercase()
        .contains("agentsessionid=legacy-confirmed"));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    assert_eq!(verify.await.unwrap().unwrap()["status"], "SUCCESS");
    let c = session::load(&dir, "instance-a").unwrap();
    assert_eq!(c.client_id.as_str(), A);
    assert_eq!(c.session_id.as_str(), "legacy-confirmed");
    assert!(!dir.join("auth-pending").exists());
    assert!(session::load(&dir, "instance-b").is_err());
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_legacy_client_only_signout() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(root.path(), "a", "instance-a", A, None, &mock.origin);
    legacy_client_only(&a);
    let dir = a.entry.dir.clone();
    let (path, server) = start(root.path(), BTreeMap::from([("alice".into(), a)])).await;
    let mut signout = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "legacy-signout",
            "auth",
            "signout",
        ],
    );
    let (req, reply) = tokio::select! {
        req = mock.next() => req,
        result = &mut signout => panic!("legacy signout returned before logout request: {result:?}"),
    };
    assert!(req.contains("/login/logout"));
    assert!(req.contains(A));
    assert!(!req.to_lowercase().contains("cookie:"));
    reply.send((json!({}), None)).unwrap();
    assert_eq!(signout.await.unwrap().unwrap()["status"], "LOGGED_OUT");
    let c = session::load(&dir, "instance-a").unwrap();
    assert_eq!(c.client_id.as_str(), A);
    assert!(c.session_id.is_empty());
    assert_eq!(
        request(&path, "alice", &["wallet", "status"])
            .await
            .unwrap()
            .unwrap()["status"],
        "UNCONNECTED"
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_signin_verify_polling_and_auth_refresh() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let a = wallet(root.path(), "a", "instance-a", A, None, &mock.origin);
    let b = wallet(
        root.path(),
        "b",
        "instance-b",
        B,
        Some("cookie-b"),
        &mock.origin,
    );
    let dir = a.entry.dir.clone();
    let (path, server) = start(
        root.path(),
        BTreeMap::from([("alice".into(), a.clone()), ("bob".into(), b)]),
    )
    .await;
    let signin = request(
        &path,
        "alice",
        &["--confirm", "--operation-id", "signin-a", "auth", "signin"],
    );
    let (req, reply) = mock.next().await;
    assert!(!req.to_lowercase().contains("cookie:"));
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 120000;
    reply.send((json!({"connectionStatus":"UNCONNECTED","qrInfo":{"qrCodeId":"qr-a","qrCodeUrl":"https://example.invalid/qr-a","expireAt":expiry}}),Some("pending-a".into()))).unwrap();
    assert_eq!(signin.await.unwrap().unwrap()["qrCodeId"], "qr-a");
    assert!(
        a.state.lock().await.client.is_none(),
        "pending cookie retained outside encrypted auth state"
    );
    let verify = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "verify-a",
            "auth",
            "verify",
            "--qrCodeId",
            "qr-a",
        ],
    );
    let (req, reply) = mock.next().await;
    assert!(req.contains("qr-a"));
    assert!(req.to_lowercase().contains("agentsessionid=pending-a"));
    let pending_reply = reply;
    let bob = request(&path, "bob", &["wallet", "status"]);
    let (req, reply) = mock.next().await;
    assert!(req.contains(B));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    assert_eq!(bob.await.unwrap().unwrap()["status"], "CONNECTED");
    assert!(!verify.is_finished());
    pending_reply
        .send((json!({"connectionStatus":"UNCONNECTED"}), None))
        .unwrap();
    let (_, reply) = mock.next().await;
    reply
        .send((
            json!({"connectionStatus":"CONNECTED"}),
            Some("verified-a".into()),
        ))
        .unwrap();
    let (_, reply) = mock.next().await;
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    assert_eq!(verify.await.unwrap().unwrap()["status"], "SUCCESS");
    let status = request(&path, "alice", &["wallet", "status"]);
    let (req, reply) = mock.next().await;
    assert!(req.to_lowercase().contains("agentsessionid=verified-a"));
    assert!(!req.contains("pending-a"));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    status.await.unwrap().unwrap();
    let signout = request(
        &path,
        "alice",
        &[
            "--confirm",
            "--operation-id",
            "signout-a",
            "auth",
            "signout",
        ],
    );
    let (_, reply) = mock.next().await;
    reply
        .send((
            json!({"code":"1200001","message":"synthetic failure"}),
            None,
        ))
        .unwrap();
    assert!(signout
        .await
        .unwrap()
        .unwrap_err()
        .contains("local session cleared"));
    assert_eq!(
        request(&path, "alice", &["wallet", "status"])
            .await
            .unwrap()
            .unwrap()["status"],
        "UNCONNECTED"
    );
    let c = session::load(&dir, "instance-a").unwrap();
    assert!(c.session_id.is_empty());
    assert!(session::load(&dir, "instance-b").is_err());
    // A cold authorized writer changed this wallet; resident must reload and not retain logout/old cookies.
    save(&dir, "instance-a", &c.client_id, "external-a");
    let status = request(&path, "alice", &["wallet", "status"]);
    let (req, reply) = mock.next().await;
    assert!(req.to_lowercase().contains("agentsessionid=external-a"));
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    status.await.unwrap().unwrap();
    std::fs::remove_file(dir.join("session.json")).unwrap();
    assert!(request(&path, "alice", &["wallet", "status"])
        .await
        .unwrap()
        .is_err());
    assert!(a.state.lock().await.client.is_none());
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}
#[tokio::test]
async fn ipc_frames_are_bounded_and_idle_connections_time_out() {
    for (len, valid) in [(MAX_FRAME, true), (MAX_FRAME + 1, false)] {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        assert!(peer(&reader).is_ok());
        let task = tokio::spawn(async move {
            let mut data = vec![b'x'; len];
            data.push(b'\n');
            writer.write_all(&data).await.unwrap();
        });
        let result = read_frame(&mut reader, MAX_FRAME).await;
        assert_eq!(result.is_ok(), valid);
        task.await.unwrap();
    }
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = tokio::spawn(handle(
        server,
        Arc::new(BTreeMap::new()),
        Arc::new(Notify::new()),
    ));
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        read_frame(&mut client, MAX_RESPONSE),
    )
    .await
    .unwrap()
    .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"]["message"].as_str().unwrap().contains("timeout"));
    task.await.unwrap();
}
#[tokio::test]
async fn daemon_never_replaces_regular_file_at_socket_path() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.path().join("daemon.sock");
    std::fs::write(&path, b"synthetic-sentinel").unwrap();
    assert!(serve(&path, BTreeMap::new())
        .await
        .unwrap_err()
        .contains("non-socket"));
    assert_eq!(std::fs::read(&path).unwrap(), b"synthetic-sentinel");
}
#[tokio::test]
async fn duplicate_resident_lease_and_replaced_directory_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mock = Mock::new().await;
    let w = wallet(root.path(), "a", "instance-a", A, Some(""), &mock.origin);
    assert!(Wallet::new(w.entry.clone()).is_err());
    std::fs::rename(&w.entry.dir, root.path().join("old-a")).unwrap();
    std::fs::create_dir(&w.entry.dir).unwrap();
    std::fs::set_permissions(&w.entry.dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(w
        .execute(argv(&["wallet", "status"]))
        .await
        .unwrap_err()
        .contains("identity changed"));
}
