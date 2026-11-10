//! Real subprocess termination, synthetic wallet files and loopback peers only.
//! No daemon installation, production credentials, external transport or real transaction.
use super::super::task_runtime::{Control, Definition, Scheduler};
use super::*;
use futures_util::{SinkExt, StreamExt};
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use tokio_tungstenite::tungstenite::Message;

const CHILD_TEST: &str = "resident::tests::single_wallet_fault_tests::subprocess_fixture_entry";
const TASK: &str = "single-wallet-crash";
const MODE: &str = "BAW_OFFLINE_FAULT_MODE";
const ROOT: &str = "BAW_OFFLINE_FAULT_ROOT";
const ORIGIN: &str = "BAW_OFFLINE_FAULT_ORIGIN";

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn private_json(path: &std::path::Path, value: &Value) {
    std::fs::write(path, value.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
struct FixtureChild(Option<Child>);
impl std::ops::Deref for FixtureChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        self.0.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for FixtureChild {
    fn deref_mut(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }
}
impl FixtureChild {
    fn output(mut self) -> std::process::Output {
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for FixtureChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn spawn_child(root: &std::path::Path, origin: &str, mode: &str) -> FixtureChild {
    FixtureChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
            .env(MODE, mode)
            .env(ROOT, root)
            .env(ORIGIN, origin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ))
}

// Not ignored: the normal suite returns immediately; only an isolated child
// explicitly selected by the parent enters the fixture. Never read user config.
#[test]
fn subprocess_fixture_entry() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    assert!(matches!(mode.as_str(), "execute" | "recover"));
    let root = std::path::PathBuf::from(std::env::var(ROOT).unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(
        root.parent().unwrap(),
        std::env::temp_dir().canonicalize().unwrap()
    );
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("baw-offline-fault-"));
    let origin = std::env::var(ORIGIN).unwrap();
    let url = reqwest::Url::parse(&origin).unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.port().is_some());
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let dir=root.join("w1");let metadata=std::fs::metadata(&dir).unwrap();
        let mut wallet=Wallet::new(Entry {dir,instance_file:root.join("w1.key"),device:metadata.dev(),inode:metadata.ino()}).unwrap();
        wallet.origin=Some(origin);let wallet=Arc::new(wallet);
        let daemon=Arc::new(Daemon {wallets:Arc::new(RwLock::new(BTreeMap::from([("w1".into(),wallet)]))),registry:root.clone(),scheduler:Arc::new(Scheduler::default())});
        if mode=="execute" {
            let definition:Definition=serde_json::from_value(read_json(&root.join("definition.json"))).unwrap();
            daemon.scheduler.control(daemon.clone(),Control::Load {config:definition}).await.unwrap();
            daemon.scheduler.control(daemon.clone(),Control::Arm {id:TASK.into(),confirm:true}).await.unwrap();
            // Parent kills this OS process after receiving the full Execute POST.
            tokio::time::sleep(Duration::from_secs(90)).await;
            panic!("parent did not terminate fixture after Execute");
        }
        let before=read_json(&root.join(format!("pipeline-{TASK}.json")));
        let status=daemon.scheduler.control(daemon.clone(),Control::Status {id:TASK.into()}).await.unwrap();
        assert_eq!(status["status"],"INTERRUPTED");
        assert_eq!(status["snapshot"]["outcomes"],json!([["uncertain"]]));
        assert!(daemon.scheduler.control(daemon.clone(),Control::Resume{id:TASK.into(),confirm:true}).await.is_err());
        let reconciliation=daemon.scheduler.control(daemon.clone(),Control::Reconcile{id:TASK.into()}).await.unwrap();
        assert_eq!(reconciliation["writesSubmitted"],0);
        assert_eq!(reconciliation["observations"][0]["status"],"UNRESOLVED");
        assert_eq!(reconciliation["status"],"PAUSED");
        assert!(daemon.scheduler.control(daemon.clone(),Control::Resume{id:TASK.into(),confirm:true}).await.is_err());
        assert!(daemon.scheduler.control(daemon.clone(),Control::Arm{id:TASK.into(),confirm:true}).await.is_err());
        let after=read_json(&root.join(format!("pipeline-{TASK}.json")));
        assert!(before["recovery"]["deadlines"].is_object());
        assert!(before["recovery"]["deadlines"]["0"].as_u64().is_some());
        assert_eq!(before["recovery"]["deadlines"],after["recovery"]["deadlines"]);
        private_json(&root.join("recovery-report.json"),&json!({"reconciliation":reconciliation,"resumeRejectedBeforeAndAfterReconcile":true,"armRejected":true,"deadlinesUnchanged":true}));
    });
}

