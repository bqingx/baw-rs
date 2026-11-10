use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

// One byte at a time avoids consuming a following pipelined request. All I/O is
// loopback only, and both clients and test waits have explicit deadlines.
async fn server(close: bool) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private?token=secret",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                loop {
                    let mut headers = Vec::new();
                    while !headers.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        if stream.read_exact(&mut byte).await.is_err() {
                            return;
                        }
                        headers.push(byte[0]);
                        assert!(headers.len() < 8192);
                    }
                    let response = if close {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"
                    };
                    if stream.write_all(response.as_bytes()).await.is_err() || close {
                        return;
                    }
                }
            });
        }
    });
    (url, task)
}

fn transport() -> Transport {
    Transport::build(
        reqwest::Client::builder()
            .no_proxy()
            .http1_only()
            .timeout(Duration::from_secs(3)),
    )
    .unwrap()
}

async fn request(client: &Transport, url: &str) -> Observation {
    let probe = client.begin();
    let response = client.get(url).send().await.unwrap();
    let observation = probe.finish(Some(&response));
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ok");
    observation
}

#[tokio::test]
async fn sequential_keepalive_has_same_socket_without_new_connect() {
    let (url, server) = server(false).await;
    let client = transport();
    let first = request(&client, &url).await;
    let second = request(&client.clone(), &url).await;
    assert_eq!(first.http_protocol, Some("http/1.1"));
    assert_eq!(first.connect_attempts, 1);
    assert_eq!(first.connect_successes, 1);
    assert_eq!(first.socket_seen_before, Some(false));
    assert!(first.socket_id.is_some());
    assert_eq!(first.socket_id, second.socket_id);
    assert_eq!(first.pool_id, second.pool_id);
    assert_eq!(second.connect_attempts, 0);
    assert_eq!(second.connect_successes, 0);
    assert_eq!(second.connector_elapsed_ms, 0.0);
    assert_eq!(second.socket_seen_before, Some(true));
    assert_eq!(second.reuse_evidence, "same_socket_no_connect_attempts");
    server.abort();
}

#[tokio::test]
async fn forced_close_opens_a_new_connection() {
    let (url, server) = server(true).await;
    let client = transport();
    let first = request(&client, &url).await;
    let second = request(&client, &url).await;
    assert_ne!(first.socket_id, second.socket_id);
    assert_eq!(second.connect_attempts, 1);
    assert_eq!(second.connect_successes, 1);
    assert_eq!(second.socket_seen_before, Some(false));
    server.abort();
}

#[tokio::test]
async fn different_pools_have_independent_metrics_and_ids() {
    let (url, server) = server(false).await;
    let a = transport();
    let b = transport();
    let idle_probe = b.begin();
    let first = request(&a, &url).await;
    let idle = idle_probe.finish(None);
    assert_eq!(idle.connect_attempts, 0);
    assert_eq!(idle.socket_id, None);
    let second = request(&b, &url).await;
    assert_ne!(first.pool_id, second.pool_id);
    assert_ne!(first.socket_id, second.socket_id);
    assert_eq!(second.connect_attempts, 1);
    server.abort();
}

#[tokio::test]
async fn failed_connection_and_build_errors_are_sanitized() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = transport();
    let probe = client.begin();
    let url = format!("http://{address}/private?token=secret");
    assert!(client
        .get(&url)
        .header("x-secret", "header-secret")
        .send()
        .await
        .is_err());
    let observation = probe.finish(None);
    assert_eq!(observation.connect_attempts, 1);
    assert_eq!(observation.connect_failures, 1);
    assert_eq!(observation.connect_successes, 0);
    assert_eq!(observation.socket_id, None);
    assert_eq!(observation.http_protocol, None);
    assert_eq!(observation.socket_seen_before, None);
    assert_eq!(observation.reuse_evidence, "unavailable");
    let text = format!(
        "{} {observation:?}",
        serde_json::to_string(&observation).unwrap()
    );
    for secret in [
        "127.0.0.1",
        &address.to_string(),
        "private",
        "token",
        "secret",
        "http://",
    ] {
        assert!(!text.contains(secret), "leaked sensitive data");
    }
    let result = Transport::build(reqwest::Client::builder().user_agent("secret\ninvalid"));
    assert_eq!(result.err().unwrap(), "HTTP transport build failed");
}

