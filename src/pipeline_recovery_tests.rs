//! Real local IPC/executor + synthetic chain proof: no production service reachable.
use super::super::task_runtime::{Control, Definition, Scheduler};
use super::fixtures;
use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn changing_session_after_authorization_blocks_even_the_next_backend_read() {
    let root = tempfile::tempdir().unwrap();
    let wallet = wallet(
        root.path(),
        "bound",
        "identity",
        A,
        Some("old-cookie"),
        "http://127.0.0.1:9",
    );
    let expected = wallet.pipeline_binding().unwrap();
    let selected = wallet.clone();
    let check = Arc::new(move || {
        if selected.pipeline_binding()? == expected {
            Ok(())
        } else {
            Err("wallet changed".into())
        }
    });
    assert!(crate::execution::binding_scope(check.clone(), async {
        crate::execution::check_task_gate()
    })
    .await
    .is_ok());
    save(&wallet.entry.dir, "identity", A, "new-cookie");
    assert!(
        crate::execution::binding_scope(check, async { crate::execution::check_task_gate() })
            .await
            .is_err()
    );
    assert!(
        crate::execution::check_task_gate().is_ok(),
        "binding policy leaked outside its task"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconcile_then_resume_never_resubmits_the_confirmed_wallet_or_renews_deadline() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut backend = Mock::new().await;
    let w1 = wallet(root.path(), "w1", "one", A, Some("one"), &backend.origin);
    let w2 = wallet(root.path(), "w2", "two", B, Some("two"), &backend.origin);
    let wallets = Arc::new(RwLock::new(BTreeMap::from([
        ("w1".into(), w1.clone()),
        ("w2".into(), w2.clone()),
    ])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let rpc_server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                while let Some(Ok(frame)) = socket.next().await {
                    if !frame.is_text() {
                        continue;
                    }
                    let request: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
                    let block_hash = format!("0x{}", "cc".repeat(32));
                    let result = match request["method"].as_str().unwrap() {
                        "eth_chainId" => json!("0x38"),
                        "eth_getTransactionReceipt" => {
                            json!({"transactionHash":request["params"][0],"from":fixtures::SENDER,"status":"0x1","blockNumber":"0x10","blockHash":block_hash})
                        }
                        "eth_blockNumber" => json!("0x10"),
                        "eth_getBlockByNumber" => json!({"number":"0x10","hash":block_hash}),
                        method => panic!("unexpected recovery RPC {method}"),
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
        }
    });
    let action = |name: &str| json!({"wallet":name,"argv":["contract-call","fire","--binanceChainId","56","--from",fixtures::SENDER,"--to","0x2222222222222222222222222222222222222222","--value","0","--inputData","0x12345678"]});
    let config:Definition=serde_json::from_value(json!({"id":"recover","chain_id":"56","rpc_url":url,"steps":[
        {"task_id":"first","trigger":{"kind":"immediate"},"valid_for_ms":120000,"actions":[action("w1"),action("w2")]},
        {"task_id":"second","trigger":{"kind":"immediate"},"valid_for_ms":120000,"completion":"submitted","actions":[action("w2")]}
    ]})).unwrap();
    let daemon = Arc::new(Daemon {
        wallets: wallets.clone(),
        registry: root.path().into(),
        scheduler: Arc::new(Scheduler::default()),
    });
    daemon
        .scheduler
        .control(daemon.clone(), Control::Load { config })
        .await
        .unwrap();
    let file = root.path().join("pipeline-recover.json");
    let mut stored: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let deadline = chrono::Utc::now().timestamp_millis() as u64 + 120000;
    stored["status"] = json!("RUNNING");
    stored["snapshot"]["outcomes"][0][0] = json!("in_flight");
    stored["results"]["0:w1"] =
        json!({"execution":{"txHash":format!("0x{}","aa".repeat(32)),"status":"BROADCASTED"}});
    stored["recovery"] = json!({"deadlines":{"0":deadline},"triggered":[0],"bindings":{"w1":w1.pipeline_binding().unwrap(),"w2":w2.pipeline_binding().unwrap()}});
    std::fs::write(&file, serde_json::to_vec(&stored).unwrap()).unwrap();
    let daemon = Arc::new(Daemon {
        wallets,
        registry: root.path().into(),
        scheduler: Arc::new(Scheduler::default()),
    });
    assert!(daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Resume {
                id: "recover".into(),
                confirm: false
            }
        )
        .await
        .is_err());
    assert!(
        daemon
            .scheduler
            .control(
                daemon.clone(),
                Control::Resume {
                    id: "recover".into(),
                    confirm: true
                }
            )
            .await
            .is_err(),
        "uncertain action must not resume"
    );
    let d = daemon.clone();
    let reconcile = tokio::spawn(async move {
        d.scheduler
            .control(
                d.clone(),
                Control::Reconcile {
                    id: "recover".into(),
                },
            )
            .await
    });
    let (request, reply) = backend.next().await;
    assert!(request.contains("/mpc-wallet/list"));
    reply.send((json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":fixtures::SENDER}]}]}]),None)).unwrap();
    let (request, reply) = backend.next().await;
    assert!(request.contains("/networks/active"));
    reply.send((json!([]), None)).unwrap();
    let result = reconcile.await.unwrap().unwrap();
    assert_eq!(result["writesSubmitted"], 0);
    assert_eq!(result["snapshot"]["outcomes"][0][0], "confirmed");
    assert_eq!(result["status"], "RECONCILED");
    daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Resume {
                id: "recover".into(),
                confirm: true,
            },
        )
        .await
        .unwrap();
    let mut executions = 0;
    tokio::time::timeout(Duration::from_secs(90),async {
        loop {
            let status=daemon.scheduler.control(daemon.clone(),Control::Status {id:"recover".into()}).await.unwrap();
            if status["status"]!="RUNNING" {assert_eq!(status["status"],"COMPLETED","{status}");break;}
            tokio::select! {
                row=backend.requests.recv()=> {
                    let (request,reply)=row.unwrap();
                    let body=if request.contains("settings/query") {json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}})}
                    else if request.contains("/mpc-wallet/list") {json!([{"subWalletList":[{"addresses":[{"binanceChainId":"56","address":fixtures::SENDER}]}]}])}
                    else if request.contains("/networks/active") {json!([])}
                    else if request.contains("/contract-call/preview") {assert!(request.contains(B),"confirmed w1 was replayed");fixtures::valid_preview(&format!("resume-preview-{executions}"))}
                    else if request.contains("/contract-call/execute") {assert!(request.contains(B),"confirmed w1 was replayed");executions+=1;json!({"txHash":format!("0x{}","bb".repeat(32)),"status":"BROADCASTED"})}
                    else {panic!("unexpected resume request {request}")};
                    reply.send((body,None)).unwrap();
                }
                _=tokio::time::sleep(Duration::from_millis(10))=>{},
            }
        }
    }).await.unwrap();
    assert_eq!(
        executions, 2,
        "only pending w2 in first and second steps should execute"
    );
    let persisted: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(persisted["recovery"]["deadlines"]["0"], deadline);
    assert!(daemon
        .scheduler
        .control(
            daemon.clone(),
            Control::Resume {
                id: "recover".into(),
                confirm: true
            }
        )
        .await
        .is_err());
    daemon.scheduler.stop().await;
    rpc_server.abort();
}
