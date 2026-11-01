//! Pending transactions are transaction inputs, never synthetic receipt logs.
use super::*;
use tokio_tungstenite::tungstenite::Message;

fn tx_hash(e: &Value) -> Result<&str, String> {
    e.get("hash")
        .and_then(Value::as_str)
        .filter(|s| hex(s, 32))
        .ok_or_else(|| "pending transaction hash missing/malformed".into())
}
/// The canonical static ABI word at `index`, or None when this calldata does not have that word:
/// dynamic arguments, shorter calldata and trailing partial words are ordinary non-matches.
fn static_word(words: &str, index: usize) -> Option<String> {
    if !(words.len() - 2).is_multiple_of(64) || index >= 256 {
        return None;
    }
    let start = 2 + index * 64;
    words.get(start..start + 64).map(|s| format!("0x{s}"))
}

/// None means an ordinary nonmatch or a transaction already mined by the time it was fetched.
/// Missing/contradictory protocol fields are errors, not permission to invent pending state.
pub(super) fn id(c: &Config, e: &Value) -> Result<Option<String>, String> {
    let TriggerSource::Pending {
        from,
        to,
        selector,
        args,
        ..
    } = &c.source
    else {
        return Err("pending transaction used with log source".into());
    };
    let hash = tx_hash(e)?;
    let sender = e
        .get("from")
        .and_then(Value::as_str)
        .filter(|s| hex(s, 20))
        .ok_or("pending sender missing/malformed")?;
    let target = match e.get("to") {
        Some(Value::Null) => None, // Contract creation is not a call to the pinned target.
        Some(Value::String(s)) if hex(s, 20) => Some(s),
        _ => return Err("pending target missing/malformed".into()),
    };
    let input = e
        .get("input")
        .and_then(Value::as_str)
        .filter(|s| data(s))
        .ok_or("pending input missing/malformed")?;
    if let Some(chain) = e.get("chainId") {
        let n = crate::uint::uint256(chain.as_str().ok_or("pending chainId malformed")?)?;
        if n != crate::uint::uint256(&c.chain_id)? {
            return Err("pending chainId mismatch".into());
        }
    }
    let mut mined = false;
    for key in ["blockHash", "blockNumber"] {
        match e.get(key) {
            Some(Value::Null) => {}
            Some(v) if key == "blockHash" && v.as_str().is_some_and(|s| hex(s, 32)) => mined = true,
            Some(v) if key == "blockNumber" && quantity(v).is_ok() => mined = true,
            _ => return Err(format!("pending {key} missing/malformed")),
        }
    }
    if let Some(index) = e.get("transactionIndex") {
        if !index.is_null() {
            quantity(index)?;
            mined = true;
        }
    }
    if mined
        || !sender.eq_ignore_ascii_case(from)
        || !target.is_some_and(|s| s.eq_ignore_ascii_case(to))
        || !input
            .get(..10)
            .is_some_and(|s| s.eq_ignore_ascii_case(selector))
    {
        return Ok(None);
    }
    // Only statically addressed ABI words are supported. Offsets/dynamic ABI interpretation are
    // not guessed. A configured argument is an exact typed word equality, not a log data mapping.
    // Shape is required only for the words actually read: mempool peers carry arbitrary and dynamic
    // calldata, so a missing or noncanonical relevant word is a non-match, never a task-fatal
    // protocol error that would abort a watcher authorized to wait for hours.
    let words = format!("0x{}", &input[10..]);
    for arg in args {
        match static_word(&words, arg.index) {
            Some(w) if w.eq_ignore_ascii_case(&arg.equals) => {}
            _ => return Ok(None),
        }
    }
    let mut h = Sha256::new();
    for s in [&c.task_id, &c.chain_id, hash] {
        h.update(s.to_ascii_lowercase().as_bytes());
        h.update([0]);
    }
    Ok(Some(format!(
        "pnd-{}",
        &format!("{:x}", h.finalize())[..60]
    )))
}

