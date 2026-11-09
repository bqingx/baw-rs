//! One lazy WSS actor per instance: RPC replies and subscriptions share the socket.
//! No HTTP fallback or implicit reconnect. Last lease closes the transport.
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc, oneshot, Semaphore};
use tokio_tungstenite::tungstenite::Message;

pub struct SharedRpc {
    inner: Mutex<Option<Arc<Connection>>>,
    transport: Arc<Semaphore>,
}
impl Default for SharedRpc {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            transport: Arc::new(Semaphore::new(1)),
        }
    }
}
struct Connection {
    url: String,
    transport: Arc<Semaphore>,
    wire: tokio::sync::Mutex<Wire>,
}
#[derive(Default)]
struct Wire {
    worker: Option<Worker>,
    failed: bool,
    cache: BTreeMap<String, (Instant, Value)>,
    subscriptions: BTreeMap<String, (String, usize)>,
    last_request: Option<Instant>,
}
struct Worker {
    tx: mpsc::Sender<Command>,
    events: broadcast::Sender<Result<Event, String>>,
    task: tokio::task::JoinHandle<()>,
    failed: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct Command {
    method: String,
    params: Value,
    reply: oneshot::Sender<Result<Value, String>>,
}
#[derive(Clone, Debug)]
struct Event {
    subscription: String,
    value: Value,
    received_at: Instant,
}
/// Local completed text-frame arrival, not provider or chain time.
pub(crate) struct ReceivedNotification {
    pub value: Value,
    pub received_at: Instant,
}
/// Local read phases; exchange includes actor scheduling, transport and response parsing.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct RpcReadTiming {
    pub total_ms: f64,
    pub queue_wait_ms: f64,
    pub throttle_wait_ms: f64,
    pub exchange_ms: f64,
    pub cache_hit: bool,
}
pub struct Lease {
    owner: Arc<SharedRpc>,
    connection: Arc<Connection>,
}
pub struct Subscription {
    lease: Option<Lease>,
    key: String,
    id: String,
    receiver: broadcast::Receiver<Result<Event, String>>,
}
impl SharedRpc {
    pub fn acquire(self: &Arc<Self>, url: &str) -> Result<Lease, String> {
        let parsed = url::Url::parse(url).map_err(|_| "invalid RPC URL")?;
        let secure = parsed.scheme() == "wss";
        #[cfg(test)]
        let secure = secure || (parsed.scheme() == "ws" && parsed.host_str() == Some("127.0.0.1"));
        if !secure
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err("shared RPC requires a wss URL without userinfo or fragment".into());
        }
        let canonical = parsed.as_str();
        let mut slot = self.inner.lock().map_err(|_| "RPC manager unavailable")?;
        if slot.as_ref().is_some_and(|active| active.url != canonical) {
            return Err(
                "single-connection RPC endpoint already in use; no second connection opened".into(),
            );
        }
        let connection = slot
            .get_or_insert_with(|| {
                Arc::new(Connection {
                    url: canonical.into(),
                    transport: self.transport.clone(),
                    wire: tokio::sync::Mutex::new(Wire::default()),
                })
            })
            .clone();
        Ok(Lease {
            owner: self.clone(),
            connection,
        })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.owner.inner.lock() {
            if slot
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(s, &self.connection))
                && Arc::strong_count(&self.connection) == 2
            {
                slot.take();
            }
        }
    }
}
impl Wire {
    fn start(&mut self, connection: &Connection) -> Result<(), String> {
        if self.failed
            || self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.failed.load(std::sync::atomic::Ordering::SeqCst))
        {
            self.failed = true;
            return Err("shared RPC failed; explicit task recovery required".into());
        }
        if self.worker.is_none() {
            let (tx, rx) = mpsc::channel(64);
            let (events, _) = broadcast::channel(1024);
            let event_sender = events.clone();
            let url = connection.url.clone();
            let transport = connection.transport.clone();
            let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let actor_failed = failed.clone();
            let task = tokio::spawn(async move {
                if actor(url, transport, rx, event_sender.clone())
                    .await
                    .is_err()
                {
                    actor_failed.store(true, std::sync::atomic::Ordering::SeqCst);
                    let _ = event_sender.send(Err(
                        "shared RPC disconnected or malformed; no implicit reconnect".into(),
                    ));
                }
            });
            self.worker = Some(Worker {
                tx,
                events,
                task,
                failed,
            });
        }
        Ok(())
    }
    async fn exchange(
        &mut self,
        connection: &Connection,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        self.exchange_observed(connection, method, params)
            .await
            .map(|(value, _, _)| value)
    }
    async fn exchange_observed(
        &mut self,
        connection: &Connection,
        method: &str,
        params: Value,
    ) -> Result<(Value, f64, f64), String> {
        self.start(connection)?;
        let mut throttle_wait_ms = 0.0;
        if let Some(last) = self.last_request {
            let started = Instant::now();
            tokio::time::sleep(Duration::from_millis(250).saturating_sub(last.elapsed())).await;
            throttle_wait_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        self.last_request = Some(Instant::now());
        let worker = self.worker.as_ref().ok_or("RPC worker unavailable")?;
        let (reply, received) = oneshot::channel();
        let exchange_started = Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(15), async {
            worker
                .tx
                .send(Command {
                    method: method.into(),
                    params,
                    reply,
                })
                .await
                .map_err(|_| "RPC actor stopped")?;
            received.await.map_err(|_| "RPC response channel closed")?
        })
        .await;
        match result {
            Ok(Ok(value)) => Ok((
                value,
                throttle_wait_ms,
                exchange_started.elapsed().as_secs_f64() * 1000.0,
            )),
            failure => {
                let reason = match failure {
                    Ok(Err(reason)) => reason,
                    _ => "RPC response timeout".into(),
                };
                let error = format!(
                    "shared RPC {method} failed: {reason}; no implicit reconnect or replay"
                );
                self.failed = true;
                if let Some(worker) = self.worker.take() {
                    let _ = worker.events.send(Err(error.clone()));
                }
                self.cache.clear();
                Err(error)
            }
        }
    }
}
// Do not forward arbitrary provider text: it can contain credential-bearing URLs.
fn rejection_reason(error: &Value) -> String {
    let code = error["code"]
        .as_i64()
        .map(|code| format!(" (code {code})"))
        .unwrap_or_default();
    let message = error["message"].as_str().unwrap_or("").to_ascii_lowercase();
    let limit = if message.contains("10 block range") {
        "; provider permits at most 10 blocks per log request"
    } else {
        ""
    };
    format!("RPC server rejected request{code}{limit}")
}

