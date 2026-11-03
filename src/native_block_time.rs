//! `block_time` trigger source: fire on the first block whose chain timestamp reaches the
//! configured threshold.
//!
//! Two readers, one decision:
//! - `newHeads` over WS is the fast path;
//! - `eth_getBlockByNumber("latest")` is the catch-up path, used when WS is slow, silent,
//!   unsupported or disconnected. It is polled on the same tick, so a provider without
//!   subscriptions still arms and fires instead of failing.
//!
//! Both feed the same `enqueue`, so a block seen twice (WS + HTTP, or re-delivered after a
//! reconnect) is deduplicated by block hash and terminal execution is guarded by the operation
//! state machine. Only the chain timestamp counts; the local clock is never a trigger.
//!
//! Expectation management, by design: by the time a qualifying header exists, that block is
//! sealed. This submits into the *next* block, so it removes dependence on another wallet's
//! event and on clock drift — it does not win the block that crossed the threshold.
use super::*;

/// Stable trigger id for a block header. Fails closed on a header that is malformed, before the
/// threshold, or already marked as a reorg casualty, so a wrong id can never reach `enqueue`.
pub(in crate::resident) fn id(c: &Config, header: &Value, timestamp_gte: u64) -> Result<String, String> {
    if header.get("removed").and_then(Value::as_bool) == Some(true) {
        return Err("removed block header; reconciliation required".into());
    }
    let hash = header["hash"]
        .as_str()
        .filter(|s| hex(s, 32))
        .ok_or("block hash missing/malformed")?;
    // `number` and `timestamp` are quantitities: 0x-prefixed minimal hex.
    quantity(&header["number"])?;
    let timestamp = quantity(&header["timestamp"])?;
    if timestamp < timestamp_gte {
        return Err("block timestamp before authorized threshold".into());
    }
    let mut h = Sha256::new();
    for part in [&c.task_id, &c.chain_id, hash] {
        h.update(part.to_ascii_lowercase().as_bytes());
        h.update([0]);
    }
    let hex = format!("{:x}", h.finalize());
    Ok(format!("blk-{}", &hex[..60]))
}

/// Push a header into the queue when it satisfies the threshold. Returns whether it was queued.
fn observe(
    c: &Config,
    header: &Value,
    timestamp_gte: u64,
    origin: &'static str,
    tx: &tokio::sync::broadcast::Sender<Event>,
    seen: &mut RecentIds,
) -> Result<bool, String> {
    if id(c, header, timestamp_gte).is_err() {
        return Ok(false);
    }
    enqueue(c, header.clone(), origin, tx, seen)?;
    Ok(true)
}

async fn latest_header(c: &Config) -> Result<Value, String> {
    let header = rpc(c, "eth_getBlockByNumber", json!(["latest", false])).await?;
    if !header.is_object() {
        return Err("latest block header missing".into());
    }
    Ok(header)
}

