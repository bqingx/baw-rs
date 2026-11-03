//! One listener per identical task source, shared by every wallet that asked for it.
//!
//! Six wallets watching the same chain condition used to open six WS connections and issue six
//! independent `eth_getBlockByNumber` polls per tick — the RPC budget scaled with wallet count and
//! each wallet could see a different block first. A feed is keyed by the *source fingerprint*
//! (chain, endpoints, source parameters), so identical tasks share one connection, one poll loop and
//! one copy of the dedup window; each subscriber gets its own health watch and can join or leave
//! without disturbing the others.
//!
//! Keying is deliberately exact: tasks whose source differs in any parameter (start block,
//! confirmations, filter, threshold) do not share a feed, because merging them would change what
//! each task was authorized to watch.
use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(in crate::resident) struct Feeds {
    inner: Mutex<HashMap<String, Shared>>,
}

struct Shared {
    events: tokio::sync::broadcast::Sender<Event>,
    health: tokio::sync::watch::Sender<Health>,
    cancel: tokio::sync::watch::Sender<bool>,
    subscribers: usize,
    task: tokio::task::JoinHandle<()>,
}

/// Dropping the last subscriber stops the listener. A subscriber that exits early (disarm, fault,
/// deadline) must not keep the socket alive for the others.
pub(in crate::resident) struct Subscription {
    events: tokio::sync::broadcast::Receiver<Event>,
    health: tokio::sync::watch::Receiver<Health>,
    key: String,
    feeds: Arc<Feeds>,
}

impl Subscription {
    pub(super) fn health(&mut self) -> tokio::sync::watch::Receiver<Health> {
        self.health.clone()
    }
    /// Move the event receiver out; the subscription itself must outlive the reader so the
    /// refcount, not the loop, decides when the listener may stop.
    pub(super) fn clone_events(&mut self) -> tokio::sync::broadcast::Receiver<Event> {
        self.events.resubscribe()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.feeds.inner.lock() {
            if let Some(shared) = inner.get_mut(&self.key) {
                shared.subscribers = shared.subscribers.saturating_sub(1);
                if shared.subscribers == 0 {
                    shared.cancel.send_replace(true);
                    if let Some(shared) = inner.remove(&self.key) {
                        shared.task.abort();
                    }
                }
            }
        }
    }
}

/// Stable identity of a source. Two tasks share a listener only when every field matches.
fn key(c: &Config) -> Result<String, String> {
    let mut h = Sha256::new();
    let fingerprint = json!({
        "chain": c.chain_id,
        "rpc": c.rpc_url,
        "ws": c.ws_url,
        "source": c.source,
        "filter": c.filter,
        "start_block": c.start_block,
        "confirmation_depth": c.confirmation_depth,
        "max_backfill_blocks": c.max_backfill_blocks,
        "chunk_blocks": c.chunk_blocks,
    });
    h.update(fingerprint.to_string().as_bytes());
    Ok(format!("{:x}", h.finalize()))
}

impl Feeds {
    pub(in crate::resident) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// Join the feed for this config, starting its listener if this is the first subscriber.
    pub(in crate::resident) fn subscribe(
        self: &Arc<Self>,
        c: &Config,
    ) -> Result<Subscription, String> {
        let key = key(c)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "event feed registry unavailable")?;
        if let Some(shared) = inner.get_mut(&key) {
            shared.subscribers += 1;
            return Ok(Subscription {
                events: shared.events.subscribe(),
                health: shared.health.subscribe(),
                key,
                feeds: self.clone(),
            });
        }
        // Depth is per subscriber: a burst of blocks must not be lost by a wallet that is busy
        // previewing, and the lag is reported rather than reinterpreted as a trigger.
        let (events, _) = tokio::sync::broadcast::channel(64);
        let (health, _) = tokio::sync::watch::channel(Health::Connecting);
        let (cancel, cancel_rx) = tokio::sync::watch::channel(false);
        let invalid = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let task = tokio::spawn(source(
            c.clone(),
            events.clone(),
            health.clone(),
            cancel_rx,
            invalid,
        ));
        inner.insert(
            key.clone(),
            Shared {
                events: events.clone(),
                health: health.clone(),
                cancel,
                subscribers: 1,
                task,
            },
        );
        Ok(Subscription {
            events: events.subscribe(),
            health: health.subscribe(),
            key,
            feeds: self.clone(),
        })
    }

    #[cfg(test)]
    pub(in crate::resident) fn len(&self) -> usize {
        self.inner.lock().map(|m| m.len()).unwrap_or(0)
    }
}