#[test]
fn socket_history_is_bounded_and_evicted_ids_are_not_reused() {
    let mut state = State::default();
    let remote = "127.0.0.1:80".parse().unwrap();
    let local = "127.0.0.1:1000".parse().unwrap();
    let (first_id, seen) = state.socket(local, remote);
    assert!(!seen);
    assert_eq!(state.socket(local, remote), (first_id, true));
    for port in 1001..=1064 {
        state.socket(SocketAddr::from(([127, 0, 0, 1], port)), remote);
        assert!(state.sockets.len() <= SOCKET_HISTORY_LIMIT);
    }
    let (new_id, seen) = state.socket(local, remote);
    assert!(!seen);
    assert_ne!(first_id, new_id);
    assert_eq!(state.sockets.len(), SOCKET_HISTORY_LIMIT);
}

#[tokio::test]
async fn overlapping_probe_reports_pool_activity_not_request_attribution() {
    let (url, server) = server(false).await;
    let client = transport();
    let overlapping = client.begin();
    let first = request(&client, &url).await;
    let probe = client.begin();
    let response = client.get(&url).send().await.unwrap();
    let second = probe.finish(Some(&response));
    let overlap = overlapping.finish(Some(&response));
    assert_eq!(first.socket_id, second.socket_id);
    assert_eq!(overlap.connect_attempts, 1);
    assert_eq!(second.connect_attempts, 0);
    assert_eq!(overlap.reuse_evidence, "same_socket_with_connect_activity");
    assert_eq!(
        overlap.activity_scope,
        "pool_interval_not_request_attributed"
    );
    assert_eq!(second.activity_scope, overlap.activity_scope);
    response.bytes().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn concurrent_requests_share_pool_interval_counters() {
    let (url, server) = server(false).await;
    let client = transport();
    let a = client.begin();
    let b = client.begin();
    let (a_response, b_response) = tokio::join!(client.get(&url).send(), client.get(&url).send());
    let a_response = a_response.unwrap();
    let b_response = b_response.unwrap();
    let a = a.finish(Some(&a_response));
    let b = b.finish(Some(&b_response));
    assert_eq!(a.pool_id, b.pool_id);
    assert!(a.connect_attempts >= 1);
    assert_eq!(a.connect_attempts, b.connect_attempts);
    assert_eq!(a.connect_successes, b.connect_successes);
    assert_eq!(a.activity_scope, "pool_interval_not_request_attributed");
    assert_eq!(b.activity_scope, a.activity_scope);
    assert_ne!(a.reuse_evidence, "same_socket_no_connect_attempts");
    assert_ne!(b.reuse_evidence, "same_socket_no_connect_attempts");
    a_response.bytes().await.unwrap();
    b_response.bytes().await.unwrap();
    server.abort();
}

#[derive(Clone)]
struct PendingService {
    ready: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}

impl Service<()> for PendingService {
    type Response = u8;
    type Error = &'static str;
    type Future = std::future::Pending<Result<u8, &'static str>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.ready.fetch_add(1, Ordering::Relaxed);
        Poll::Ready(Err("unchanged readiness error"))
    }

    fn call(&mut self, _: ()) -> Self::Future {
        self.calls.fetch_add(1, Ordering::Relaxed);
        std::future::pending()
    }
}

#[test]
fn cancellation_even_before_poll_is_counted_and_readiness_is_delegated() {
    let metrics = Arc::new(Metrics::default());
    let inner = PendingService {
        ready: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
    };
    let mut service = ObserveLayer(metrics.clone()).layer(inner.clone());
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        service.poll_ready(&mut cx),
        Poll::Ready(Err("unchanged readiness error"))
    );
    assert_eq!(inner.ready.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.lock().counters.attempts, 0);
    let future = service.call(());
    assert_eq!(inner.calls.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.lock().counters.attempts, 1);
    drop(future);
    let state = metrics.lock();
    assert_eq!(state.counters.cancellations, 1);
    assert_eq!(state.counters.successes, 0);
    assert_eq!(state.counters.failures, 0);
    assert!(state.counters.elapsed > Duration::ZERO);
}

#[tokio::test]
async fn generic_service_preserves_response_and_error_without_retry() {
    #[derive(Clone)]
    struct Echo;
    impl Service<Result<u8, &'static str>> for Echo {
        type Response = u8;
        type Error = &'static str;
        type Future = std::future::Ready<Result<u8, &'static str>>;
        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, request: Result<u8, &'static str>) -> Self::Future {
            std::future::ready(request)
        }
    }
    let metrics = Arc::new(Metrics::default());
    let mut service = ObserveLayer(metrics.clone()).layer(Echo);
    assert_eq!(service.call(Ok(42)).await, Ok(42));
    assert_eq!(service.call(Err("secret error")).await, Err("secret error"));
    let state = metrics.lock();
    assert_eq!(state.counters.attempts, 2);
    assert_eq!(state.counters.successes, 1);
    assert_eq!(state.counters.failures, 1);
    assert_eq!(state.counters.cancellations, 0);
}
