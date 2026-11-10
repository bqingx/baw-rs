use super::*;
use crate::task::inspection::describe_definition;
use futures_util::{SinkExt, StreamExt};
use std::os::unix::fs::PermissionsExt;
use tokio_tungstenite::tungstenite::Message;
fn config() -> Definition {
    Definition {
        id: "p".into(),
        chain_id: "56".into(),
        rpc_url: None,
        steps: vec![Task {
            task_id: "t".into(),
            trigger: Trigger::Immediate,
            actions: vec![Action {
                wallet: "w1".into(),
                argv: [
                    "contract-call",
                    "fire",
                    "--binanceChainId",
                    "56",
                    "--from",
                    "0x1111111111111111111111111111111111111111",
                    "--to",
                    "0x2222222222222222222222222222222222222222",
                    "--value",
                    "0",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            }],
            completion: Completion::Submitted,
            valid_for_ms: 60000,
            confirmations: 1,
        }],
    }
}
#[tokio::test]
async fn block_and_log_triggers_use_subscription_notifications_not_repeated_head_polls() {
    for event in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let hash = format!("0x{}", "ab".repeat(32));
        let topic = format!("0x{}", "cd".repeat(32));
        let address = "0x2222222222222222222222222222222222222222";
        let log = json!({"address":address,"topics":[topic],"removed":false,"blockNumber":"0x11","blockHash":hash});
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut heads = 0;
            while let Some(frame) = socket.next().await {
                let frame = frame.unwrap();
                if !frame.is_text() {
                    continue;
                }
                let request: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
                let method = request["method"].as_str().unwrap();
                let result = match method {
                    "eth_chainId" => json!("0x38"),
                    "eth_subscribe" => json!("trigger-sub"),
                    "eth_blockNumber" => {
                        heads += 1;
                        json!("0x10")
                    }
                    "eth_getBlockByNumber" => json!({"number":"0x11","hash":hash}),
                    "eth_unsubscribe" => json!(true),
                    _ => panic!("unexpected trigger RPC {method}"),
                };
                socket
                    .send(Message::Text(
                        json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                if method == "eth_blockNumber" {
                    let value = if event {
                        log.clone()
                    } else {
                        json!({"number":"0x11","hash":hash})
                    };
                    socket.send(Message::Text(json!({"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"trigger-sub","result":value}}).to_string().into())).await.unwrap();
                }
                if method == "eth_unsubscribe" {
                    break;
                }
            }
            assert_eq!(heads, 1, "trigger polled instead of listening");
        });
        let c = config();
        let pipeline = validate(&c).unwrap();
        let run = Run {
            state: Mutex::new(Stored {
                definition: c,
                snapshot: pipeline.snapshot(),
                status: TaskStatus::Running,
                results: BTreeMap::new(),
                recovery: None,
            }),
            cancel: Arc::new(AtomicBool::new(false)),
            active: AtomicBool::new(true),
        };
        let trigger = if event {
            Trigger::Event {
                address: address.into(),
                topics: vec![format!("0x{}", "cd".repeat(32))],
                from_block: 17,
            }
        } else {
            Trigger::Block { number: 17 }
        };
        let manager = Arc::new(crate::chain::rpc::SharedRpc::default());
        let observation = tokio::time::timeout(
            Duration::from_secs(5),
            wait_trigger(&run, &trigger, now() + 5000, &manager, Some(&url), "56"),
        )
        .await
        .unwrap()
        .unwrap()
        .step_json(None, None);
        assert_eq!(
            observation["triggerSource"],
            if event { "live_log" } else { "live_head" }
        );
        assert!(observation["notificationQueueMs"].is_number());
        assert!(observation["notificationToAcceptedMs"].is_number());
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[test]
fn notifications_classify_evidence_without_copying_private_results() {
    let definition = config();
    let snapshot = validate(&definition).unwrap().snapshot();
    let mut state = Stored {
        definition,
        snapshot,
        status: TaskStatus::Paused,
        results: BTreeMap::from([(
            "runtime".into(),
            json!({"error":"secret https://private.example?apiKey=do-not-copy","calldata":"0xdeadbeef"}),
        )]),
        recovery: Some(Recovery {
            deadlines: BTreeMap::from([(0, 1)]),
            ..Recovery::default()
        }),
    };
    let expired = notification_event(&state).unwrap();
    assert_eq!(expired.kind, "expired");
    let encoded = serde_json::to_string(&expired).unwrap();
    for secret in ["secret", "private.example", "apiKey", "0xdeadbeef"] {
        assert!(!encoded.contains(secret));
    }
    state.snapshot.outcomes[0][0] = Outcome::Uncertain;
    assert_eq!(notification_event(&state).unwrap().kind, "unknown");
    state.results.insert(
        "0:w1".into(),
        json!({"execution":{"status":"PENDING_CONFIRMATION"}}),
    );
    assert_eq!(notification_event(&state).unwrap().kind, "awaiting_app");
    state.status = TaskStatus::Cancelled;
    assert_eq!(notification_event(&state).unwrap().kind, "cancelled");
    state.status = TaskStatus::Completed;
    state.snapshot.outcomes[0][0] = Outcome::Submitted;
    let completed = notification_event(&state).unwrap();
    assert_eq!(completed.kind, "completed");
    assert_eq!(completed.completion, "submitted");
    state.snapshot.outcomes[0][0] = Outcome::Confirmed;
    assert_eq!(notification_event(&state).unwrap().completion, "confirmed");
    state.status = TaskStatus::Running;
    assert!(notification_event(&state).is_none());
    state.status = TaskStatus::PersistenceError;
    assert!(notification_event(&state).is_none());
}

#[test]
fn mixed_wallet_notification_does_not_hide_unknown_or_failed_members_behind_app_confirmation() {
    let mut definition = config();
    let mut second = definition.steps[0].actions[0].clone();
    second.wallet = "w2".into();
    definition.steps[0].actions.push(second);
    let mut snapshot = validate(&definition).unwrap().snapshot();
    snapshot.outcomes[0] = vec![Outcome::Uncertain, Outcome::Uncertain];
    let mut state = Stored {
        definition,
        snapshot,
        status: TaskStatus::Paused,
        results: BTreeMap::from([
            ("0:w1".into(), json!({"status":"PENDING_CONFIRMATION"})),
            ("0:w2".into(), json!({"status":"UNCERTAIN"})),
        ]),
        recovery: None,
    };
    let event = notification_event(&state).unwrap();
    assert_eq!(event.kind, "mixed");
    assert_eq!(event.unresolved, 2);
    assert_eq!(event.awaiting_app, 1);
    state.snapshot.outcomes[0][1] = Outcome::Failed;
    assert_eq!(notification_event(&state).unwrap().kind, "mixed");
    state.snapshot.outcomes[0][1] = Outcome::Confirmed;
    assert_eq!(notification_event(&state).unwrap().kind, "awaiting_app");
}

#[tokio::test]
async fn describe_definition_does_not_connect_to_configured_rpc_or_grant_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut definition = config();
    definition.rpc_url = Some(format!("ws://{}", listener.local_addr().unwrap()));
    definition.steps[0].completion = Completion::Confirmed;
    let value = describe_definition(definition).unwrap();
    assert_eq!(value["rpcConfigured"], true);
    assert_eq!(value["executionAuthorized"], false);
    assert_eq!(value["deadlineGranted"], false);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn terminal_archives_remain_readable_without_reintroducing_retired_cli_parsing() {
    let root = tempfile::tempdir().unwrap();
    for status in ["COMPLETED", "CANCELLED", "LOADED"] {
        let mut definition = config();
        let mut pipeline = validate(&definition).unwrap();
        if status == "COMPLETED" {
            pipeline.claim(0, "w1").unwrap();
            pipeline.record(0, "w1", Outcome::Submitted).unwrap();
        }
        definition.steps[0].actions[0]
            .argv
            .push("--okx-allow-opaque-calldata".into());
        assert!(
            validate(&definition).is_err(),
            "retired syntax accepted as a new executable definition"
        );
        let state = Stored {
            definition,
            snapshot: pipeline.snapshot(),
            status: TaskStatus::parse(status),
            results: BTreeMap::new(),
            recovery: None,
        };
        save(root.path(), &state).unwrap();
        let archived = load(root.path(), "p");
        if status == "LOADED" {
            assert!(archived.is_err());
        } else {
            assert_eq!(archived.unwrap().status, TaskStatus::parse(status));
        }
    }
}

#[test]
fn rejects_unsafe_actions_and_mismatched_chains() {
    assert!(validate(&config()).is_ok());
    for args in [
        vec!["auth", "signout"],
        vec!["wallet", "send"],
        vec!["contract-call", "execute", "--requestId", "unreviewed"],
    ] {
        let mut c = config();
        c.steps[0].actions[0].argv = args.into_iter().map(String::from).collect();
        assert!(validate(&c).is_err());
    }
    let mut c = config();
    c.chain_id = "1".into();
    assert!(validate(&c).is_err());
    for flag in [
        "--skip-prechecks",
        "--confirm",
        "--operation-id=override",
        "--wallet=w2",
        "--instance-id=override",
    ] {
        let mut c = config();
        c.steps[0].actions[0].argv.push(flag.into());
        assert!(validate(&c).is_err(), "accepted {flag}");
    }
    let mut c = config();
    c.steps[0].completion = Completion::Confirmed;
    assert!(validate(&c).is_err());
}
#[test]
fn operation_ids_fit_the_journal_and_are_bound_to_each_member() {
    let a = operation_id("p", 0, "w1");
    assert!(crate::exec::execution::require_consent(true, Some(&a)).is_ok());
    assert_ne!(a, operation_id("p", 0, "w2"));
    assert_ne!(a, operation_id("p", 1, "w1"));
    assert_ne!(a, operation_id("q", 0, "w1"));
}
#[test]
fn app_confirmation_is_not_a_submitted_transaction() {
    assert!(!submitted(
        &json!({"execution":{"status":"PENDING_CONFIRMATION","orderId":"o"}})
    ));
    assert!(!submitted(&json!({"status":"UNCERTAIN","orderId":"o"})));
    for value in [
        json!({"status":"BROADCASTED","orderId":"o"}),
        json!({"execution":{"status":"BROADCASTED","orderId":"o"}}),
        json!({"execution":{"execution":{"status":"BROADCASTED","orderId":"o","txHash":"0x1234"}}}),
    ] {
        assert!(
            !submitted(&value),
            "order-only or invalid-hash response cannot advance: {value}"
        );
    }
    assert!(submitted(
        &json!({"execution":{"status":"BROADCASTED","txHash":format!("0x{}","ab".repeat(32))}})
    ));
}
#[test]
fn persisted_running_state_is_interrupted_and_cannot_be_replayed() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = config();
    let mut pipeline = validate(&config).unwrap();
    pipeline.claim(0, "w1").unwrap();
    save(
        root.path(),
        &Stored {
            definition: config,
            snapshot: pipeline.snapshot(),
            status: TaskStatus::Running,
            results: BTreeMap::new(),
            recovery: None,
        },
    )
    .unwrap();
    let loaded = load(root.path(), "p").unwrap();
    assert_eq!(loaded.status, TaskStatus::Interrupted);
    assert_eq!(loaded.snapshot.outcomes[0][0], Outcome::Uncertain);
}
#[tokio::test]
async fn immediate_time_expiry_and_cancel_are_local_only() {
    let c = config();
    let pipeline = validate(&c).unwrap();
    let run = Run {
        state: Mutex::new(Stored {
            definition: c,
            snapshot: pipeline.snapshot(),
            status: TaskStatus::Running,
            results: BTreeMap::new(),
            recovery: None,
        }),
        cancel: Arc::new(AtomicBool::new(false)),
        active: AtomicBool::new(true),
    };
    let rpc = Arc::new(crate::chain::rpc::SharedRpc::default());
    wait_trigger(&run, &Trigger::Immediate, now() + 1000, &rpc, None, "56")
        .await
        .unwrap();
    wait_trigger(
        &run,
        &Trigger::Time {
            unix_ms: now() + 10,
        },
        now() + 1000,
        &rpc,
        None,
        "56",
    )
    .await
    .unwrap();
    assert!(wait_trigger(
        &run,
        &Trigger::Time { unix_ms: u64::MAX },
        1,
        &rpc,
        None,
        "56"
    )
    .await
    .is_err());
    run.cancel.store(true, Ordering::SeqCst);
    assert!(
        wait_trigger(&run, &Trigger::Immediate, now() + 1000, &rpc, None, "56")
            .await
            .is_err()
    );
}
