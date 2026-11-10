//! Optional best-effort notifications, outside wallet request scopes and critical saves.
//! Durable pre-send claims give at-most-once attempts, NOT exactly-once delivery.
use crate::util::fsx::{atomic_write, create_private_once, open_dir, private_dir};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Event {
    pub task_id: String,
    pub kind: &'static str,
    pub completion: &'static str,
    pub wallets: Vec<String>,
    pub confirmed: usize,
    pub submitted: usize,
    pub unresolved: usize,
    pub failed: usize,
    pub pending: usize,
    pub awaiting_app: usize,
    pub outcomes: Vec<Vec<&'static str>>,
}
impl Event {
    fn safe(&self) -> bool {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        id(&self.task_id)
            && self.wallets.len() <= 4096
            && self.wallets.iter().all(|w| id(w))
            && matches!(
                self.kind,
                "completed"
                    | "cancelled"
                    | "awaiting_app"
                    | "unknown"
                    | "failed"
                    | "expired"
                    | "paused"
                    | "mixed"
            )
            && matches!(self.completion, "confirmed" | "submitted" | "incomplete")
    }
    fn key(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).expect("typed notification event"))
        )
    }
    fn text(&self) -> String {
        let label = match self.kind {
            "completed" if self.completion == "confirmed" => "链上确认完成",
            "completed" => "提交阶段完成（不代表链上成功）",
            "cancelled" => "已取消（已提交交易不可撤回）",
            "awaiting_app" => "已暂停：等待 App 确认；勿重复提交",
            "unknown" => "已暂停：结果未知；先对账，禁止盲目重试",
            "mixed" => "已暂停：存在不同的未知、待确认或失败结果；逐钱包核验，禁止盲目重试",
            "failed" => "已暂停：动作失败",
            "expired" => "已暂停：原有效期已过，不续期",
            _ => "已暂停：需人工检查",
        };
        let omitted = self.wallets.len().saturating_sub(16);
        let wallet_label = format!(
            "{}{}",
            self.wallets
                .iter()
                .take(16)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if omitted > 0 {
                format!("（另有 {omitted} 个）")
            } else {
                String::new()
            }
        );
        format!(
            "任务 {}：{}\n钱包：{}\n已确认 {} / 已提交 {} / 未解决 {} / 失败 {} / 未开始 {} / 待 App {}", 
            self.task_id,
            label,
            wallet_label,
            self.confirmed,
            self.submitted,
            self.unresolved,
            self.failed,
            self.pending,
            self.awaiting_app
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    token: String,
    channel: Channel,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Channel {
    App,
    Wechat,
}
#[derive(Clone)]
struct Config {
    token: Arc<zeroize::Zeroizing<String>>,
    channel: Channel,
    endpoint: String,
    client: reqwest::Client,
}
impl Config {
    fn read(path: &Path, endpoint: &str) -> Result<Self, ()> {
        if !path.is_absolute() {
            return Err(());
        }
        crate::util::fsx::private_dir(path.parent().ok_or(())?).map_err(|_| ())?;
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        crate::util::fsx::private_file(path, false)
            .map_err(|_| ())?
            .take(8193)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() > 8192 {
            return Err(());
        }
        let config: FileConfig = serde_json::from_slice(&bytes).map_err(|_| ())?;
        let token = zeroize::Zeroizing::new(config.token);
        if token.is_empty() || token.len() > 1024 || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| ())?;
        Ok(Self {
            token: Arc::new(token),
            channel: config.channel,
            endpoint: endpoint.into(),
            client,
        })
    }
    async fn send(&self, event: &Event) -> bool {
        let Ok(mut response)=self.client.post(&self.endpoint).json(&json!({"token":self.token.as_str(),"channel":self.channel,"title":"baw-rs 任务状态","content":event.text(),"template":"txt"})).send().await else {return false};
        if !response.status().is_success() {
            return false;
        }
        let mut bytes = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if bytes.len() + chunk.len() > 4096 {
                        return false;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => return false,
            }
        }
        serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(|v| v["code"] == 200)
    }
}
enum Message {
    Event(PathBuf, Event),
    Flush(oneshot::Sender<()>),
}
struct Inner {
    config: Config,
    sender: OnceLock<mpsc::Sender<Message>>,
    dropped: AtomicU64,
}
#[derive(Clone)]
// Unit/subprocess fixtures must never consume the host's notification credentials.
#[cfg_attr(test, derive(Default))]
pub(crate) struct Notifier(Option<Arc<Inner>>);
#[cfg(not(test))]
impl Default for Notifier {
    fn default() -> Self {
        {
            let Some(path) = std::env::var_os("BAW_RS_NOTIFY_CONFIG") else {
                return Self(None);
            };
            match Config::read(Path::new(&path), "https://www.pushplus.plus/send") {
                Ok(config) => Self(Some(Arc::new(Inner {
                    config,
                    sender: OnceLock::new(),
                    dropped: AtomicU64::new(0),
                }))),
                Err(()) => Self(None),
            }
        }
    }
}
impl Notifier {
    pub(crate) fn status(&self) -> serde_json::Value {
        json!({"enabled":self.0.is_some(),"provider":"pushplus","delivery":"best_effort_at_most_once_attempt","queueCapacity":32,"droppedQueueEvents":self.0.as_ref().map_or(0,|inner|inner.dropped.load(Ordering::Relaxed))})
    }
    pub(crate) fn publish(&self, registry: &Path, event: Event) {
        let Some(inner) = &self.0 else { return };
        if !event.safe() {
            return;
        }
        let sender = inner.sender.get_or_init(|| {
            let (sender, mut receiver) = mpsc::channel(32);
            let config = inner.config.clone();
            tokio::spawn(async move {
                while let Some(message) = receiver.recv().await {
                    match message {
                        Message::Flush(reply) => {
                            let _ = reply.send(());
                        }
                        Message::Event(registry, event) => {
                            let key = event.key();
                            let claimed = {
                                let (registry, key) = (registry.clone(), key.clone());
                                tokio::task::spawn_blocking(move || claim(&registry, &key)).await
                            };
                            let Ok(Ok(Some(_claimed))) = claimed else {
                                continue;
                            };
                            let accepted = config.send(&event).await;
                            let _ = tokio::task::spawn_blocking(move || {
                                record(&registry, &key, accepted)
                            })
                            .await;
                        }
                    }
                }
            });
            sender
        });
        // Notification backpressure must never become wallet/trigger backpressure.
        if sender
            .try_send(Message::Event(registry.into(), event))
            .is_err()
        {
            inner.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(crate) async fn flush(&self) {
        let Some(sender) = self.0.as_ref().and_then(|i| i.sender.get()) else {
            return;
        };
        let (reply, wait) = oneshot::channel();
        if sender.try_send(Message::Flush(reply)).is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(2), wait).await;
        }
    }
    #[cfg(test)]
    pub(crate) fn for_test(path: &Path, endpoint: &str) -> Result<Self, ()> {
        let url = reqwest::Url::parse(endpoint).map_err(|_| ())?;
        if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
            return Err(());
        }
        Ok(Self(Some(Arc::new(Inner {
            config: Config::read(path, endpoint)?,
            sender: OnceLock::new(),
            dropped: AtomicU64::new(0),
        }))))
    }
}
fn claim(registry: &Path, key: &str) -> Result<Option<std::fs::File>, ()> {
    private_dir(registry).map_err(|_| ())?;
    let directory = registry.join(".task-notifications");
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(()),
    }
    private_dir(&directory).map_err(|_| ())?;
    let name = std::ffi::CString::new(format!("{key}.json")).map_err(|_| ())?;
    // `O_EXCL` creation is the at-most-once claim; an existing marker is never rewritten.
    let claimed = create_private_once(
        &open_dir(&directory).map_err(|_| ())?,
        &name,
        b"{\"state\":\"CLAIMED\"}",
    )
    .map_err(|_| ())?;
    // Persist the claim directory entry itself before the claim can authorize a send.
    std::fs::File::open(registry)
        .and_then(|d| d.sync_all())
        .map_err(|_| ())?;
    Ok(claimed)
}

/// Replace the claim marker with the recorded outcome. The claim name itself never
/// disappears, so at-most-once is unaffected, but a reader now sees either the claim or the
/// outcome, never a truncated file.
fn record(registry: &Path, key: &str, accepted: bool) -> Result<(), ()> {
    let directory = open_dir(&registry.join(".task-notifications")).map_err(|_| ())?;
    let name = std::ffi::CString::new(format!("{key}.json")).map_err(|_| ())?;
    let body: &[u8] = if accepted {
        b"{\"state\":\"ACCEPTED\"}"
    } else {
        b"{\"state\":\"FAILED_OR_UNKNOWN\"}"
    };
    atomic_write(&directory, &name, body).map_err(|_| ())
}
#[cfg(test)]
mod tests;
