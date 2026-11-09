//! Bounded command-local read set, not a server-side atomic snapshot.
use super::*;
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    Settings,
    Limits,
    Addresses,
    Networks,
    Lock(String),
}

pub(super) struct Snapshot {
    pub(super) ttl: Duration,
    entries: BTreeMap<Key, (Instant, Value)>,
    network: u64,
    policy_reads: u64,
    hits: u64,
}
impl Snapshot {
    pub(super) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: BTreeMap::new(),
            network: 0,
            policy_reads: 0,
            hits: 0,
        }
    }
    pub(super) fn key(method: &Method, path: &str, body: Option<&Value>) -> Option<Key> {
        match (method.as_str(), path.strip_prefix(API), body) {
            ("GET", Some("/settings/query"), None) => Some(Key::Settings),
            ("GET", Some("/trading-limit/query"), None) => Some(Key::Limits),
            ("POST", Some("/mpc-wallet/list"), None) => Some(Key::Addresses),
            ("POST", Some("/order/tx-lock-status"), Some(body)) => {
                let object = body.as_object()?;
                if object.len() != 1 {
                    return None;
                }
                Some(Key::Lock(
                    object.get("binanceChainId")?.as_str()?.to_owned(),
                ))
            }
            ("GET", None, None)
                if path == "/bapi/defi/v1/public/wallet-direct/mgmt/agent/networks/active" =>
            {
                Some(Key::Networks)
            }
            _ => None,
        }
    }
    pub(super) fn get(&mut self, key: &Key) -> Option<Value> {
        if self.ttl.is_zero() {
            return None;
        }
        let (at, value) = self.entries.get(key)?;
        if at.elapsed() >= self.ttl {
            self.entries.remove(key);
            return None;
        }
        Some(value.clone())
    }
    pub(super) fn put(&mut self, key: Key, at: Instant, value: Value) {
        if !self.ttl.is_zero() && self.entries.len() < 8 && value.to_string().len() <= 65536 {
            self.entries.insert(key, (at, value));
        }
    }
    pub(super) fn hit(&mut self) {
        self.hits += 1;
    }
    pub(super) fn sent(&mut self, policy: bool) {
        self.network += 1;
        self.policy_reads += u64::from(policy);
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
    pub(super) fn stats(&self) -> Value {
        json!({"binanceRequests":self.network,"policyReads":self.policy_reads,
            "cacheHits":self.hits,"maxAgeMs":self.ttl.as_millis()})
    }
}
pub(super) fn ttl() -> Duration {
    let ms = match std::env::var("BAW_RS_CHECK_TTL_MS") {
        Ok(raw) => raw.parse::<u64>().unwrap_or(0),
        Err(_) => 3000,
    };
    Duration::from_millis(ms.min(3000))
}
