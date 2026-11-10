use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy, PartialEq)]
enum Case {
    Historical,
    Live,
    Early,
    OutsidePage,
    Cancel,
    Expire,
    TooWide,
    Boundary,
}

async fn exercise(case: Case) {
    let definition: Definition = serde_json::from_value(json!({
        "id":"backfill-test", "chain_id":"56", "steps":[{
            "task_id":"watch", "trigger":{"kind":"immediate"},
            "valid_for_ms":10000, "completion":"submitted", "actions":[{
                "wallet":"w1", "argv":["contract-call","fire","--binanceChainId","56",
                    "--from","0x1111111111111111111111111111111111111111",
                    "--to","0x2222222222222222222222222222222222222222","--value","0"]
            }]
        }]
    }))
    .unwrap();
    let snapshot = validate(&definition).unwrap().snapshot();
    let run = Run {
        state: Mutex::new(Stored {
            definition,
            snapshot,
            status: "RUNNING".into(),
            results: BTreeMap::new(),
            recovery: None,
        }),
        cancel: Arc::new(AtomicBool::new(false)),
        active: AtomicBool::new(true),
    };
    let cancel = run.cancel.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let address = "0x2222222222222222222222222222222222222222";
    let topic = format!("0x{}", "cd".repeat(32));
    let server_topic = topic.clone();
    let deadline = now() + if case == Case::Expire { 4000 } else { 8000 };
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let hash = format!("0x{}", "ab".repeat(32));
        let log = |height| {
            json!({"address":address,"topics":[server_topic],
            "blockNumber":format!("0x{height:x}"),"blockHash":hash,"removed":false})
        };
        let head = match case {
            Case::TooWide => 510,
            Case::Boundary => 509,
            _ => 35,
        };
        let mut ranges = Vec::new();
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
                "eth_subscribe" => json!("event-sub"),
                "eth_blockNumber" => {
                    heads += 1;
                    json!(format!("0x{head:x}"))
                }
                "eth_getLogs" => {
                    let filter = &request["params"][0];
                    let from = quantity(&filter["fromBlock"]).unwrap();
                    let to = quantity(&filter["toBlock"]).unwrap();
                    assert!(
                        to >= from && to - from < 10,
                        "free-tier request exceeded 10 blocks"
                    );
                    assert_eq!(filter["address"], address);
                    assert_eq!(filter["topics"], json!([server_topic]));
                    ranges.push((from, to));
                    match case {
                        Case::Historical if to == 35 => json!([log(35)]),
                        Case::Early | Case::Boundary => json!([log(15)]),
                        Case::OutsidePage => json!([log(20)]),
                        Case::Cancel => {
                            cancel.store(true, Ordering::SeqCst);
                            json!([])
                        }
                        Case::Expire => {
                            tokio::time::sleep(Duration::from_millis(
                                deadline.saturating_sub(now()) + 25,
                            ))
                            .await;
                            json!([])
                        }
                        _ => json!([]),
                    }
                }
                "eth_getBlockByNumber" => json!({"number":request["params"][0],"hash":hash}),
                "eth_unsubscribe" => json!(true),
                _ => panic!("unexpected {method}"),
            };
            socket
                .send(Message::Text(
                    json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            if case == Case::Live && method == "eth_getLogs" && ranges.len() == 1 {
                socket
                    .send(Message::Text(
                        json!({"jsonrpc":"2.0","method":"eth_subscription",
                    "params":{"subscription":"event-sub","result":log(36)}})
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            if method == "eth_unsubscribe" {
                break;
            }
        }
        assert_eq!(heads, 1, "backfill must not poll the head");
        let expected = match case {
            Case::Historical | Case::Live => vec![(10, 19), (20, 29), (30, 35)],
            Case::TooWide => vec![],
            _ => vec![(10, 19)],
        };
        assert_eq!(ranges, expected);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "opened a second transport"
        );
    });
    let manager = Arc::new(crate::chain::rpc::SharedRpc::default());
    let result = tokio::time::timeout(
        Duration::from_secs(8),
        wait_trigger(
            &run,
            &Trigger::Event {
                address: address.into(),
                topics: vec![topic],
                from_block: 10,
            },
            deadline,
            &manager,
            Some(&url),
            "56",
        ),
    )
    .await
    .unwrap();
    match case {
        Case::OutsidePage => assert!(result.unwrap_err().contains("outside requested interval")),
        Case::Cancel => assert!(result.unwrap_err().contains("cancelled")),
        Case::Expire => assert!(result.unwrap_err().contains("expired")),
        Case::TooWide => assert!(result.unwrap_err().contains("500 blocks")),
        _ => {
            let observation = result.unwrap().step_json(None, None);
            assert_eq!(
                observation["triggerSource"],
                if matches!(case, Case::Live) {
                    "live_log"
                } else {
                    "backfill_log"
                }
            );
            assert!(observation["canonicalRead"]["total_ms"].is_number());
            if matches!(case, Case::Live) {
                assert!(observation["notificationQueueMs"].is_number());
            } else {
                assert!(
                    observation["notificationQueueMs"].is_null(),
                    "backfill cannot invent original event ingress"
                );
                assert!(observation["notificationToAcceptedMs"].is_null());
            }
        }
    }
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn later_backfill_pages_trigger_using_free_tier_ranges() {
    exercise(Case::Historical).await;
}
#[tokio::test]
async fn live_notification_during_backfill_is_not_dropped() {
    exercise(Case::Live).await;
}
#[tokio::test]
async fn first_matching_page_stops_further_backfill_reads() {
    exercise(Case::Early).await;
}
#[tokio::test]
async fn logs_outside_the_requested_page_are_rejected() {
    exercise(Case::OutsidePage).await;
}
#[tokio::test]
async fn cancellation_between_pages_stops_additional_reads() {
    exercise(Case::Cancel).await;
}
#[tokio::test]
async fn more_than_500_blocks_is_rejected_without_log_reads() {
    exercise(Case::TooWide).await;
}
#[tokio::test]
async fn exactly_500_blocks_is_admitted() {
    exercise(Case::Boundary).await;
}
#[tokio::test]
async fn expiry_between_pages_stops_additional_reads() {
    exercise(Case::Expire).await;
}
