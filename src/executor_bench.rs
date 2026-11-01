//! Explicitly opt-in, loopback-only comparison with the real CLI subprocess.
use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "loopback benchmark; requires BAW_RS_BENCH_BINARY and explicit invocation"]
async fn persistent_executor_loopback_benchmark() {
    let binary = std::env::var("BAW_RS_BENCH_BINARY").expect("set scratch-built binary path");
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut mock = Mock::new().await;
    let token = format!("{:032x}", rand::random::<u128>());
    std::fs::write(
        root.path().join("loopback-benchmark.json"),
        json!({"token":token,"origin":mock.origin}).to_string(),
    )
    .unwrap();
    let a = wallet(
        root.path(),
        "a",
        "instance-a",
        A,
        Some("cookie-a"),
        &mock.origin,
    );
    let path = root.path().join("daemon.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = tokio::spawn(serve_listener(
        listener,
        Arc::new(BTreeMap::from([("alice".into(), a)])),
    ));
    let backend = tokio::spawn(async move {
        let mut previews = 0u64;
        while let Some((request, reply)) = mock.requests.recv().await {
            let value = if request.contains("settings/query") {
                json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
            } else if request.contains("mpc-wallet/list") {
                json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":super::executor_tests::SENDER}]}]}])
            } else if request.contains("networks/active") {
                json!([])
            } else if request.contains("contract-call/preview") {
                previews += 1;
                super::executor_tests::valid_preview(&format!("bench-{previews}"))
            } else if request.contains("contract-call/execute") {
                json!({"orderId":"synthetic-order"})
            } else {
                panic!("unexpected HTTP request")
            };
            let _ = reply.send((value, None));
        }
    });
    let registry = root.path().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("python3")
            .arg("examples/benchmark_persistent.py")
            .arg("--binary")
            .arg(binary)
            .arg("--registry")
            .arg(registry)
            .arg("--samples")
            .arg("100")
            .env("BAW_RS_LOOPBACK_FIXTURE", token)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    send(&path, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    backend.abort();
}