pub(super) async fn run(
    c: &Config,
    tx: &tokio::sync::broadcast::Sender<Event>,
    health: &tokio::sync::watch::Sender<Health>,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
) {
    let TriggerSource::BlockTime { timestamp_gte } = c.source.clone() else {
        return;
    };
    let mut seen = RecentIds::default();
    // The catch-up read runs before the first connection attempt: a task armed after the
    // threshold must behave the same whether the provider supports subscriptions or not.
    let mut reconnects = 0u32;
    loop {
        if *cancel.borrow() {
            return;
        }
        health.send_replace(Health::Connecting);
        // HTTP first: this both proves the chain answers and covers a threshold that is already
        // past when the task arms.
        match latest_header(c).await {
            Ok(header) => {
                if observe(c, &header, timestamp_gte, "head_http", tx, &mut seen)
                    .unwrap_or(false)
                {
                    health.send_replace(Health::Ready);
                    return;
                }
            }
            Err(error) => {
                health.send_replace(Health::Fault(error));
                return;
            }
        }
        let connected = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async_with_config(
                &c.ws_url,
                Some(
                    tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
                        .max_message_size(Some(65536))
                        .max_frame_size(Some(65536)),
                ),
                false,
            ),
        )
        .await;
        let mut ws = match connected {
            Ok(Ok((ws, _))) => ws,
            // No subscription support is not fatal here: the poll below is a real catch-up path,
            // not a placeholder, so keep running instead of faulting the task.
            _ => {
                reconnects += 1;
                if reconnects > 5 {
                    health.send_replace(Health::Fault(
                        "WS connect and HTTP catch-up both unavailable".into(),
                    ));
                    return;
                }
                if poll_until(c, timestamp_gte, tx, &mut seen, cancel, health, reconnects).await
                {
                    return;
                }
                continue;
            }
        };
        if ws
            .send(tokio_tungstenite::tungstenite::Message::Text(
                json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]})
                    .to_string()
                    .into(),
            ))
            .await
            .is_err()
        {
            reconnects += 1;
            continue;
        }
        let ack = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
        let subscribed = ack
            .ok()
            .flatten()
            .and_then(Result::ok)
            .and_then(|m| m.to_text().ok().and_then(|t| serde_json::from_str::<Value>(t).ok()))
            .filter(|v| v["id"] == 1 && v.get("error").is_none() && v["jsonrpc"] == "2.0")
            .and_then(|v| v["result"].as_str().filter(|s| !s.is_empty() && s.len() <= 256).map(str::to_owned));
        let Some(sub) = subscribed else {
            // Same reasoning as a failed connect: fall back to polling rather than failing.
            reconnects += 1;
            if reconnects > 5 {
                health.send_replace(Health::Fault(
                    "eth_subscribe newHeads rejected and catch-up unavailable".into(),
                ));
                return;
            }
            if poll_until(c, timestamp_gte, tx, &mut seen, cancel, health, reconnects).await {
                return;
            }
            continue;
        };
        let chain = rpc(c, "eth_chainId", json!([])).await;
        let same_chain = chain.ok().is_some_and(|chain| {
            let got = chain
                .as_str()
                .ok_or_else(|| "chainId missing".to_owned())
                .and_then(crate::uint::uint256);
            let want = crate::uint::uint256(&c.chain_id);
            matches!((got, want), (Ok(a), Ok(b)) if a == b)
        });
        if !same_chain {
            health.send_replace(Health::Fault("RPC chainId changed".into()));
            return;
        }
        reconnects = 0;
        health.send_replace(Health::Ready);
        let mut tick = tokio::time::interval(Duration::from_millis(400));
        loop {
            tokio::select! {
                biased;
                _ = cancel.changed() => {
                    let _ = ws.close(None).await;
                    return;
                }
                msg = ws.next() => {
                    let Some(Ok(frame)) = msg else {
                        // Socket ended: reconnect, and let the poll cover the gap meanwhile.
                        break;
                    };
                    match frame {
                        tokio_tungstenite::tungstenite::Message::Ping(p) => {
                            let _ = ws.send(tokio_tungstenite::tungstenite::Message::Pong(p)).await;
                            continue;
                        }
                        tokio_tungstenite::tungstenite::Message::Pong(_) => continue,
                        tokio_tungstenite::tungstenite::Message::Text(text) => {
                            let Ok(v) = serde_json::from_str::<Value>(&text) else { break };
                            // Provider chatter and our own subscribe ack are not headers.
                            if v["jsonrpc"] != "2.0"
                                || v["method"] != "eth_subscription"
                                || v["params"]["subscription"] != sub.as_str()
                            {
                                continue;
                            }
                            let header = &v["params"]["result"];
                            if observe(c, header, timestamp_gte, "head_ws", tx, &mut seen)
                                .unwrap_or(false)
                            {
                                let _ = ws.close(None).await;
                                return;
                            }
                        }
                        _ => break,
                    }
                }
                _ = tick.tick() => {
                    if clock().map(|t| t >= c.deadline_ms).unwrap_or(true) {
                        health.send_replace(Health::Fault("native task deadline expired".into()));
                        return;
                    }
                    // A single failed poll is not a fault while the socket is alive: the next
                    // tick retries, and a dead socket is handled by another branch.
                    if let Ok(header) = latest_header(c).await {
                        if observe(c, &header, timestamp_gte, "head_http", tx, &mut seen)
                            .unwrap_or(false)
                        {
                            let _ = ws.close(None).await;
                            return;
                        }
                    }
                    let _ = ws
                        .send(tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into()))
                        .await;
                }
            }
        }
        reconnects += 1;
        if reconnects > 20 {
            health.send_replace(Health::Fault("WS reconnect exhausted".into()));
            return;
        }
        if poll_until(c, timestamp_gte, tx, &mut seen, cancel, health, reconnects).await {
            return;
        }
    }
}

/// HTTP-only wait between reconnect attempts. Returns true when the caller should stop, either
/// because a qualifying block was queued (task work continues in the runtime) or because the
/// source became unusable.
async fn poll_until(
    c: &Config,
    timestamp_gte: u64,
    tx: &tokio::sync::broadcast::Sender<Event>,
    seen: &mut RecentIds,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
    health: &tokio::sync::watch::Sender<Health>,
    attempt: u32,
) -> bool {
    let window = Duration::from_millis(400 * u64::from(attempt).max(1));
    let until = Instant::now() + window;
    while Instant::now() < until {
        if *cancel.borrow() {
            return true;
        }
        match latest_header(c).await {
            Ok(header) => {
                health.send_replace(Health::Ready);
                if observe(c, &header, timestamp_gte, "head_http", tx, seen).unwrap_or(false) {
                    return true;
                }
            }
            Err(error) => {
                health.send_replace(Health::Fault(error));
                return true;
            }
        }
        tokio::select! {
            _ = cancel.changed() => return true,
            _ = tokio::time::sleep(Duration::from_millis(400)) => {}
        }
    }
    false
}
