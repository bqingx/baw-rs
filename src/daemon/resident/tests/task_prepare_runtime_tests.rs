//! End-to-end task preparation uses synthetic sessions and loopback HTTP/WS only.
use super::*;
use crate::task::runtime::{Control, Definition, Scheduler};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

const INPUT: &str = "0x2222222222222222222222222222222222222222";
const OUTPUT: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const ROUTER: &str = "0x3333333333333333333333333333333333333333";
const SPENDER: &str = "0x4444444444444444444444444444444444444444";
const AMOUNT: &str = "1000000000000000000";
fn private(path: &std::path::Path, value: Value) {
    std::fs::write(path, value.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn quote() -> Value {
    json!({"chainIndex":"56","swapMode":"exactIn","fromTokenAmount":AMOUNT,
        "toTokenAmount":"2000000000000000","priceImpactPercent":"0.1",
        "fromToken":{"tokenContractAddress":INPUT,"decimal":"18","isHoneyPot":false,"taxRate":"0","tokenSymbol":"IN"},
        "toToken":{"tokenContractAddress":OUTPUT,"decimal":"18","isHoneyPot":false,"taxRate":"0","tokenSymbol":"BNB"}})
}
fn preview(approve: bool) -> Value {
    json!({"requestId":if approve {"prep-preview"} else {"swap-preview"},"requireConfirmation":false,
        "risks":{"addresses":{},"riskDetails":[],"riskBehaviors":[]},
        "parsedTx":{"contractAddress":if approve {INPUT} else {ROUTER}},
        "simulationResult":{"simulationCode":"000000000","authorityChanges":[],
        "allowanceChanges":[{"owner":fixtures::SENDER,"tokenAddress":INPUT,"spender":SPENDER,
            "preAmount":if approve {"0"} else {AMOUNT},"postAmount":if approve {AMOUNT} else {"0"},"tokenType":"Erc20","tokenId":null}],
        "balanceChanges":if approve {json!([])} else {json!([
            {"owner":fixtures::SENDER,"contractAddress":INPUT,"tokenType":"Erc20","change":format!("-{AMOUNT}")},
            {"owner":fixtures::SENDER,"contractAddress":OUTPUT,"tokenType":"Native","change":"2000000000000000"}])}}})
}

#[derive(Clone, Copy, PartialEq)]
enum Case {
    Success,
    UncertainPreparation,
    Confirmed,
    RejectedPreview,
    PendingExecute,
    UnknownExecute,
    DroppedExecuteReply,
    ExecuteTimeout,
    HashlessExecute,
    DroppedApprovalReply,
    NoFixedFloor,
}
async fn run_test(case: Case) {
    run_test_with_notification(case, false).await;
}
async fn run_test_with_notification(case: Case, notify: bool) {
    let ambiguous = matches!(
        case,
        Case::UncertainPreparation | Case::DroppedApprovalReply
    );
    let confirmed = case == Case::Confirmed;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let (notifications, notification_job) = if notify {
        let mut server = Mock::new().await;
        let path = root.path().join("notify.json");
        private(
            &path,
            json!({"token":"synthetic-notify-token","channel":"app"}),
        );
        let notifier =
            crate::task::notifications::Notifier::for_test(&path, &server.origin).unwrap();
        let job = tokio::spawn(async move {
            let (request, reply) = server.next().await;
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["channel"], "app");
            let content = body["content"].as_str().unwrap();
            assert!(content.contains("prepared-demo"));
            match case {
                Case::Success => assert!(content.contains("提交阶段完成（不代表链上成功）")),
                Case::PendingExecute => assert!(content.contains("等待 App 确认")),
                Case::HashlessExecute => assert!(content.contains("结果未知")),
                _ => panic!("unconfigured notification case"),
            }
            for secret in [
                "synthetic-key",
                "synthetic-secret",
                "synthetic-pass",
                "swap-preview",
                "0x",
            ] {
                assert!(!content.contains(secret));
            }
            // Notification provider failure must not change task completion or retry it.
            reply
                .send((
                    json!({"code":500,"msg":"private-notification-provider-error"}),
                    None,
                ))
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), server.requests.recv())
                    .await
                    .is_err()
            );
        });
        (notifier, Some(job))
    } else {
        (crate::task::notifications::Notifier::default(), None)
    };
    let mut backend = Mock::new().await;
    let mut provider = Mock::new().await;
    let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let shared_events = events.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_addr = listener.local_addr().unwrap();
    let ws = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let hash = format!("0x{}", "ab".repeat(32));
        let approval_hash = format!("0x{}", "cd".repeat(32));
        let mut allowance_calls = 0;
        let mut listening = false;
        while let Some(frame) = socket.next().await {
            let Ok(frame) = frame else { break };
            if !frame.is_text() {
                continue;
            }
            let request: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "eth_chainId" => json!("0x38"),
                "eth_call" => {
                    assert!(!listening, "allowance read leaked onto trigger path");
                    allowance_calls += 1;
                    assert_eq!(allowance_calls, 1, "allowance was reread after approval");
                    json!(format!("0x{}", "00".repeat(32)))
                }
                "eth_getTransactionReceipt" => {
                    assert!(!ambiguous);
                    if request["params"][0] == approval_hash {
                        assert!(!listening);
                        shared_events
                            .lock()
                            .unwrap()
                            .push("approval_receipt".into());
                        json!({"transactionHash":approval_hash,"from":fixtures::SENDER,"to":INPUT,
                            "blockNumber":"0x64","blockHash":hash,"status":"0x1"})
                    } else {
                        assert!(confirmed && listening);
                        assert_eq!(request["params"][0], format!("0x{}", "ef".repeat(32)));
                        json!({"transactionHash":request["params"][0],"from":fixtures::SENDER,"to":ROUTER,
                            "blockNumber":"0x66","blockHash":hash,"status":"0x1"})
                    }
                }
                "eth_subscribe" => {
                    assert!(shared_events
                        .lock()
                        .unwrap()
                        .contains(&"approval_receipt".into()));
                    listening = true;
                    shared_events.lock().unwrap().push("listening".into());
                    json!("event-sub")
                }
                "eth_blockNumber" => {
                    assert!(listening, "extra confirmation-depth read before listening");
                    json!(if shared_events
                        .lock()
                        .unwrap()
                        .contains(&"swap_execute".into())
                    {
                        "0x67"
                    } else {
                        "0x64"
                    })
                }
                "eth_getBlockByNumber" => {
                    assert!(listening, "canonical reread before listening");
                    json!({"number":request["params"][0],"hash":hash})
                }
                "eth_unsubscribe" => json!(true),
                _ => panic!("unexpected RPC {method}"),
            };
            socket
                .send(Message::Text(
                    json!({"jsonrpc":"2.0","id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            if method == "eth_subscribe" {
                socket.send(Message::Text(json!({"jsonrpc":"2.0","method":"eth_subscription","params":{
                    "subscription":"event-sub","result":{"address":INPUT,"topics":[hash],"removed":false,
                    "blockNumber":"0x65","blockHash":hash}}}).to_string().into())).await.unwrap();
            }
        }
        assert_eq!(allowance_calls, 1);
        assert_eq!(listening, !ambiguous);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let credentials = root.path().join("credentials.json");
    private(
        &credentials,
        json!({"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass"}),
    );
    let profile = root.path().join("profile.json");
    private(
        &profile,
        json!({"credential_file":credentials,"chain_index":"56",
        "expected_spender":SPENDER,"allowed_router_addresses":[ROUTER],
        "test_origin":provider.origin,"rpc_url":format!("http://{rpc_addr}"),"approval_confirmations":20}),
    );
    let mut task:Definition=serde_json::from_value(json!({"id":"prepared-demo","chain_id":"56","rpc_url":format!("ws://{rpc_addr}"),"steps":[{
        "task_id":"watch","trigger":{"kind":"event","address":INPUT,"topics":[format!("0x{}","ab".repeat(32))],"from_block":101},
        "valid_for_ms":60000,"completion":"submitted","actions":[{"wallet":"w1","argv":["market-order","swap",
            "--okx-config",profile,"--binanceChainId","56","--fromToken",INPUT,"--toToken",OUTPUT,
            "--fromTokenQty","1","--fromTokenDecimals","18","--slippage","1","--okx-min-output-units","1",
            "--mev","false","--acknowledge-token-risk","--auto-approve"]}]}]})).unwrap();
    if case == Case::NoFixedFloor {
        let argv = &mut task.steps[0].actions[0].argv;
        let index = argv
            .iter()
            .position(|s| s == "--okx-min-output-units")
            .unwrap();
        argv.drain(index..index + 2);
    }
    if case == Case::HashlessExecute {
        let mut successor = task.steps[0].clone();
        successor.task_id = "must-not-start".into();
        successor.trigger = crate::task::runtime::Trigger::Immediate;
        task.steps.push(successor);
    }
    if confirmed {
        task.steps[0].completion = crate::task::pipeline::Completion::Confirmed;
        task.steps[0].confirmations = 2;
    }
    let entry = wallet(
        root.path(),
        "w1",
        "w1",
        A,
        Some("synthetic"),
        &backend.origin,
    );
    let daemon = Arc::new(Daemon {
        wallets: Arc::new(RwLock::new(BTreeMap::from([("w1".into(), entry)]))),
        registry: root.path().into(),
        scheduler: Arc::new(Scheduler::with_notifications(notifications)),
    });
    daemon
        .scheduler
        .control(daemon.clone(), Control::Load { config: task })
        .await
        .unwrap();
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Arm {
                id: "prepared-demo".into(),
                confirm: true,
            },
        )
        .await
        .unwrap();
    let mut route_requests = 0;
    let mut hot_previews = 0;
    let mut hot_executes = 0;
    let mut held_execute_replies = Vec::new();
    let status=tokio::time::timeout(Duration::from_secs(60),async {
        loop {
            let status=daemon.scheduler.control(daemon.clone(),Control::Status{id:"prepared-demo".into()}).await.unwrap();
            if status["status"]!="RUNNING" {break status;}
            tokio::select! {
                row=backend.requests.recv()=>{
                    let (request,reply)=row.unwrap();let line=request.lines().next().unwrap();
                    if events.lock().unwrap().contains(&"listening".into()) {
                        assert!(line.contains("contract-call/preview") || line.contains("contract-call/execute"),
                            "auxiliary backend request leaked onto the trigger path: {line}");
                    }
                    let value=if line.contains("settings/query") {json!({"dailyLimit":"100","devMode":{"enabled":true,"expiresAt":4102444800u64}})
                    }else if line.contains("trading-limit/query"){json!({"quotaUsed":"0"})
                    }else if line.contains("tx-lock-status"){json!({"status":"UNLOCKED"})
                    }else if line.contains("mpc-wallet/list"){json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":fixtures::SENDER}]}]}])
                    }else if line.contains("networks/active"){json!([])
                    }else if line.contains("contract-call/preview"){
                        let body:Value=serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                        let approve=body["to"]==INPUT;
                        if approve {assert!(!events.lock().unwrap().contains(&"listening".into()));}
                        else {hot_previews+=1;}
                        if !approve && case==Case::RejectedPreview {json!({"code":"351805","message":"state changed"})}
                        else {let mut value=preview(approve);if !approve && case==Case::NoFixedFloor {value["simulationResult"]["balanceChanges"][1]["change"]=json!("1600000000000000");}value}
                    }else if line.contains("contract-call/execute"){
                        let body:Value=serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                        if body["requestId"]=="prep-preview" {
                            assert!(!events.lock().unwrap().contains(&"listening".into()));
                            events.lock().unwrap().push("approval_execute".into());
                            if case==Case::DroppedApprovalReply {drop(reply);continue;}
                            if ambiguous {json!({"status":"PENDING_CONFIRMATION","orderId":"approval-order"})}
                            else {json!({"status":"BROADCASTED","txHash":format!("0x{}","cd".repeat(32))})}
                        }else{
                            assert!(events.lock().unwrap().contains(&"listening".into()));
                            events.lock().unwrap().push("swap_execute".into());
                            hot_executes+=1;
                            if case==Case::DroppedExecuteReply {drop(reply);continue;}
                            if case==Case::ExecuteTimeout {held_execute_replies.push(reply);continue;}
                            match case {
                                Case::PendingExecute=>json!({"status":"PENDING_CONFIRMATION","orderId":"swap-pending"}),
                                Case::UnknownExecute=>json!({"code":"351805","message":"execute unknown"}),
                                Case::HashlessExecute=>json!({"status":"BROADCASTED","orderId":"hash-not-yet-known"}),
                                _=>json!({"status":"BROADCASTED","txHash":format!("0x{}","ef".repeat(32))}),
                            }
                        }
                    }else{panic!("unexpected backend {line}")};
                    if confirmed && value["txHash"]==format!("0x{}","ef".repeat(32)) {
                        // Expire legacy address caches: confirmation must not fall back
                        // to a hidden wallet-list/network read after submission.
                        tokio::time::sleep(Duration::from_millis(3100)).await;
                    }
                    reply.send((value,None)).unwrap();
                }
                row=provider.requests.recv()=>{
                    let (request,reply)=row.unwrap();let line=request.lines().next().unwrap();
                    let value=if line.contains("/approve-transaction?"){
                        assert!(!events.lock().unwrap().contains(&"listening".into()));
                        json!({"dexContractAddress":SPENDER})
                    }else if line.contains("/quote?"){
                        assert!(!events.lock().unwrap().contains(&"listening".into()));quote()
                    }else if line.contains("/swap?"){
                        route_requests+=1;
                        assert!(events.lock().unwrap().contains(&"listening".into()));
                        let admitted:Value=serde_json::from_slice(&std::fs::read(root.path().join("pipeline-prepared-demo.json")).unwrap()).unwrap();
                        assert_eq!(admitted["recovery"]["triggered"],json!([0]));
                        assert_eq!(admitted["snapshot"]["outcomes"],if case==Case::HashlessExecute {json!([["in_flight"],["pending"]])} else {json!([["in_flight"]])});
                        assert_eq!(admitted["results"]["triggerTiming:0"]["triggerSource"],"live_log");
                        assert!(admitted["results"]["triggerTiming:0"]["admissionSaveMs"].is_null(),
                            "the admission save cannot contain its own completion duration");
                        let mut route=json!({"routerResult":quote(),"tx":{"from":fixtures::SENDER,"to":ROUTER,"value":"0",
                            "gas":"100000","data":"0x12345678","minReceiveAmount":"1980000000000000","chainId":"56"}});
                        if case==Case::NoFixedFloor {route["routerResult"]["toTokenAmount"]=json!("1600000000000000");route["tx"]["minReceiveAmount"]=json!("1584000000000000");}route
                    }else{panic!("unexpected provider {line}")};
                    reply.send((json!({"code":"0","data":[value]}),None)).unwrap();
                }
                _=tokio::time::sleep(Duration::from_millis(10))=>{},
            }
        }
    }).await.unwrap();
    if ambiguous {
        assert_eq!(status["status"], "PAUSED", "{status}");
        assert_eq!(status["snapshot"]["outcomes"], json!([["pending"]]));
        assert_eq!(*events.lock().unwrap(), vec!["approval_execute"]);
    } else if matches!(
        case,
        Case::RejectedPreview
            | Case::PendingExecute
            | Case::UnknownExecute
            | Case::DroppedExecuteReply
            | Case::ExecuteTimeout
            | Case::HashlessExecute
    ) {
        assert_eq!(status["status"], "PAUSED", "{status}");
        assert_eq!(
            status["snapshot"]["outcomes"],
            if case == Case::HashlessExecute {
                json!([["uncertain"], ["pending"]])
            } else {
                json!([["uncertain"]])
            }
        );
        if case == Case::HashlessExecute {
            assert!(status["results"].get("prepare:1:w1").is_none());
            assert!(status["results"].get("1:w1").is_none());
        }
        if case == Case::PendingExecute {
            assert_eq!(status["results"]["0:w1"]["status"], "PENDING_CONFIRMATION");
        }
        if matches!(
            case,
            Case::UnknownExecute
                | Case::DroppedExecuteReply
                | Case::ExecuteTimeout
                | Case::HashlessExecute
        ) {
            assert_eq!(status["results"]["0:w1"]["status"], "UNCERTAIN");
        }
    } else {
        assert_eq!(status["status"], "COMPLETED", "{status}");
        assert_eq!(
            status["snapshot"]["outcomes"],
            json!([[if confirmed { "confirmed" } else { "submitted" }]])
        );
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                "approval_execute",
                "approval_receipt",
                "listening",
                "swap_execute"
            ]
        );
        assert_eq!(
            status["results"]["0:w1"]["allowancePreparedBeforeTrigger"],
            true
        );
        assert!(status["results"]["0:w1"]["walletExecuteMs"]
            .as_f64()
            .unwrap()
            .is_finite());
        for key in [
            "routeFetchMs",
            "routeValidationMs",
            "previewCallMs",
            "executeCallMs",
            "beforeExecuteCallMs",
            "buildThroughResponseMs",
        ] {
            assert!(status["results"]["0:w1"]["hotPathTiming"][key]
                .as_f64()
                .unwrap()
                .is_finite());
        }
        let reads = &status["results"]["0:w1"]["stateReadStats"];
        assert_eq!(reads["binanceRequests"], 2);
        assert_eq!(reads["policyReads"], 0);
        assert_eq!(reads["cacheHits"], 0);
        let transports = &status["results"]["0:w1"]["hotPathTiming"]["httpTransport"];
        let preview_connection = &transports["binancePreview"]["connection"];
        let execute_connection = &transports["binanceExecute"]["connection"];
        assert!(preview_connection["pool_id"].is_u64());
        assert_eq!(preview_connection["pool_id"], execute_connection["pool_id"]);
        assert_ne!(
            transports["okxSwap"]["pool_id"],
            preview_connection["pool_id"]
        );
        assert!(transports["binancePreview"]["request_ms"].is_number());
        assert!(transports["binanceExecute"]["body_ms"].is_number());
        assert_eq!(status["results"]["0:w1"]["preparedWalletPolicyUsed"], true);
        let stored: Value = serde_json::from_slice(
            &std::fs::read(root.path().join("pipeline-prepared-demo.json")).unwrap(),
        )
        .unwrap();
        assert!(
            stored["recovery"].get("plans").is_none(),
            "ephemeral compiled plans leaked into durable state"
        );
        assert!(stored["recovery"]["preparation_deadlines"]["0"]
            .as_u64()
            .is_some());
        assert!(
            stored["recovery"]["deadlines"]["0"].as_u64().unwrap() - 60_000
                > stored["recovery"]["preparation_deadlines"]["0"]
                    .as_u64()
                    .unwrap()
                    - 120_000,
            "preparation must not consume the listening budget"
        );
    }
    if !ambiguous {
        let timing = &status["results"]["0:w1"]["triggerTiming"];
        assert_eq!(timing["triggerSource"], "live_log");
        for key in [
            "notificationQueueMs",
            "notificationToAcceptedMs",
            "candidateValidationMs",
            "admissionSaveMs",
            "acceptedToAdmissionEndMs",
            "workerDispatchMs",
            "walletQueueWaitMs",
            "walletPrepareMs",
            "acceptedToDispatchMs",
            "notificationToDispatchMs",
            "notificationToActionReturnMs",
            "workerResultCollectMs",
            "resultSaveMs",
        ] {
            assert!(
                timing[key]
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v >= 0.),
                "missing/invalid {key}: {timing}"
            );
        }
        assert!(
            timing["acceptedToAdmissionEndMs"].as_f64().unwrap()
                <= timing["acceptedToDispatchMs"].as_f64().unwrap()
        );
        assert!(
            timing["notificationToDispatchMs"].as_f64().unwrap()
                <= timing["notificationToActionReturnMs"].as_f64().unwrap()
        );
        assert!(timing["canonicalRead"]["queue_wait_ms"].is_number());
        assert!(timing["canonicalRead"]["throttle_wait_ms"].is_number());
        let core = &timing["coreHttp"];
        assert!(core["okxSwap"]["sinceNotificationStartMs"].is_number());
        if case == Case::RejectedPreview {
            assert!(core["binanceExecute"]["sinceNotificationStartMs"].is_null());
        } else {
            assert!(core["binanceExecute"]["sinceNotificationFinishMs"].is_number());
        }
    }
    if ambiguous
        || matches!(
            case,
            Case::RejectedPreview
                | Case::PendingExecute
                | Case::UnknownExecute
                | Case::DroppedExecuteReply
                | Case::ExecuteTimeout
                | Case::HashlessExecute
        )
    {
        let before: Value = serde_json::from_slice(
            &std::fs::read(root.path().join("pipeline-prepared-demo.json")).unwrap(),
        )
        .unwrap();
        let restarted = Arc::new(Daemon {
            wallets: daemon.wallets.clone(),
            registry: daemon.registry.clone(),
            scheduler: Arc::new(Scheduler::default()),
        });
        let recovered = restarted
            .scheduler
            .control(
                restarted.clone(),
                Control::Status {
                    id: "prepared-demo".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(recovered["status"], "PAUSED");
        assert!(restarted
            .scheduler
            .control(
                restarted.clone(),
                Control::Resume {
                    id: "prepared-demo".into(),
                    confirm: true
                }
            )
            .await
            .is_err());
        assert!(restarted
            .scheduler
            .control(
                restarted.clone(),
                Control::Arm {
                    id: "prepared-demo".into(),
                    confirm: true
                }
            )
            .await
            .is_err());
        let after: Value = serde_json::from_slice(
            &std::fs::read(root.path().join("pipeline-prepared-demo.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            before["recovery"]["deadlines"],
            after["recovery"]["deadlines"]
        );
        assert_eq!(
            before["recovery"]["preparation_deadlines"],
            after["recovery"]["preparation_deadlines"]
        );
        assert!(
            backend.requests.try_recv().is_err(),
            "recovery reissued a wallet request"
        );
        assert!(
            provider.requests.try_recv().is_err(),
            "recovery reissued an OKX request"
        );
    }
    if case == Case::NoFixedFloor {
        let value = &status["results"]["0:w1"];
        assert_eq!(value["minimumOutputPolicy"]["mode"], "fresh_route");
        assert!(value["minimumOutputPolicy"]["fixedUserMinimumUnits"].is_null());
        assert_eq!(value["reportedMinimumOutputUnits"], "1584000000000000");
        assert_eq!(
            value["runtimeBalanceValidation"]["minimumNetOutputUnits"],
            "1584000000000000"
        );
        assert_eq!(value["quote"]["toTokenAmount"], "1600000000000000");
    }
    drop(held_execute_replies);
    assert_eq!(route_requests, usize::from(!ambiguous));
    assert_eq!(hot_previews, usize::from(!ambiguous));
    assert_eq!(
        hot_executes,
        usize::from(!ambiguous && case != Case::RejectedPreview)
    );
    let journal: Vec<_> = std::fs::read_dir(root.path().join("w1/.baw-rs-operations"))
        .unwrap()
        .map(|p| p.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(journal.iter().filter(|p| p.starts_with("a-")).count(), 1);
    assert_eq!(
        journal.iter().filter(|p| p.starts_with("p-")).count(),
        usize::from(!ambiguous)
    );
    let stored_text =
        std::fs::read_to_string(root.path().join("pipeline-prepared-demo.json")).unwrap();
    assert!(!stored_text.contains("synthetic-secret"));
    assert!(!stored_text.contains("synthetic-pass"));
    if ambiguous {
        let stored: Value = serde_json::from_str(&stored_text).unwrap();
        assert!(stored["recovery"]["deadlines"]
            .as_object()
            .unwrap()
            .is_empty());
        assert_eq!(stored["recovery"]["preparation_started"], json!(["0:w1"]));
    }
    daemon.scheduler.stop().await;
    if let Some(job) = notification_job {
        tokio::time::timeout(Duration::from_secs(3), job)
            .await
            .unwrap()
            .unwrap();
        let markers: Vec<_> = std::fs::read_dir(root.path().join(".task-notifications"))
            .unwrap()
            .map(|p| std::fs::read_to_string(p.unwrap().path()).unwrap())
            .collect();
        assert_eq!(markers.len(), 1);
        assert!(!markers[0].contains("synthetic"));
        assert!(!markers[0].contains("private-notification-provider-error"));
    }
    tokio::time::timeout(Duration::from_secs(3), ws)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn failed_completion_notification_cannot_affect_swap_or_add_core_requests() {
    run_test_with_notification(Case::Success, true).await;
}
#[tokio::test]
async fn pending_app_and_unknown_results_get_distinct_sanitized_notifications() {
    for case in [Case::PendingExecute, Case::HashlessExecute] {
        run_test_with_notification(case, true).await;
    }
}

#[tokio::test]
async fn task_without_fixed_floor_uses_fresh_trigger_minimum_not_preparation_quote() {
    run_test(Case::NoFixedFloor).await;
}

#[tokio::test]
async fn task_prepares_before_subscription_and_only_swaps_after_event() {
    run_test(Case::Success).await;
}
#[tokio::test]
async fn unknown_approval_never_starts_the_event_listener() {
    run_test(Case::UncertainPreparation).await;
}

#[tokio::test]
async fn confirmed_swap_uses_fixed_sender_without_post_submit_backend_reads() {
    run_test(Case::Confirmed).await;
}

#[tokio::test]
async fn rejected_preview_pauses_without_state_refresh_or_execute() {
    run_test(Case::RejectedPreview).await;
}
#[tokio::test]
async fn pending_app_confirmation_pauses_without_retry() {
    run_test(Case::PendingExecute).await;
}
#[tokio::test]
async fn unknown_execute_pauses_without_retry() {
    run_test(Case::UnknownExecute).await;
}

#[tokio::test]
async fn approval_reply_lost_never_listens_or_reapproves_after_scheduler_reload() {
    run_test(Case::DroppedApprovalReply).await;
}

#[tokio::test]
async fn execute_reply_lost_after_backend_acceptance_pauses_without_retry() {
    run_test(Case::DroppedExecuteReply).await;
}
#[tokio::test]
async fn execute_timeout_after_backend_acceptance_pauses_without_retry() {
    run_test(Case::ExecuteTimeout).await;
}
#[tokio::test]
async fn order_id_without_hash_does_not_complete_or_advance_task() {
    run_test(Case::HashlessExecute).await;
}

#[tokio::test]
async fn compiled_wallet_requests_reject_other_wallets_controls_and_skip_flags() {
    let root = tempfile::tempdir().unwrap();
    let one = wallet(
        root.path(),
        "one",
        "one",
        A,
        Some("synthetic"),
        "http://127.0.0.1:9",
    );
    let two = wallet(
        root.path(),
        "two",
        "two",
        B,
        Some("synthetic"),
        "http://127.0.0.1:9",
    );
    let mut plan = one
        .compile(argv(&[
            "contract-call",
            "fire",
            "--binanceChainId",
            "56",
            "--from",
            fixtures::SENDER,
            "--to",
            ROUTER,
            "--value",
            "0",
            "--confirm",
            "--operation-id",
            "compiled-test",
        ]))
        .unwrap();
    assert!(two.execute_compiled(&plan).await.is_err());
    plan.skip_prechecks = true;
    assert!(one.execute_compiled(&plan).await.is_err());
    let control = one
        .compile(argv(&[
            "auth",
            "signin",
            "--confirm",
            "--operation-id",
            "not-a-task",
        ]))
        .unwrap();
    assert!(one.execute_compiled(&control).await.is_err());
}
