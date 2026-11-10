//! Synthetic encrypted sessions, local private files and loopback HTTP only.
use super::super::task_runtime::{Control, Definition, Scheduler};
use super::*;
fn private(path: &Path, value: Value) {
    std::fs::write(path, value.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
async fn setup() -> (tempfile::TempDir, Arc<Daemon>, Arc<Wallet>, Mock) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let backend = Mock::new().await;
    let wallet = wallet(
        root.path(),
        "w1",
        "synthetic-identity",
        A,
        Some("synthetic-session"),
        &backend.origin,
    );
    private(
        &root.path().join("registry.json"),
        json!({"w1":wallet.entry.clone()}),
    );
    let daemon = Arc::new(Daemon {
        wallets: Arc::new(RwLock::new(BTreeMap::from([("w1".into(), wallet.clone())]))),
        registry: root.path().into(),
        scheduler: Arc::new(Scheduler::default()),
    });
    (root, daemon, wallet, backend)
}
fn definition() -> Definition {
    serde_json::from_value(json!({"id":"inspection-demo","chain_id":"56","steps":[{"task_id":"future","trigger":{"kind":"time","unix_ms":4102444800000u64},"completion":"submitted","valid_for_ms":60000,"actions":[{"wallet":"w1","argv":["contract-call","fire","--binanceChainId","56","--from",fixtures::SENDER,"--to","0x2222222222222222222222222222222222222222","--value","0"]}]}]})).unwrap()
}
#[tokio::test]
async fn overview_default_is_local_and_never_loads_financial_client_or_updates_credentials() {
    let (root, daemon, wallet, mut backend) = setup().await;
    let before = std::fs::read(root.path().join("w1/session.json")).unwrap();
    let result = super::super::overview::run(&daemon, false).await.unwrap();
    assert_eq!(
        result["wallets"][0]["remote"]["readStatus"],
        "not_requested"
    );
    assert_eq!(
        result["wallets"][0]["remote"]["connectionStatus"],
        "NOT_REFRESHED"
    );
    assert_eq!(result["wallets"][0]["local"]["sessionFilePresent"], true);
    assert!(wallet.state.lock().await.client.is_none());
    assert_eq!(
        std::fs::read(root.path().join("w1/session.json")).unwrap(),
        before
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
            .await
            .is_err()
    );
    assert!(!result.to_string().contains("synthetic"));
    assert_eq!(result["chainQueries"], 0);
}
#[tokio::test]
async fn explicit_overview_refresh_is_two_gets_sanitized_and_does_not_persist_rotated_cookie() {
    let (root, daemon, wallet, mut backend) = setup().await;
    let session = std::fs::read(root.path().join("w1/session.json")).unwrap();
    let identity = std::fs::read(root.path().join("w1.key")).unwrap();
    let query = tokio::spawn(async move {
        let (request, reply) = backend.next().await;
        assert!(
            request.starts_with("GET /bapi/defi/v1/public/wallet-direct/agent-wallet/login/query ")
        );
        reply.send((json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED","sessionExpireTime":"1794000000000","private":"secret-login-response"}),Some("synthetic-rotated-cookie".into()))).unwrap();
        let (request, reply) = backend.next().await;
        assert!(request
            .starts_with("GET /bapi/defi/v1/public/wallet-direct/agent-wallet/settings/query "));
        assert!(request.contains("synthetic-rotated-cookie"));
        reply.send((json!({"devMode":{"enabled":true,"expiresAt":4102444800u64},"dailyLimit":"50000.123456789123456789","private":"secret-settings-response"}),None)).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
                .await
                .is_err()
        );
    });
    let result = super::super::overview::run(&daemon, true).await.unwrap();
    query.await.unwrap();
    let remote = &result["wallets"][0]["remote"];
    assert_eq!(remote["readStatus"], "session_rotation_not_persisted");
    assert_eq!(remote["connectionStatus"], "UNKNOWN");
    assert_eq!(remote["observedConnectionStatus"], "CONNECTED");
    assert_eq!(remote["developerMode"]["expired"], false);
    assert_eq!(remote["sessionExpireTimeAsReported"], "1794000000000");
    assert_eq!(
        remote["dailyLimitUsdAsReported"],
        "50000.123456789123456789"
    );
    assert_eq!(remote["sessionRotationObserved"], true);
    assert_eq!(
        std::fs::read(root.path().join("w1/session.json")).unwrap(),
        session
    );
    assert_eq!(std::fs::read(root.path().join("w1.key")).unwrap(), identity);
    assert!(wallet.state.lock().await.client.is_none());
    for secret in [
        "synthetic",
        "secret-login",
        "secret-settings",
        "session.json",
        "w1.key",
    ] {
        assert!(!result.to_string().contains(secret));
    }
}
#[tokio::test]
async fn overview_never_queues_behind_a_busy_wallet_or_refreshes_an_armed_task() {
    let (root, daemon, wallet, mut backend) = setup().await;
    let guard = wallet.state.lock().await;
    let result = super::super::overview::run(&daemon, true).await.unwrap();
    assert_eq!(
        result["wallets"][0]["remote"]["readStatus"],
        "skipped_wallet_busy"
    );
    drop(guard);
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Load {
                config: definition(),
            },
        )
        .await
        .unwrap();
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "inspection-demo".into(),
                confirm: true,
            },
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let status = daemon
                .scheduler
                .control(
                    daemon.clone(),
                    Control::Status {
                        id: "inspection-demo".into(),
                    },
                )
                .await
                .unwrap();
            if status["results"]["phase"]["status"] == "WAITING" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let before = std::fs::read(root.path().join("pipeline-inspection-demo.json")).unwrap();
    let result = super::super::overview::run(&daemon, true).await.unwrap();
    assert_eq!(
        result["wallets"][0]["remote"]["readStatus"],
        "skipped_active_or_interrupted_task"
    );
    assert_eq!(result["tasks"][0]["phase"], "WAITING");
    assert_eq!(result["tasks"][0]["active"], true);
    assert_eq!(
        std::fs::read(root.path().join("pipeline-inspection-demo.json")).unwrap(),
        before
    );
    assert!(backend.requests.try_recv().is_err());
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Cancel {
                id: "inspection-demo".into(),
            },
        )
        .await
        .unwrap();
    daemon.scheduler.stop().await;
}
#[tokio::test]
async fn overview_reports_uncertain_corrupt_and_hashless_journals_without_raw_payloads() {
    let (root, daemon, _wallet, _backend) = setup().await;
    let dir = root.path().join("w1/.baw-rs-operations");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    private(
        &dir.join("uncertain.json"),
        json!({"state":"SUBMITTING","intent":"secret-intent","error":"https://private.example?key=secret"}),
    );
    private(
        &dir.join("hashless.json"),
        json!({"state":"RESPONSE_RECEIVED","action":"contract-call.execute","recovery":{"status":"BROADCASTED","orderId":"secret-order"}}),
    );
    private(&dir.join("corrupt.json"), json!([]));
    let result = super::super::overview::run(&daemon, false).await.unwrap();
    let rows = result["wallets"][0]["local"]["unresolvedOperations"]["operations"]
        .as_array()
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["retrySafe"] == false));
    for secret in ["secret", "private.example", "intentSha"] {
        assert!(!result.to_string().contains(secret));
    }
}
#[tokio::test]
async fn refresh_reservation_blocks_new_task_admission_without_consuming_consent_or_deadline() {
    let (root, daemon, wallet, mut backend) = setup().await;
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Load {
                config: definition(),
            },
        )
        .await
        .unwrap();
    let task_before = std::fs::read(root.path().join("pipeline-inspection-demo.json")).unwrap();
    let inspecting = daemon.clone();
    let job = tokio::spawn(async move {
        super::super::overview::run(&inspecting, true)
            .await
            .unwrap()
    });
    let (request, reply) = backend.next().await;
    assert!(request.contains("/login/query"));
    assert!(
        wallet.state.try_lock().is_err(),
        "refresh must retain the per-wallet reservation through HTTP"
    );
    let error = daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "inspection-demo".into(),
                confirm: true,
            },
        )
        .await
        .unwrap_err();
    assert!(error.contains("overview refresh"));
    assert_eq!(
        std::fs::read(root.path().join("pipeline-inspection-demo.json")).unwrap(),
        task_before
    );
    reply
        .send((
            json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
            None,
        ))
        .unwrap();
    let (request, reply) = backend.next().await;
    assert!(request.contains("/settings/query"));
    reply
        .send((
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            None,
        ))
        .unwrap();
    assert_eq!(
        job.await.unwrap()["wallets"][0]["remote"]["readStatus"],
        "ok"
    );
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "inspection-demo".into(),
                confirm: true,
            },
        )
        .await
        .unwrap();
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Cancel {
                id: "inspection-demo".into(),
            },
        )
        .await
        .unwrap();
    daemon.scheduler.stop().await;
    assert!(backend.requests.try_recv().is_err());
}
#[tokio::test]
async fn refresh_does_not_hot_load_unloaded_wallet_or_create_its_lease() {
    let (root, daemon, wallet, mut backend) = setup().await;
    daemon.wallets.write().await.clear();
    drop(wallet);
    let lease = root.path().join("w1/.baw-rs-resident.lock");
    std::fs::remove_file(&lease).unwrap();
    let result = super::super::overview::run(&daemon, true).await.unwrap();
    assert_eq!(result["wallets"][0]["loaded"], false);
    assert_eq!(
        result["wallets"][0]["remote"]["readStatus"],
        "skipped_wallet_not_loaded"
    );
    assert!(!lease.exists());
    assert!(daemon.wallets.read().await.is_empty());
    assert!(backend.requests.try_recv().is_err());
}