async fn response<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    expected_id: u64,
) -> Result<Value, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
        .await
        .map_err(|_| "pending WS RPC timeout")?
        .ok_or("pending WS closed during RPC")?
        .map_err(|_| "pending WS RPC transport failed")?;
    let v: Value = serde_json::from_str(msg.to_text().map_err(|_| "pending WS RPC not text")?)
        .map_err(|_| "pending WS RPC malformed JSON")?;
    if v["jsonrpc"] != "2.0" || v["id"] != expected_id || v.get("error").is_some() {
        return Err("pending WS RPC rejected/malformed; subscription unsupported or wrong chain; no fallback".into());
    }
    v.get("result")
        .cloned()
        .ok_or("pending WS RPC result missing".into())
}

pub(super) async fn run(
    wallet: &Wallet,
    c: &Config,
    events: &tokio::sync::mpsc::Sender<Event>,
    health: &tokio::sync::watch::Sender<Health>,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let TriggerSource::Pending { subscription, .. } = &c.source else {
        unreachable!()
    };
    wallet
        .native_invalid
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let session = async {
        let (mut ws, _) = tokio::time::timeout(
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
        .await
        .map_err(|_| "pending WS connect timeout")?
        .map_err(|_| "pending WS connect failed; no log fallback")?;
        // Pending visibility cannot be recovered by log backfill, so both transports must identify
        // the authorized chain before READY. Unsupported WS RPC is a hard failure, not a guess.
        ws.send(Message::Text(
            json!({"jsonrpc":"2.0","id":2,"method":"eth_chainId","params":[]})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| "pending WS chain request failed")?;
        let chain = response(&mut ws, 2).await?;
        if crate::uint::uint256(chain.as_str().ok_or("pending WS chainId missing")?)?
            != crate::uint::uint256(&c.chain_id)?
        {
            return Err("pending WS chainId mismatch".into());
        }
        let params = match subscription {
            PendingSubscription::Hashes => json!(["newPendingTransactions"]),
            PendingSubscription::Full => json!(["newPendingTransactions", true]),
        };
        ws.send(Message::Text(
            json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":params})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| "pending WS subscribe send failed")?;
        let sub = response(&mut ws, 1)
            .await?
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .ok_or("pending subscription id missing")?
            .to_owned();
        let chain = rpc(c, "eth_chainId", json!([])).await?;
        if crate::uint::uint256(chain.as_str().ok_or("pending HTTP chainId missing")?)?
            != crate::uint::uint256(&c.chain_id)?
        {
            return Err("pending HTTP chainId mismatch".into());
        }
        wallet
            .native_invalid
            .store(false, std::sync::atomic::Ordering::SeqCst);
        health.send_replace(Health::Ready);
        let (queue, mut incoming) = tokio::sync::mpsc::channel::<Event>(c.queue_capacity);
        // Poll ingress even while an HTTP fetch is outstanding. Do not hide overload behind TCP
        // buffering, drop hashes silently, or spawn an unbounded HTTP request per hash.
        let reader = async {
            let mut seen = RecentIds::default();
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            let mut last = Instant::now();
            loop {
                tokio::select! {
                    biased;
                    msg = ws.next() => {
                        let msg = msg.ok_or("pending WS closed; visibility gap; no replay/fallback")?
                            .map_err(|_| "pending WS transport failed; no replay/fallback")?;
                        last = Instant::now();
                        let text = match msg {
                            Message::Text(t) => t,
                            Message::Ping(p) => {ws.send(Message::Pong(p)).await.map_err(|_| "pending WS pong failed")?; continue;},
                            Message::Pong(_) => continue,
                            _ => return Err::<(), String>("pending WS closed/unsupported frame; no replay/fallback".into()),
                        };
                        let received = Instant::now();
                        let v: Value = serde_json::from_str(&text).map_err(|_| "pending WS malformed notification")?;
                        if v["jsonrpc"] != "2.0" || v["method"] != "eth_subscription" || v["params"]["subscription"] != sub {
                            return Err("pending WS unexpected subscription notification".into());
                        }
                        let value = v["params"].get("result").ok_or("pending WS result missing")?.clone();
                        let hash = match subscription {
                            PendingSubscription::Hashes => value.as_str().filter(|s| hex(s, 32))
                                .ok_or("pending hash notification malformed or wrong subscription mode")?,
                            PendingSubscription::Full => {id(c, &value)?; tx_hash(&value)?},
                        }.to_ascii_lowercase();
                        if !seen.observe(hash) {continue;}
                        queue.try_send(Event{value, received, origin:"pending_ws"})
                            .map_err(|_| "pending ingress queue overflow/closed; not READY")?;
                    },
                    _ = tick.tick() => {
                        if last.elapsed() > Duration::from_secs(30) {return Err("pending WS heartbeat timeout; visibility gap".into());}
                        ws.send(Message::Ping(Vec::new().into())).await.map_err(|_| "pending WS ping failed")?;
                    }
                }
            }
        };
        let consumer = async {
            let mut seen = RecentIds::default();
            while let Some(mut event) = incoming.recv().await {
                if matches!(subscription, PendingSubscription::Hashes) {
                    let hash = event
                        .value
                        .as_str()
                        .ok_or("pending hash missing")?
                        .to_owned();
                    event.value = rpc(c, "eth_getTransactionByHash", json!([hash])).await?;
                    // Different HTTP/WS nodes may not share a mempool. This is a missed observation,
                    // not evidence of a transaction, and is deliberately not retried automatically.
                    if event.value.is_null() {
                        continue;
                    }
                    if !tx_hash(&event.value)?.eq_ignore_ascii_case(&hash) {
                        return Err::<(), String>(
                            "pending hash lookup returned different transaction".into(),
                        );
                    }
                    event.origin = "pending_hash_rpc";
                }
                if let Some(id) = id(c, &event.value)? {
                    if !seen.observe(id) {
                        continue;
                    }
                    events
                        .try_send(event)
                        .map_err(|_| "pending event queue overflow/closed; not READY")?;
                }
            }
            Err("pending ingress stopped".into())
        };
        tokio::select! { biased; r = reader => r, r = consumer => r }
    };
    let deadline = c
        .monotonic_deadline
        .map(tokio::time::Instant::from_std)
        .ok_or("pending task has no activated duration")?;
    tokio::select! {
        biased;
        _ = cancel.changed() => Ok(()),
        _ = tokio::time::sleep_until(deadline) => Err("pending task deadline expired".into()),
        result = session => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pending_recent_hash_cache_evicts_without_limiting_wait_lifetime() {
        let mut cache = RecentIds::default();
        for n in 0..5000 {
            assert!(
                cache.observe(format!("0x{n:064x}")),
                "unrelated history prevented processing hash {n}"
            );
            assert!(cache.seen.len() <= 4096 && cache.order.len() <= 4096);
        }
        assert!(!cache.observe(format!("0x{:064x}", 4999)));
        assert!(
            cache.observe(format!("0x{:064x}", 0)),
            "evicted hash was not admissible again"
        );
        let c = Config::parse(super::super::super::native_events_tests::pending_config(
            "full",
        ))
        .unwrap();
        let tx = super::super::super::native_events_tests::pending_transaction(0xbb);
        assert!(cache.observe(tx["hash"].as_str().unwrap().to_owned()));
        assert!(
            id(&c, &tx).unwrap().is_some(),
            "target could no longer trigger after unrelated history"
        );
    }
    #[test]
    fn pending_calldata_shape_is_required_only_for_configured_words() {
        let nested = super::super::super::native_events_tests::pending_config("full");
        let mut selector_only = nested.clone();
        selector_only["source"]["args"] = json!([]);
        let sel = Config::parse(selector_only).unwrap();
        // Dynamic calldata with a trailing partial word still carries the pinned selector.
        let mut tx = super::super::super::native_events_tests::pending_transaction(0xcc);
        tx["input"] = json!(format!("0xabcdef01{:064x}{:064x}deadbeef", 0x20u64, 4u64));
        assert!(
            id(&sel, &tx).unwrap().is_some(),
            "selector-only trigger stopped matching dynamic calldata"
        );
        // With a pinned word, malformed/absent relevant words are a non-match, not a fatal error.
        let pinned = Config::parse(nested).unwrap();
        assert!(
            id(&pinned, &tx).unwrap().is_none(),
            "malformed pinned word must be a non-match"
        );
        tx["input"] = json!("0xabcdef01");
        assert!(
            id(&pinned, &tx).unwrap().is_none(),
            "absent pinned word must be a non-match"
        );
        // A canonical matching word is still an exact match.
        assert!(id(
            &pinned,
            &super::super::super::native_events_tests::pending_transaction(0xdd)
        )
        .unwrap()
        .is_some());
    }
}
