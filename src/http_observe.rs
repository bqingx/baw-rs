//! Privacy-preserving connector observations. No URLs, errors, or socket addresses escape.
//!
//! Counters describe activity in a shared pool between begin/finish, not attribution to
//! one request. Connector elapsed time aggregates DNS + TCP + TLS (including failures
//! and cancellations); it is neither isolated DNS/TLS timing nor server processing time.
use std::{
    collections::VecDeque,
    future::Future,
    net::SocketAddr,
    ops::Deref,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tower::{Layer, Service};

const SOCKET_HISTORY_LIMIT: usize = 64;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    let mut id = NEXT_ID.load(Ordering::Relaxed);
    loop {
        let next = id
            .checked_add(1)
            .expect("opaque transport id space exhausted");
        match NEXT_ID.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return id,
            Err(current) => id = current,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Counters {
    attempts: u64,
    successes: u64,
    failures: u64,
    cancellations: u64,
    elapsed: Duration,
}

#[derive(Default)]
struct State {
    counters: Counters,
    sockets: VecDeque<((SocketAddr, SocketAddr), u64)>,
}

impl State {
    fn socket(&mut self, local: SocketAddr, remote: SocketAddr) -> (u64, bool) {
        let tuple = (local, remote);
        if let Some(index) = self.sockets.iter().position(|(key, _)| *key == tuple) {
            let entry = self.sockets.remove(index).expect("socket history index");
            let id = entry.1;
            self.sockets.push_back(entry);
            return (id, true);
        }
        let id = next_id();
        if self.sockets.len() == SOCKET_HISTORY_LIMIT {
            self.sockets.pop_front();
        }
        self.sockets.push_back((tuple, id));
        (id, false)
    }
}

#[derive(Default)]
struct Metrics {
    state: Mutex<State>,
}

impl Metrics {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // Instrumentation must not introduce a poison-induced request failure.
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Clones retain the same reqwest connection pool and observer state.
#[derive(Clone)]
pub(crate) struct Transport {
    client: reqwest::Client,
    pool_id: u64,
    metrics: Arc<Metrics>,
}

impl Deref for Transport {
    type Target = reqwest::Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl Transport {
    pub(crate) fn build(builder: reqwest::ClientBuilder) -> Result<Self, String> {
        let metrics = Arc::new(Metrics::default());
        let client = builder
            .connector_layer(ObserveLayer(metrics.clone()))
            .build()
            .map_err(|_| "HTTP transport build failed".to_owned())?;
        Ok(Self {
            client,
            pool_id: next_id(),
            metrics,
        })
    }

    pub(crate) fn begin(&self) -> Probe {
        let state = self.metrics.lock();
        Probe {
            metrics: self.metrics.clone(),
            pool_id: self.pool_id,
            start: Instant::now(),
            before: state.counters,
        }
    }
}

pub(crate) struct Probe {
    metrics: Arc<Metrics>,
    pool_id: u64,
    start: Instant,
    before: Counters,
}

/// Socket IDs identify observed address tuples, not a guaranteed physical connection:
/// tuple recycling is possible. History eviction also removes prior-seen evidence.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct Observation {
    pub pool_id: u64,
    pub socket_id: Option<u64>,
    pub http_protocol: Option<&'static str>,
    pub socket_seen_before: Option<bool>,
    pub connect_attempts: u64,
    pub connect_successes: u64,
    pub connect_failures: u64,
    pub connect_cancellations: u64,
    /// Sum of connector durations completed in the interval, including cancellation.
    /// An attempt may have started before begin; pending attempts add no duration yet.
    pub connector_elapsed_ms: f64,
    pub observation_ms: f64,
    pub activity_scope: &'static str,
    /// Evidence only: concurrent requests can share the interval and socket history.
    pub reuse_evidence: &'static str,
}

impl Probe {
    pub(crate) fn finish(self, response: Option<&reqwest::Response>) -> Observation {
        let mut state = self.metrics.lock();
        let after = state.counters;
        let attempts = after.attempts - self.before.attempts;
        let socket = response
            .and_then(|response| {
                response
                    .extensions()
                    .get::<hyper_util::client::legacy::connect::HttpInfo>()
            })
            .map(|info| state.socket(info.local_addr(), info.remote_addr()));
        Observation {
            pool_id: self.pool_id,
            socket_id: socket.map(|(id, _)| id),
            http_protocol: response.map(|response| match response.version() {
                reqwest::Version::HTTP_09 => "http/0.9",
                reqwest::Version::HTTP_10 => "http/1.0",
                reqwest::Version::HTTP_11 => "http/1.1",
                reqwest::Version::HTTP_2 => "http/2",
                reqwest::Version::HTTP_3 => "http/3",
                _ => "unknown",
            }),
            socket_seen_before: socket.map(|(_, seen)| seen),
            connect_attempts: attempts,
            connect_successes: after.successes - self.before.successes,
            connect_failures: after.failures - self.before.failures,
            connect_cancellations: after.cancellations - self.before.cancellations,
            connector_elapsed_ms: (after.elapsed - self.before.elapsed).as_secs_f64() * 1000.0,
            observation_ms: self.start.elapsed().as_secs_f64() * 1000.0,
            activity_scope: "pool_interval_not_request_attributed",
            reuse_evidence: match socket {
                Some((_, true)) if attempts == 0 => "same_socket_no_connect_attempts",
                Some((_, true)) => "same_socket_with_connect_activity",
                Some((_, false)) => "first_socket_observation",
                None => "unavailable",
            },
        }
    }
}

#[derive(Clone)]
struct ObserveLayer(Arc<Metrics>);

impl<S> Layer<S> for ObserveLayer {
    type Service = ObserveService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ObserveService {
            inner,
            metrics: self.0.clone(),
        }
    }
}

#[derive(Clone)]
struct ObserveService<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

impl<S, R> Service<R> for ObserveService<S>
where
    S: Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let mut guard = ConnectGuard::new(self.metrics.clone());
        let future = self.inner.call(request);
        Box::pin(async move {
            let result = future.await;
            guard.complete(result.is_ok());
            result
        })
    }
}

struct ConnectGuard {
    metrics: Arc<Metrics>,
    start: Instant,
    completed: bool,
}

impl ConnectGuard {
    fn new(metrics: Arc<Metrics>) -> Self {
        let start = Instant::now();
        metrics.lock().counters.attempts += 1;
        Self {
            metrics,
            start,
            completed: false,
        }
    }

    fn complete(&mut self, success: bool) {
        let mut state = self.metrics.lock();
        if success {
            state.counters.successes += 1;
        } else {
            state.counters.failures += 1;
        }
        state.counters.elapsed += self.start.elapsed();
        self.completed = true;
    }
}

impl Drop for ConnectGuard {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = self.metrics.lock();
            state.counters.cancellations += 1;
            state.counters.elapsed += self.start.elapsed();
        }
    }
}

#[cfg(test)]
#[path = "http_observe_tests.rs"]
mod tests;
