use super::*;
use crate::core::{client::Client, session};
use crate::task::prepare as preparation;
use crate::task::runtime::{validate, Definition, Recovery};
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeMap;
use tokio_tungstenite::tungstenite::Message;
fn state() -> Stored {
    let config:Definition=serde_json::from_value(json!({"id":"p","chain_id":"56","steps":[{"task_id":"t","trigger":{"kind":"immediate"},"completion":"submitted","valid_for_ms":60000,"actions":[{"wallet":"w1","argv":["contract-call","fire","--binanceChainId","56","--from","0x1111111111111111111111111111111111111111","--to","0x2222222222222222222222222222222222222222"]}]}]})).unwrap();
    let snapshot = validate(&config).unwrap().snapshot();
    Stored {
        definition: config,
        snapshot,
        status: TaskStatus::Reconciled,
        results: BTreeMap::new(),
        recovery: Some(Recovery {
            deadlines: BTreeMap::from([(0, now() + 60000)]),
            triggered: std::collections::BTreeSet::from([0]),
            bindings: BTreeMap::new(),
            ..Recovery::default()
        }),
    }
}
#[test]
fn receipt_sender_uses_the_bound_record_even_after_mode_expiry() {
    let mut s = state();
    let client = Client::test_client(
        session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-policy".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        },
        "http://127.0.0.1:9",
    )
    .unwrap();
    let mut policy = crate::exec::execution::PreparedWalletPolicy::capture(
        &client,
        "56",
        "0x1111111111111111111111111111111111111111",
        4102444800,
    )
    .unwrap();
    policy.developer_mode_expires_at = 1;
    s.recovery.as_mut().unwrap().prepared.insert(
        "0:w1".into(),
        json!({"status":"ALLOWANCE_READY",
        "preparedBeforeTrigger":true,"walletPolicy":policy}),
    );
    assert_eq!(
        preparation::receipt_sender(&s, 0, "w1", "56").unwrap(),
        Some("0x1111111111111111111111111111111111111111".into())
    );
    assert!(preparation::receipt_sender(&s, 0, "w1", "1").is_err());
}
#[test]
fn malformed_new_receipt_policy_is_not_treated_as_a_legacy_lookup() {
    let mut s = state();
    s.recovery
        .as_mut()
        .unwrap()
        .prepared
        .insert("0:w1".into(), json!({"status":"ALLOWANCE_READY"}));
    assert!(preparation::receipt_sender(&s, 0, "w1", "56")
        .unwrap()
        .is_none());
    s.recovery
        .as_mut()
        .unwrap()
        .prepared
        .get_mut("0:w1")
        .unwrap()["walletPolicy"] = Value::Null;
    assert!(preparation::receipt_sender(&s, 0, "w1", "56").is_err());
}

#[test]
fn continuation_never_resets_deadlines_or_replays_unknown_failed_or_cancelled_work() {
    assert!(resumable(&state()).is_ok());
    let mut s = state();
    s.recovery.as_mut().unwrap().deadlines.insert(0, 1);
    assert!(resumable(&s).is_err());
    let mut s = state();
    s.recovery.as_mut().unwrap().deadlines.clear();
    assert!(
        resumable(&s).is_err(),
        "missing deadline must not become a fresh budget"
    );
    let mut s = state();
    s.recovery = None;
    assert!(resumable(&s).is_err());
    let mut s = state();
    s.status = TaskStatus::Cancelled;
    assert!(resumable(&s).is_err());
    for outcome in [Outcome::Uncertain, Outcome::Failed, Outcome::InFlight] {
        let mut s = state();
        s.snapshot.outcomes[0][0] = outcome;
        assert!(resumable(&s).is_err(), "accepted {outcome:?}");
    }
}
#[test]
fn started_preparation_without_ready_proof_is_rejected_before_resume_launch() {
    let mut state = state();
    state
        .recovery
        .as_mut()
        .unwrap()
        .preparation_started
        .insert("0:w1".into());
    let deadlines = state.recovery.as_ref().unwrap().deadlines.clone();
    assert!(resumable(&state).unwrap_err().contains("preparation"));
    assert_eq!(state.recovery.as_ref().unwrap().deadlines, deadlines);
}

#[tokio::test]
async fn reconciliation_only_accepts_bound_canonical_sufficiently_confirmed_receipts() {
    let sender = "0x1111111111111111111111111111111111111111";
    let hash = format!("0x{}", "aa".repeat(32));
    let block_hash = format!("0x{}", "bb".repeat(32));
    for case in 0..7 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let expected_hash = hash.clone();
        let canonical_hash = block_hash.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(frame)) = socket.next().await {
                if !frame.is_text() {
                    continue;
                }
                let request: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
                let mut receipt = json!({"transactionHash":expected_hash,"from":sender,"status":"0x1","blockNumber":"0x10","blockHash":canonical_hash});
                if case == 1 {
                    receipt["transactionHash"] = json!(format!("0x{}", "dd".repeat(32)));
                }
                if case == 2 {
                    receipt["from"] = json!("0x2222222222222222222222222222222222222222");
                }
                if case == 3 {
                    receipt["status"] = json!("0x0");
                }
                let result = match request["method"].as_str().unwrap() {
                    "eth_getTransactionReceipt" => {
                        if case == 0 {
                            Value::Null
                        } else {
                            receipt
                        }
                    }
                    "eth_blockNumber" => json!("0x10"),
                    "eth_getBlockByNumber" => {
                        json!({"number":"0x10","hash":if case==4 {format!("0x{}","ee".repeat(32))} else {canonical_hash.clone()}})
                    }
                    other => panic!("unexpected read-only RPC {other}"),
                };
                if socket
                    .send(Message::Text(
                        json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let manager = Arc::new(crate::chain::rpc::SharedRpc::default());
        let rpc = manager.acquire(&url).unwrap();
        let result = receipt_status(&rpc, &hash, sender, if case == 5 { 2 } else { 1 }).await;
        match case {
            0 | 5 => assert_eq!(result.unwrap(), None),
            3 => assert_eq!(result.unwrap(), Some(false)),
            6 => assert_eq!(result.unwrap(), Some(true)),
            _ => assert!(result.is_err()),
        }
        drop(rpc);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }
}
