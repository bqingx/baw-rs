use super::super::task_runtime::*;
use super::fixtures;
use super::*;

fn definition() -> Definition {
    Definition {
        id: "pipeline-demo".into(),
        chain_id: "56".into(),
        rpc_url: None,
        steps: [
            ("task1", ["w1", "w2"]),
            ("task2", ["w2", "w3"]),
            ("task3", ["w3", "w4"]),
        ]
        .into_iter()
        .map(|(id, names)| Task {
            task_id: id.into(),
            trigger: Trigger::Immediate,
            actions: names
                .into_iter()
                .map(|name| Action {
                    wallet: name.into(),
                    argv: vec![
                        "contract-call",
                        "fire",
                        "--binanceChainId",
                        "56",
                        "--from",
                        fixtures::SENDER,
                        "--to",
                        "0x2222222222222222222222222222222222222222",
                        "--value",
                        "0",
                        "--inputData",
                        "0x12345678",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                })
                .collect(),
            completion: crate::pipeline::Completion::Submitted,
            valid_for_ms: 60000,
            confirmations: 1,
        })
        .collect(),
    }
}
#[tokio::test]
async fn instance_load_persists_and_refuses_overwrite_or_unconfirmed_arm() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let entries = ["w1", "w2", "w3", "w4"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                wallet(
                    root.path(),
                    name,
                    name,
                    A,
                    Some("synthetic"),
                    "http://127.0.0.1:9",
                ),
            )
        })
        .collect();
    let daemon = Arc::new(Daemon {
        wallets: Arc::new(RwLock::new(entries)),
        registry: Some(root.path().into()),
        scheduler: Arc::new(Scheduler::default()),
    });
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
    let file = root.path().join("pipeline-pipeline-demo.json");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Load {
                config: definition()
            }
        )
        .await
        .is_err());
    assert!(daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "pipeline-demo".into(),
                confirm: false
            }
        )
        .await
        .is_err());
    let restored = Arc::new(Scheduler::default());
    let status = restored
        .control(
            daemon.clone(),
            Control::Status {
                id: "pipeline-demo".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(status["status"], "LOADED");
    assert_eq!(
        status["snapshot"]["steps"][1]["wallets"],
        json!(["w2", "w3"])
    );
    restored
        .control(
            daemon.clone(),
            Control::Cancel {
                id: "pipeline-demo".into(),
            },
        )
        .await
        .unwrap();
    assert!(restored
        .control(
            daemon.clone(),
            Control::Arm {
                id: "pipeline-demo".into(),
                confirm: true
            }
        )
        .await
        .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_pipeline_runs_overlapping_wallet_sets_in_order_and_drains() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut mock = Mock::new().await;
    let entries = ["w1", "w2", "w3", "w4"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                wallet(root.path(), name, name, A, Some("synthetic"), &mock.origin),
            )
        })
        .collect();
    let daemon = Arc::new(Daemon {
        wallets: Arc::new(RwLock::new(entries)),
        registry: Some(root.path().into()),
        scheduler: Arc::new(Scheduler::default()),
    });
    let mut config = definition();
    for step in &mut config.steps {
        step.valid_for_ms = 120000;
    }
    daemon
        .scheduler
        .control(daemon.clone(), Control::Load { config })
        .await
        .unwrap();
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "pipeline-demo".into(),
                confirm: true,
            },
        )
        .await
        .unwrap();
    let mut executed = 0;
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let status = daemon.scheduler.control(daemon.clone(), Control::Status { id: "pipeline-demo".into() }).await.unwrap();
            if status["status"] != "RUNNING" {
                assert_eq!(status["status"], "COMPLETED", "{status}"); break;
            }
            tokio::select! {
                row = mock.requests.recv() => {
                    let (request, reply) = row.unwrap();
                    let body = if request.contains("settings/query") {
                        json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})
                    } else if request.contains("/mpc-wallet/list") {
                        json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":fixtures::SENDER}]}]}])
                    } else if request.contains("/networks/active") { json!([])
                    } else if request.contains("/contract-call/preview") {
                        fixtures::valid_preview(&format!("preview-{}", rand::random::<u64>()))
                    } else if request.contains("/contract-call/execute") {
                        executed += 1; json!({"orderId":format!("synthetic-order-{executed}"),"status":"BROADCASTED","txHash":format!("0x{executed:064x}")})
                    } else { panic!("unexpected mock request: {request}") };
                    reply.send((body, None)).unwrap();
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
    }).await.unwrap();
    assert_eq!(executed, 6);
    daemon.scheduler.stop().await;
    assert!(daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "pipeline-demo".into(),
                confirm: true
            }
        )
        .await
        .is_err());
}

#[tokio::test]
async fn task_gate_blocks_expired_and_cancelled_requests() {
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    assert!(crate::execution::task_scope(1, cancel.clone(), async {
        crate::execution::check_task_gate()
    })
    .await
    .is_err());
    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(crate::execution::task_scope(u64::MAX, cancel, async {
        crate::execution::check_task_gate()
    })
    .await
    .is_err());
    assert!(crate::execution::check_task_gate().is_ok());
}