#[tokio::test]
async fn cookie_change_is_not_reported_connected_even_if_settings_read_fails() {
    let (_root, daemon, _wallet, mut backend) = setup().await;
    let job = tokio::spawn(async move {
        let (_, reply) = backend.next().await;
        reply
            .send((
                json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
                Some("synthetic-rotated-cookie".into()),
            ))
            .unwrap();
        let (_, reply) = backend.next().await;
        reply
            .send((
                json!({"code":"351805","message":"sensitive settings failure"}),
                None,
            ))
            .unwrap();
    });
    let value = super::super::overview::run(&daemon, true).await.unwrap();
    job.await.unwrap();
    assert_eq!(value["wallets"][0]["remote"]["connectionStatus"], "UNKNOWN");
    assert_eq!(
        value["wallets"][0]["remote"]["readStatus"],
        "session_rotation_not_persisted"
    );
    assert!(!value.to_string().contains("sensitive settings failure"));
}

#[tokio::test]
async fn refresh_failure_is_partial_without_raw_backend_error_or_retry() {
    let (_root, daemon, _wallet, mut backend) = setup().await;
    let job = tokio::spawn(async move {
        let (_request, reply) = backend.next().await;
        reply
            .send((
                json!({"code":"351805","message":"sensitive backend details"}),
                None,
            ))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), backend.requests.recv())
                .await
                .is_err()
        );
    });
    let result = super::super::overview::run(&daemon, true).await.unwrap();
    job.await.unwrap();
    assert_eq!(
        result["wallets"][0]["remote"]["readStatus"],
        "backend_read_failed"
    );
    assert_eq!(
        result["wallets"][0]["remote"]["connectionStatus"],
        "UNKNOWN"
    );
    assert!(!result.to_string().contains("sensitive"));
}