async fn actor(
    url: String,
    transport: Arc<Semaphore>,
    mut commands: mpsc::Receiver<Command>,
    events: broadcast::Sender<Result<Event, String>>,
) -> Result<(), String> {
    // Transport permit outlives socket: a new actor cannot connect before this one drops it.
    let _permit = transport
        .acquire_owned()
        .await
        .map_err(|_| "RPC transport unavailable")?;
    let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(4 * 1024 * 1024);
    config.max_frame_size = Some(4 * 1024 * 1024);
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async_with_config(&url, Some(config), false),
    )
    .await
    .map_err(|_| "RPC connection timeout")?
    .map_err(|_| "RPC connection failed")?;
    let mut id = 0u64;
    let mut pending = BTreeMap::new();
    loop {
        tokio::select! {
            command=commands.recv()=> {
                let Some(command)=command else {return Ok(());};
                id=id.checked_add(1).ok_or("RPC ID exhausted")?;
                let text=json!({"jsonrpc":"2.0","id":id,"method":command.method,"params":command.params}).to_string();
                if pending.len() >= 64 { return Err("RPC pending request limit reached".into()); }
                pending.insert(id,command.reply);
                socket.send(Message::Text(text.into())).await.map_err(|_| "RPC send failed")?;
            }
            frame=socket.next()=> {
                match frame.ok_or("RPC closed")?.map_err(|_| "RPC receive failed")? {
                    Message::Ping(payload)=>socket.send(Message::Pong(payload)).await.map_err(|_| "RPC pong failed")?,
                    Message::Text(text)=> {
                        let received_at = Instant::now();
                        let body:Value=serde_json::from_str(&text).map_err(|_| "malformed RPC JSON")?;
                        if body["jsonrpc"]!="2.0" {return Err("invalid JSON-RPC version".into());}
                        if body["method"]=="eth_subscription" {
                            let subscription=body.pointer("/params/subscription").and_then(Value::as_str).filter(|s|!s.is_empty()).ok_or("invalid subscription ID")?;
                            let value=body.pointer("/params/result").cloned().ok_or("subscription result missing")?;
                            let _=events.send(Ok(Event {subscription:subscription.into(),value,received_at}));
                        } else {
                            let id=body["id"].as_u64().ok_or("RPC response ID missing")?;
                            let reply=pending.remove(&id).ok_or("unexpected RPC response ID")?;
                            let response=if body.get("error").is_some_and(|v|!v.is_null()) {Err(rejection_reason(&body["error"]))} else {body.get("result").cloned().ok_or_else(|| "RPC result missing".into())};
                            let _=reply.send(response);
                        }
                    }
                    Message::Close(_)=>return Err("RPC closed".into()),
                    Message::Binary(_)=>return Err("unexpected binary RPC message".into()),
                    _=>{},
                }
            }
        }
    }
}
impl Lease {
    pub async fn read(&self, method: &str, params: Value) -> Result<Value, String> {
        self.read_observed(method, params)
            .await
            .map(|(value, _)| value)
    }
    pub(crate) async fn read_observed(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(Value, RpcReadTiming), String> {
        let started = Instant::now();
        if !matches!(
            method,
            "eth_chainId"
                | "eth_call"
                | "eth_blockNumber"
                | "eth_getLogs"
                | "eth_getTransactionReceipt"
                | "eth_getBlockByNumber"
        ) {
            return Err("RPC method is not an allowed chain read".into());
        }
        let key = json!([method, params]).to_string();
        let queued = Instant::now();
        let mut wire = self.connection.wire.lock().await;
        let queue_wait_ms = queued.elapsed().as_secs_f64() * 1000.0;
        wire.start(&self.connection)?;
        if let Some((at, value)) = wire.cache.get(&key) {
            if at.elapsed() < Duration::from_millis(250) {
                return Ok((
                    value.clone(),
                    RpcReadTiming {
                        total_ms: started.elapsed().as_secs_f64() * 1000.0,
                        queue_wait_ms,
                        throttle_wait_ms: 0.0,
                        exchange_ms: 0.0,
                        cache_hit: true,
                    },
                ));
            }
        }
        let (value, throttle_wait_ms, exchange_ms) = wire
            .exchange_observed(&self.connection, method, params)
            .await?;
        wire.cache
            .retain(|_, (at, _)| at.elapsed() < Duration::from_millis(250));
        if wire.cache.len() >= 256 {
            wire.cache.clear();
        }
        wire.cache.insert(key, (Instant::now(), value.clone()));
        Ok((
            value,
            RpcReadTiming {
                total_ms: started.elapsed().as_secs_f64() * 1000.0,
                queue_wait_ms,
                throttle_wait_ms,
                exchange_ms,
                cache_hit: false,
            },
        ))
    }
    /// Exact subscription parameters share a server subscription and notification buffer.
    pub async fn subscribe(&self, params: Value) -> Result<Subscription, String> {
        let kind = params
            .get(0)
            .and_then(Value::as_str)
            .ok_or("subscription kind missing")?;
        if !matches!(kind, "newHeads" | "logs") {
            return Err("only confirmed block/log subscriptions are supported".into());
        }
        let key = params.to_string();
        let mut wire = self.connection.wire.lock().await;
        wire.start(&self.connection)?;
        let receiver = wire
            .worker
            .as_ref()
            .ok_or("RPC worker unavailable")?
            .events
            .subscribe();
        let id = if let Some((id, count)) = wire.subscriptions.get_mut(&key) {
            *count += 1;
            id.clone()
        } else {
            let value = wire
                .exchange(&self.connection, "eth_subscribe", params)
                .await?;
            let id = value
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or("invalid RPC subscription result")?
                .to_owned();
            wire.subscriptions.insert(key.clone(), (id.clone(), 1));
            id
        };
        let lease = self.owner.acquire(&self.connection.url)?;
        Ok(Subscription {
            lease: Some(lease),
            key,
            id,
            receiver,
        })
    }
}
impl Subscription {
    pub async fn recv(&mut self) -> Result<Value, String> {
        self.recv_observed().await.map(|event| event.value)
    }
    pub(crate) async fn recv_observed(&mut self) -> Result<ReceivedNotification, String> {
        loop {
            match self
                .receiver
                .recv()
                .await
                .map_err(|_| "subscription lost or overflowed; task must pause")??
            {
                event if event.subscription == self.id => {
                    return Ok(ReceivedNotification {
                        value: event.value,
                        received_at: event.received_at,
                    })
                }
                _ => {}
            }
        }
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let key = self.key.clone();
        // No blocking network work in Drop. A temporary lease keeps the actor alive
        // until unsubscribe completes; ordinary runtime shutdown may abort it safely.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut wire = lease.connection.wire.lock().await;
                if let Some((id, count)) = wire.subscriptions.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let id = id.clone();
                        wire.subscriptions.remove(&key);
                        let _ = wire
                            .exchange(&lease.connection, "eth_unsubscribe", json!([id]))
                            .await;
                    }
                }
            });
        }
    }
}
#[cfg(test)]
#[path = "chain_rpc_tests.rs"]
mod tests;
