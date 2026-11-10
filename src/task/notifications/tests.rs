use super::*;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn event() -> Event {
    Event {
        task_id: "task-1".into(),
        kind: "unknown",
        completion: "incomplete",
        wallets: vec!["w1".into()],
        confirmed: 0,
        submitted: 0,
        unresolved: 1,
        failed: 0,
        pending: 0,
        awaiting_app: 0,
        outcomes: vec![vec!["uncertain"]],
    }
}
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}
fn config(root: &Path) -> PathBuf {
    let path = root.join("notify.json");
    std::fs::write(
        &path,
        r#"{"token":"synthetic-notification-token","channel":"wechat"}"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}
async fn server(
    status: &str,
    body: &str,
) -> (String, mpsc::Receiver<String>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel(8);
    let status = status.to_owned();
    let body = body.to_owned();
    let job = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut data = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = stream.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..n]);
                if let Some(index) = data.windows(4).position(|x| x == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&data[..index]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if data.len() >= index + 4 + length {
                        break;
                    }
                }
            }
            sender.send(String::from_utf8(data).unwrap()).await.unwrap();
            let response=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (url, receiver, job)
}
fn markers(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_dir(root.join(".task-notifications"))
        .unwrap()
        .map(|p| serde_json::from_slice(&std::fs::read(p.unwrap().path()).unwrap()).unwrap())
        .collect()
}
#[tokio::test]
async fn failed_delivery_is_deduplicated_in_memory_and_after_notifier_restart() {
    let root = root();
    let config = config(root.path());
    let (url, mut requests, job) = server(
        "503 Unavailable",
        r#"{"error":"synthetic-private-provider-body"}"#,
    )
    .await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.publish(root.path(), event());
    notifier.flush().await;
    let request = requests.recv().await.unwrap();
    assert!(request.starts_with("POST / "));
    let payload: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(payload["channel"], "wechat");
    assert_eq!(payload["template"], "txt");
    assert!(payload["content"].as_str().unwrap().contains("结果未知"));
    assert!(requests.try_recv().is_err());
    let rows = markers(root.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "FAILED_OR_UNKNOWN");
    assert!(!rows[0].to_string().contains("synthetic"));
    let restarted = Notifier::for_test(&config, &url).unwrap();
    restarted.publish(root.path(), event());
    restarted.flush().await;
    assert!(requests.try_recv().is_err());
    job.abort();
}
#[tokio::test]
async fn provider_acceptance_is_recorded_as_accepted_not_delivered() {
    let root = root();
    let config = config(root.path());
    let (url, mut requests, job) = server("200 OK", r#"{"code":200}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert!(requests.try_recv().is_ok());
    assert_eq!(markers(root.path())[0]["state"], "ACCEPTED");
    job.abort();
}
#[tokio::test]
async fn rejected_provider_envelope_is_not_mistaken_for_delivery() {
    let root = root();
    let config = config(root.path());
    let (url, _requests, job) =
        server("200 OK", r#"{"code":500,"msg":"private-provider-error"}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert_eq!(markers(root.path())[0]["state"], "FAILED_OR_UNKNOWN");
    job.abort();
}
#[tokio::test]
async fn redirects_cannot_forward_notification_credentials() {
    let root = root();
    let config = config(root.path());
    let (target, mut target_requests, target_job) = server("200 OK", r#"{"code":200}"#).await;
    let (url, mut requests, job) = server(&format!("302 Found\r\nLocation: {target}"), "").await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert!(requests.try_recv().is_ok());
    assert!(target_requests.try_recv().is_err());
    assert_eq!(markers(root.path())[0]["state"], "FAILED_OR_UNKNOWN");
    job.abort();
    target_job.abort();
}
#[tokio::test]
async fn oversized_response_is_bounded_and_not_saved_or_retried() {
    let root = root();
    let config = config(root.path());
    let (url, mut requests, job) = server("200 OK", &"sensitive-provider-data".repeat(1024)).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert!(requests.try_recv().is_ok());
    assert!(requests.try_recv().is_err());
    assert_eq!(markers(root.path())[0]["state"], "FAILED_OR_UNKNOWN");
    job.abort();
}
#[tokio::test]
async fn durable_claim_without_outcome_is_never_resent_after_restart() {
    let root = root();
    let config = config(root.path());
    let claimed = claim(root.path(), &event().key()).unwrap().unwrap();
    drop(claimed);
    let (url, mut requests, job) = server("200 OK", r#"{"code":200}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert!(requests.try_recv().is_err());
    assert_eq!(markers(root.path())[0]["state"], "CLAIMED");
    job.abort();
}
#[test]
fn config_requires_private_file_explicit_channel_and_no_unknown_fields() {
    let root = root();
    let path = config(root.path());
    assert!(Config::read(&path, "https://www.pushplus.plus/send").is_ok());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Config::read(&path, "https://www.pushplus.plus/send").is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    for value in [
        r#"{"token":"synthetic"}"#,
        r#"{"token":"synthetic","channel":"other"}"#,
        r#"{"token":"synthetic","channel":"app","endpoint":"https://evil.example"}"#,
    ] {
        std::fs::write(&path, value).unwrap();
        assert!(Config::read(&path, "https://www.pushplus.plus/send").is_err());
    }
    let link = root.path().join("link");
    symlink(&path, &link).unwrap();
    assert!(Config::read(&link, "https://www.pushplus.plus/send").is_err());
    assert!(Notifier::for_test(&path, "https://external.example").is_err());
}
#[tokio::test]
async fn unsafe_identifiers_and_unavailable_ledger_never_send() {
    let root = root();
    let config = config(root.path());
    let (url, mut requests, job) = server("200 OK", r#"{"code":200}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    let mut unsafe_event = event();
    unsafe_event.task_id = "credential=https://private".into();
    notifier.publish(root.path(), unsafe_event);
    notifier.flush().await;
    assert!(!root.path().join(".task-notifications").exists());
    let target = root.path().join("other");
    std::fs::create_dir(&target).unwrap();
    symlink(&target, root.path().join(".task-notifications")).unwrap();
    notifier.publish(root.path(), event());
    notifier.flush().await;
    assert!(requests.try_recv().is_err());
    job.abort();
}
#[tokio::test]
async fn large_valid_task_notification_truncates_display_not_delivery() {
    let root = root();
    let config = config(root.path());
    let (url, mut requests, job) = server("200 OK", r#"{"code":200}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    let mut event = event();
    event.wallets = (0..144).map(|i| format!("w{i}")).collect();
    event.outcomes = vec![vec!["pending"; 16]; 9];
    event.pending = 144;
    event.unresolved = 0;
    event.kind = "cancelled";
    notifier.publish(root.path(), event);
    notifier.flush().await;
    let request = requests.try_recv().unwrap();
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(body["content"].as_str().unwrap().contains("另有 128 个"));
    assert!(!body["content"].as_str().unwrap().contains("w143"));
    assert_eq!(markers(root.path())[0]["state"], "ACCEPTED");
    job.abort();
}

#[tokio::test]
async fn disabled_notifier_does_not_create_ledger() {
    let root = root();
    Notifier::default().publish(root.path(), event());
    assert!(!root.path().join(".task-notifications").exists());
}
#[tokio::test]
async fn bounded_queue_never_waits_for_network_capacity() {
    let root = root();
    let config = config(root.path());
    let (url, _requests, job) = server("200 OK", r#"{"code":200}"#).await;
    let notifier = Notifier::for_test(&config, &url).unwrap();
    // No await: this current-thread test cannot poll the network worker here.
    for index in 0..64 {
        let mut event = event();
        event.task_id = format!("task-{index}");
        notifier.publish(root.path(), event);
    }
    assert_eq!(
        notifier
            .0
            .as_ref()
            .unwrap()
            .sender
            .get()
            .unwrap()
            .capacity(),
        0
    );
    assert_eq!(notifier.status()["droppedQueueEvents"], 32);
    job.abort();
}

#[test]
fn outcome_replaces_the_claim_atomically_instead_of_rewriting_it_in_place() {
    let root = root();
    let key = event().key();
    let claimed = claim(root.path(), &key).unwrap().unwrap();
    let claimed_inode = claimed.metadata().unwrap().ino();
    drop(claimed);
    record(root.path(), &key, false).unwrap();
    let dir = root.path().join(".task-notifications");
    let marker = dir.join(format!("{key}.json"));
    // A new file published by rename: no reader can ever observe a truncated record, and the
    // claim name itself never disappears, so at-most-once still holds.
    assert_ne!(std::fs::metadata(&marker).unwrap().ino(), claimed_inode);
    assert_eq!(
        std::fs::read(&marker).unwrap(),
        b"{\"state\":\"FAILED_OR_UNKNOWN\"}".to_vec()
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}