#[tokio::test]
async fn sigkill_after_execute_received_restart_and_reconcile_never_replays() {
    let root = tempfile::Builder::new()
        .prefix("baw-offline-fault-")
        .tempdir()
        .unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut backend = Mock::new().await;
    // Generate disposable encrypted fixture data, then release the directory lease
    // so the independent child must acquire and later recover that real OS lock.
    drop(wallet(
        root.path(),
        "w1",
        "fault-instance",
        A,
        Some("synthetic-only"),
        &backend.origin,
    ));
    let identity_before = std::fs::read(root.path().join("w1.key")).unwrap();
    let session_before = std::fs::read(root.path().join("w1/session.json")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = format!("ws://{}", listener.local_addr().unwrap());
    let ws = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request = socket.next().await.unwrap().unwrap();
        let request: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
        assert_eq!(
            request["method"], "eth_chainId",
            "reconciliation must not invent a receipt/hash lookup"
        );
        socket
            .send(Message::Text(
                json!({"jsonrpc":"2.0","id":request["id"],"result":"0x38"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        while let Some(frame) = socket.next().await {
            match frame {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(frame) => assert!(!frame.is_text(), "unexpected extra RPC: {frame}"),
            }
        }
    });
    private_json(
        &root.path().join("definition.json"),
        &json!({"id":TASK,"chain_id":"56","rpc_url":rpc,"steps":[{"task_id":"only-step","trigger":{"kind":"immediate"},"valid_for_ms":60000,"completion":"submitted","confirmations":1,"actions":[{"wallet":"w1","argv":["contract-call","fire","--binanceChainId","56","--from",fixtures::SENDER,"--to","0x2222222222222222222222222222222222222222","--value","0","--inputData","0x12345678"]}]}]}),
    );
    let mut child = spawn_child(root.path(), &backend.origin, "execute");
    let received=tokio::time::timeout(Duration::from_secs(30),async {
        loop {
            let (request,reply)=backend.next().await;let line=request.lines().next().unwrap();
            let result=if line.contains("settings/query") {json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})}
            else if line.contains("mpc-wallet/list") {json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":fixtures::SENDER}]}]}])}
            else if line.contains("networks/active") {json!([])}
            else if line.contains("contract-call/preview") {fixtures::valid_preview("crash-preview")}
            else if line.contains("contract-call/execute") {
                let body:Value=serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                assert_eq!(body["requestId"],"crash-preview");break reply;
            }else {panic!("unexpected execution request {line}")};
            reply.send((result,None)).unwrap();
        }
    }).await;
    if received.is_err() {
        let _ = child.kill();
        let output = child.output();
        panic!(
            "fixture did not reach Execute: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let reply = received.unwrap();
    let admitted = read_json(&root.path().join(format!("pipeline-{TASK}.json")));
    assert_eq!(admitted["status"], "RUNNING");
    assert_eq!(admitted["snapshot"]["outcomes"], json!([["in_flight"]]));
    assert_eq!(admitted["recovery"]["triggered"], json!([0]));
    // No simulated reply: acceptance may have side effects. SIGKILL cannot run Drop.
    child.kill().unwrap();
    let terminated = child.wait().unwrap();
    assert!(!terminated.success());
    drop(reply);
    let mut recovered = spawn_child(root.path(), &backend.origin, "recover");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if recovered.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = recovered.kill();
            let _ = recovered.wait();
            panic!("recovery subprocess timed out");
        }
        tokio::select! {
            row=backend.requests.recv()=>{let _=recovered.kill();let _=recovered.wait();panic!("restart/reconcile replayed wallet HTTP: {}",row.unwrap().0.lines().next().unwrap());},
            _=tokio::time::sleep(Duration::from_millis(10))=>{},
        }
    }
    let output = recovered.output();
    assert!(
        output.status.success(),
        "child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(backend.requests.try_recv().is_err());
    assert_eq!(
        read_json(&root.path().join("recovery-report.json"))["reconciliation"]["status"],
        "PAUSED"
    );
    assert_eq!(
        std::fs::read(root.path().join("w1.key")).unwrap(),
        identity_before
    );
    assert_eq!(
        std::fs::read(root.path().join("w1/session.json")).unwrap(),
        session_before
    );
    tokio::time::timeout(Duration::from_secs(3), ws)
        .await
        .unwrap()
        .unwrap();
}
