use super::tests::{credentials, mock, response};
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn scoped(origin: &str, ttl: Duration) -> Client {
    let mut client = Client::test_client(credentials(), origin).unwrap();
    client.policy_snapshot = Some(policy_snapshot::Snapshot::new(ttl));
    client
}
fn finish(
    requests: std::sync::mpsc::Receiver<String>,
    handle: std::thread::JoinHandle<()>,
    count: usize,
) -> Vec<String> {
    let rows = (0..count)
        .map(|_| requests.recv_timeout(Duration::from_secs(2)).unwrap())
        .collect();
    handle.join().unwrap();
    rows
}
#[tokio::test]
async fn settings_and_limit_reads_share_a_bounded_command_snapshot() {
    let (origin, requests, server) = mock(vec![
        response(json!({"v":1}), None),
        response(json!({"v":2}), None),
        response(json!({"v":3}), None),
    ]);
    let mut c = scoped(&origin, Duration::from_secs(3));
    assert_eq!(c.cached_settings().await.unwrap()["v"], 1);
    assert_eq!(
        c.request(Method::GET, &format!("{API}/settings/query"), None)
            .await
            .unwrap()["v"],
        1
    );
    for _ in 0..2 {
        assert_eq!(
            c.request(Method::GET, &format!("{API}/trading-limit/query"), None)
                .await
                .unwrap()["v"],
            2
        );
    }
    let stats = c.end_policy_snapshot().unwrap();
    assert_eq!(stats["binanceRequests"], 2);
    assert_eq!(stats["policyReads"], 2);
    assert_eq!(stats["cacheHits"], 2);
    assert_eq!(
        c.request(Method::GET, &format!("{API}/settings/query"), None)
            .await
            .unwrap()["v"],
        3
    );
    finish(requests, server, 3);
}
#[tokio::test]
async fn expired_and_zero_ttl_snapshots_do_not_serve_stale_values() {
    for ttl in [Duration::ZERO, Duration::from_millis(10)] {
        let (origin, requests, server) =
            mock(vec![response(json!(1), None), response(json!(2), None)]);
        let mut c = scoped(&origin, ttl);
        assert_eq!(c.cached_settings().await.unwrap(), 1);
        if !ttl.is_zero() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(c.cached_settings().await.unwrap(), 2);
        assert_eq!(c.end_policy_snapshot().unwrap()["cacheHits"], 0);
        finish(requests, server, 2);
    }
}
#[tokio::test]
async fn failed_reads_are_never_cached_or_implicitly_retried() {
    let (origin, requests, server) = mock(vec![
        "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        response(json!(2), None),
    ]);
    let mut c = scoped(&origin, Duration::from_secs(3));
    assert!(c.cached_settings().await.is_err());
    assert_eq!(c.cached_settings().await.unwrap(), 2);
    assert_eq!(c.cached_settings().await.unwrap(), 2);
    assert_eq!(c.end_policy_snapshot().unwrap()["binanceRequests"], 2);
    finish(requests, server, 2);
}
#[tokio::test]
async fn execute_posts_are_not_part_of_the_read_set() {
    let (origin, requests, server) = mock(vec![response(json!(1), None), response(json!(2), None)]);
    let mut c = scoped(&origin, Duration::from_secs(3));
    for expected in [1, 2] {
        assert_eq!(
            c.request(
                Method::POST,
                &format!("{API}/contract-call/execute"),
                Some(json!({"requestId":"synthetic"}))
            )
            .await
            .unwrap(),
            expected
        );
    }
    let stats = c.end_policy_snapshot().unwrap();
    assert_eq!(stats["policyReads"], 0);
    assert_eq!(stats["cacheHits"], 0);
    finish(requests, server, 2);
}
#[tokio::test]
async fn rotated_cookie_invalidates_preceding_policy_values() {
    let (origin, requests, server) = mock(vec![
        response(json!(1), None),
        response(json!(2), Some("synthetic-rotated")),
        response(json!(3), None),
    ]);
    let mut c = scoped(&origin, Duration::from_secs(3));
    assert_eq!(c.cached_settings().await.unwrap(), 1);
    c.request(Method::GET, &format!("{API}/trading-limit/query"), None)
        .await
        .unwrap();
    assert_eq!(c.cached_settings().await.unwrap(), 3);
    let rows = finish(requests, server, 3);
    assert!(rows[2].contains("agentSessionId=synthetic-rotated"));
}
#[tokio::test]
async fn cache_hits_still_enforce_cancellation_and_binding_gates() {
    let (origin, requests, server) = mock(vec![response(json!(1), None)]);
    let mut c = scoped(&origin, Duration::from_secs(3));
    c.cached_settings().await.unwrap();
    let cancel = Arc::new(AtomicBool::new(true));
    assert!(
        crate::execution::task_scope(u64::MAX, cancel.clone(), c.cached_settings())
            .await
            .is_err()
    );
    cancel.store(false, Ordering::SeqCst);
    let check = Arc::new(|| Err("changed identity".into()));
    assert!(crate::execution::binding_scope(check, c.cached_settings())
        .await
        .is_err());
    assert_eq!(c.end_policy_snapshot().unwrap()["cacheHits"], 0);
    finish(requests, server, 1);
}
#[tokio::test]
async fn snapshots_do_not_cross_wallets_or_explicit_session_replacements() {
    let (origin, requests, server) = mock(vec![
        response(json!(1), None),
        response(json!(2), None),
        response(json!(3), None),
    ]);
    let mut a = scoped(&origin, Duration::from_secs(3));
    let mut b = scoped(&origin, Duration::from_secs(3));
    assert_eq!(a.cached_settings().await.unwrap(), 1);
    assert_eq!(b.cached_settings().await.unwrap(), 2);
    a.set_session(zeroize::Zeroizing::new("synthetic-replaced".into()))
        .unwrap();
    assert_eq!(a.cached_settings().await.unwrap(), 3);
    finish(requests, server, 3);
}
#[tokio::test]
async fn prepared_policy_blocks_auxiliary_reads_even_when_their_values_are_cached() {
    let mut c = scoped("http://127.0.0.1:9", Duration::from_secs(3));
    c.settings_cache = Some((
        std::time::Instant::now(),
        json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
    ));
    c.addresses_cache = Some((std::time::Instant::now(), json!({"addresses":[]})));
    c.begin_policy_snapshot();
    let policy = crate::execution::PreparedWalletPolicy::capture(
        &c,
        "56",
        "0x1111111111111111111111111111111111111111",
        4102444800,
    )
    .unwrap();
    crate::execution::binding_scope(
        Arc::new(|| Ok(())),
        crate::execution::task_scope(
            u64::MAX,
            Arc::new(AtomicBool::new(false)),
            crate::execution::prepared_wallet_scope(policy, async {
                assert!(c
                    .cached_settings()
                    .await
                    .err()
                    .unwrap()
                    .contains("auxiliary"));
                assert!(c
                    .cached_addresses()
                    .await
                    .err()
                    .unwrap()
                    .contains("auxiliary"));
                let error = c
                    .request(Method::GET, &format!("{API}/settings/query"), None)
                    .await
                    .err()
                    .unwrap();
                assert!(error.contains("permits only preview/execute"));
            }),
        ),
    )
    .await;
    let stats = c.end_policy_snapshot().unwrap();
    assert_eq!(stats["binanceRequests"], 0);
    assert_eq!(stats["cacheHits"], 0);
}

#[test]
fn only_known_read_only_routes_can_enter_the_snapshot() {
    use policy_snapshot::Snapshot;
    assert!(Snapshot::key(
        &Method::POST,
        &format!("{API}/order/tx-lock-status"),
        Some(&json!({"binanceChainId":"56"}))
    )
    .is_some());
    for (method, path, body) in [
        (
            Method::POST,
            format!("{API}/contract-call/execute"),
            Some(json!({"requestId":"x"})),
        ),
        (Method::POST, format!("{API}/settings/query"), None),
        (Method::GET, format!("{API}/settings/query?other=1"), None),
        (
            Method::POST,
            format!("{API}/order/tx-lock-status"),
            Some(json!({"binanceChainId":"56","extra":true})),
        ),
    ] {
        assert!(Snapshot::key(&method, &path, body.as_ref()).is_none());
    }
}
