use super::*;
use crate::task::timing::{self as task_timing, ActionTrace, TriggerObservation};
fn trace() -> Arc<ActionTrace> {
    ActionTrace::new(
        TriggerObservation::simple("immediate"),
        std::time::Instant::now(),
        std::time::Instant::now(),
    )
}
async fn entered(trace: &ActionTrace) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while trace.snapshot()["workerToWalletQueueMs"].is_null() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn held_wallet_lock_is_observed_before_dispatch_in_cold_and_warm_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let wallet = wallet(
        root.path(),
        "timed",
        "instance-timed",
        A,
        Some("synthetic-cookie"),
        &mock.origin,
    );
    // The first request reloads the synthetic session; the second uses the resident client.
    for _ in 0..2 {
        let held = wallet.state.lock().await;
        let trace = trace();
        let worker_trace = trace.clone();
        let target = wallet.clone();
        let worker = tokio::spawn(task_timing::scope(worker_trace.clone(), async move {
            worker_trace.worker_started();
            target.execute(argv(&["wallet", "status"])).await
        }));
        entered(&trace).await;
        let pending = trace.snapshot();
        assert!(pending["walletQueueWaitMs"].is_null());
        assert!(pending["acceptedToDispatchMs"].is_null());
        assert!(
            mock.requests.try_recv().is_err(),
            "HTTP ran before wallet lock admission"
        );
        drop(held);
        let (request, reply) = mock.next().await;
        assert!(request.contains("login/query"));
        reply
            .send((json!({"connectionStatus":"CONNECTED"}), None))
            .unwrap();
        assert!(worker.await.unwrap().is_ok());
        let done = trace.snapshot();
        for key in [
            "walletQueueWaitMs",
            "walletPrepareMs",
            "acceptedToDispatchMs",
        ] {
            assert!(done[key].as_f64().unwrap() >= 0., "{key}");
        }
        assert!(
            done["notificationToDispatchMs"].is_null(),
            "immediate commands have no WSS origin"
        );
    }
}
#[tokio::test]
async fn cancelling_queued_work_does_not_invent_lock_dispatch_or_http_stages() {
    let root = tempfile::tempdir().unwrap();
    let mut mock = Mock::new().await;
    let wallet = wallet(
        root.path(),
        "cancelled",
        "instance-timed",
        A,
        Some("synthetic-cookie"),
        &mock.origin,
    );
    let held = wallet.state.lock().await;
    let trace = trace();
    let worker_trace = trace.clone();
    let target = wallet.clone();
    let worker = tokio::spawn(task_timing::scope(worker_trace.clone(), async move {
        worker_trace.worker_started();
        target.execute(argv(&["wallet", "status"])).await
    }));
    entered(&trace).await;
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    drop(held);
    let row = trace.snapshot();
    assert!(row["walletQueueWaitMs"].is_null());
    assert!(row["unacquiredWalletQueueMs"].is_number());
    assert!(row["walletPrepareMs"].is_null());
    assert!(row["acceptedToDispatchMs"].is_null());
    assert!(mock.requests.try_recv().is_err());
}
