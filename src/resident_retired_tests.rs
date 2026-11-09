//! Removed protocols fail closed; no compatibility path may sign or start listeners.
use super::*;
#[tokio::test]
async fn old_wallet_task_and_typed_frames_are_rejected_without_wallet_work() {
    for frame in [
        json!({"op":"native","id":"old","name":"alice","command":{"action":"arm","task_id":"t","confirm":true}}),
        json!({"op":"prepare","id":"old","name":"alice","operation_id":"t","deadline_ms":u64::MAX,"call":{}}),
        json!({"op":"trigger","id":"old","name":"alice","operation_id":"t","confirm":true}),
        json!({"op":"disarm","id":"old","name":"alice","operation_id":"t"}),
    ] {
        let (mut client, server) = UnixStream::pair().unwrap();
        let task = tokio::spawn(handle(
            server,
            Arc::new(Daemon {
                wallets: Arc::new(RwLock::new(BTreeMap::new())),
                registry: None,
                scheduler: Arc::new(super::super::task_runtime::Scheduler::default()),
            }),
            Arc::new(Notify::new()),
            Arc::new(Semaphore::new(1)).acquire_owned().await.unwrap(),
            Arc::new(Semaphore::new(16)),
            tokio::sync::watch::channel(false).0,
        ));
        client
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .unwrap();
        let reply: Value =
            serde_json::from_slice(&read_frame(&mut client, MAX_RESPONSE).await.unwrap()).unwrap();
        assert_eq!(reply["success"], false);
        assert_eq!(reply["error"]["message"], "invalid IPC request");
        task.await.unwrap();
    }
}
