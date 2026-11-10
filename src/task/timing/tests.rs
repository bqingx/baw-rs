use super::*;
use std::time::Duration;
fn at(base: Instant, offset: u64) -> Instant {
    base + Duration::from_millis(offset)
}
fn synthetic() -> Arc<ActionTrace> {
    // Synthetic offsets test arithmetic only, never reported as real latency.
    let base = Instant::now() - Duration::from_secs(1);
    let trigger = TriggerObservation {
        source: "live_log",
        ingress: Some(base),
        dequeued: Some(at(base, 10)),
        candidate: at(base, 15),
        accepted: at(base, 30),
        fresh: true,
        canonical: Some(crate::chain::rpc::RpcReadTiming {
            total_ms: 12.,
            queue_wait_ms: 2.,
            throttle_wait_ms: 3.,
            exchange_ms: 7.,
            cache_hit: false,
        }),
    };
    Arc::new(ActionTrace {
        trigger,
        admission_start: at(base, 35),
        admission_end: at(base, 40),
        enqueued: at(base, 40),
        marks: Mutex::new(Marks {
            worker_started: Some(at(base, 45)),
            queue_entered: Some(at(base, 50)),
            lock_acquired: Some(at(base, 60)),
            dispatch_started: Some(at(base, 70)),
            core: [
                Span {
                    started: Some(at(base, 75)),
                    finished: Some(at(base, 85)),
                },
                Span::default(),
                Span::default(),
            ],
        }),
    })
}
#[test]
fn boundaries_are_monotonic_and_keep_queue_admission_and_core_work_distinct() {
    let row = synthetic().snapshot();
    for (key, value) in [
        ("notificationQueueMs", 10.),
        ("notificationToAcceptedMs", 30.),
        ("candidateValidationMs", 15.),
        ("acceptedToAdmissionStartMs", 5.),
        ("admissionSaveMs", 5.),
        ("acceptedToAdmissionEndMs", 10.),
        ("workerDispatchMs", 5.),
        ("workerToWalletQueueMs", 5.),
        ("walletQueueWaitMs", 10.),
        ("walletPrepareMs", 10.),
        ("acceptedToDispatchMs", 40.),
        ("notificationToDispatchMs", 70.),
    ] {
        assert_eq!(row[key].as_f64().unwrap(), value, "{key}");
    }
    assert_eq!(row["coreHttp"]["okxSwap"]["sinceAcceptedStartMs"], 45.);
    assert_eq!(row["coreHttp"]["okxSwap"]["sinceNotificationFinishMs"], 85.);
    assert_eq!(row["coreHttp"]["okxSwap"]["localAttemptMs"], 10.);
}
#[test]
fn missing_stages_are_null_not_fabricated_zero() {
    let trace = ActionTrace::new(
        TriggerObservation::simple("backfill_log"),
        Instant::now(),
        Instant::now(),
    );
    let row = trace.snapshot();
    for key in [
        "notificationQueueMs",
        "notificationToAcceptedMs",
        "walletQueueWaitMs",
        "walletPrepareMs",
        "acceptedToDispatchMs",
        "notificationToDispatchMs",
    ] {
        assert!(row[key].is_null(), "{key}");
    }
    assert!(row["coreHttp"]["binanceExecute"]["localAttemptMs"].is_null());
}
#[test]
fn resumed_work_never_invents_old_ingress_or_trigger_acceptance() {
    let trace = ActionTrace::new(
        TriggerObservation::resumed(),
        Instant::now(),
        Instant::now(),
    );
    let row = trace.snapshot();
    assert_eq!(row["triggerSource"], "resume_without_original_ingress");
    for key in [
        "notificationToAcceptedMs",
        "notificationToDispatchMs",
        "notificationToActionReturnMs",
        "acceptedToAdmissionStartMs",
        "acceptedToAdmissionEndMs",
    ] {
        assert!(row[key].is_null(), "{key}");
    }
    assert!(row["admissionSaveMs"].is_number());
}
#[test]
fn reversed_endpoints_are_unavailable_instead_of_negative_or_clamped() {
    let now = Instant::now();
    assert!(ms(Some(now), Some(now - Duration::from_millis(1))).is_none());
    assert!(ms(None, Some(now)).is_none());
    assert!(ms(Some(now), None).is_none());
}
#[tokio::test]
async fn markers_and_core_attempts_are_scoped_to_one_wallet_action() {
    let trace = ActionTrace::new(
        TriggerObservation::simple("immediate"),
        Instant::now(),
        Instant::now(),
    );
    trace.worker_started();
    scope(trace.clone(), async {
        wallet_queue_entered();
        wallet_lock_acquired();
        dispatch_started();
        {
            let _guard = CoreGuard::begin(CoreRequest::Preview);
        }
        tokio::spawn(async {
            wallet_queue_entered();
            let _guard = CoreGuard::begin(CoreRequest::Execute);
        })
        .await
        .unwrap();
    })
    .await;
    {
        let _guard = CoreGuard::begin(CoreRequest::Execute);
    }
    let row = trace.snapshot();
    assert!(row["walletQueueWaitMs"].is_number());
    assert!(row["coreHttp"]["binancePreview"]["localAttemptMs"].is_number());
    assert!(row["coreHttp"]["binanceExecute"]["localAttemptMs"].is_null());
}
#[tokio::test]
async fn abort_records_a_dropped_local_attempt_not_an_assumed_response() {
    let trace = ActionTrace::new(
        TriggerObservation::simple("immediate"),
        Instant::now(),
        Instant::now(),
    );
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(scope(trace.clone(), async {
        let _guard = CoreGuard::begin(CoreRequest::Execute);
        tx.send(()).unwrap();
        std::future::pending::<()>().await;
    }));
    rx.await.unwrap();
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
    let row = trace.snapshot();
    let attempt = &row["coreHttp"]["binanceExecute"];
    assert!(attempt["localAttemptMs"].is_number());
    assert_eq!(
        attempt["finishMeans"],
        "local_attempt_finished_or_dropped_not_chain_or_broadcast_time"
    );
}
