//! Monotonic, process-local task observations. Never authorizes/replays an action.
//! Notification ingress is local completed WSS-frame receipt, NOT chain inclusion time.
use serde_json::{json, Value};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Instant,
};

fn ms(from: Option<Instant>, to: Option<Instant>) -> Option<f64> {
    to?.checked_duration_since(from?)
        .map(|d| d.as_secs_f64() * 1000.0)
}
#[derive(Clone, Debug)]
pub(crate) struct TriggerObservation {
    source: &'static str,
    ingress: Option<Instant>,
    dequeued: Option<Instant>,
    candidate: Instant,
    pub accepted: Instant,
    fresh: bool,
    canonical: Option<crate::chain::rpc::RpcReadTiming>,
}
impl TriggerObservation {
    pub fn accepted(
        source: &'static str,
        ingress: Option<Instant>,
        dequeued: Option<Instant>,
        candidate: Instant,
        canonical: Option<crate::chain::rpc::RpcReadTiming>,
    ) -> Self {
        Self {
            source,
            ingress,
            dequeued,
            candidate,
            accepted: Instant::now(),
            fresh: true,
            canonical,
        }
    }
    pub fn simple(source: &'static str) -> Self {
        Self::accepted(source, None, None, Instant::now(), None)
    }
    pub fn resumed() -> Self {
        Self {
            fresh: false,
            ..Self::simple("resume_without_original_ingress")
        }
    }
    fn accepted_origin(&self) -> Option<Instant> {
        self.fresh.then_some(self.accepted)
    }
    pub fn step_json(
        &self,
        admission_start: Option<Instant>,
        admission_end: Option<Instant>,
    ) -> Value {
        json!({"scope":"local_monotonic_not_chain_or_network_delivery_latency","triggerSource":self.source,
            "notificationQueueMs":ms(self.ingress,self.dequeued),
            "notificationToAcceptedMs":ms(self.ingress,Some(self.accepted)),
            "candidateValidationMs":if self.fresh {ms(Some(self.candidate),Some(self.accepted))} else {None},
            "canonicalRead":self.canonical,
            "acceptedToAdmissionStartMs":ms(self.accepted_origin(),admission_start),
            "admissionSaveMs":ms(admission_start,admission_end),
            "acceptedToAdmissionEndMs":ms(self.accepted_origin(),admission_end)})
    }
}
#[derive(Clone, Copy)]
pub(crate) enum CoreRequest {
    Route,
    Preview,
    Execute,
}
impl CoreRequest {
    fn index(self) -> usize {
        match self {
            Self::Route => 0,
            Self::Preview => 1,
            Self::Execute => 2,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Span {
    started: Option<Instant>,
    finished: Option<Instant>,
}
#[derive(Default)]
struct Marks {
    worker_started: Option<Instant>,
    queue_entered: Option<Instant>,
    lock_acquired: Option<Instant>,
    dispatch_started: Option<Instant>,
    core: [Span; 3],
}
pub(crate) struct ActionTrace {
    trigger: TriggerObservation,
    admission_start: Instant,
    admission_end: Instant,
    enqueued: Instant,
    marks: Mutex<Marks>,
}
impl ActionTrace {
    pub fn new(
        trigger: TriggerObservation,
        admission_start: Instant,
        admission_end: Instant,
    ) -> Arc<Self> {
        Arc::new(Self {
            trigger,
            admission_start,
            admission_end,
            enqueued: Instant::now(),
            marks: Mutex::new(Marks::default()),
        })
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn worker_started(&self) {
        self.lock().worker_started = Some(Instant::now());
    }
    #[cfg(test)]
    pub fn snapshot(&self) -> Value {
        self.snapshot_at(Instant::now())
    }
    pub fn snapshot_at(&self, returned: Instant) -> Value {
        let m = self.lock();
        let origin = self.trigger.accepted_origin();
        let mut value = self
            .trigger
            .step_json(Some(self.admission_start), Some(self.admission_end));
        let row = value.as_object_mut().expect("timing object");
        for (name, value) in [
            (
                "admissionToEnqueueMs",
                ms(Some(self.admission_end), Some(self.enqueued)),
            ),
            (
                "admissionToWorkerStartMs",
                ms(Some(self.admission_end), m.worker_started),
            ),
            (
                "workerDispatchMs",
                ms(Some(self.enqueued), m.worker_started),
            ),
            (
                "workerToWalletQueueMs",
                ms(m.worker_started, m.queue_entered),
            ),
            ("walletQueueWaitMs", ms(m.queue_entered, m.lock_acquired)),
            (
                "unacquiredWalletQueueMs",
                if m.lock_acquired.is_none() {
                    ms(m.queue_entered, Some(returned))
                } else {
                    None
                },
            ),
            ("walletPrepareMs", ms(m.lock_acquired, m.dispatch_started)),
            ("acceptedToDispatchMs", ms(origin, m.dispatch_started)),
            (
                "notificationToDispatchMs",
                ms(self.trigger.ingress, m.dispatch_started),
            ),
            (
                "dispatchToActionReturnMs",
                ms(m.dispatch_started, Some(returned)),
            ),
            (
                "notificationToActionReturnMs",
                ms(self.trigger.ingress, Some(returned)),
            ),
        ] {
            row.insert(name.into(), json!(value));
        }
        let mut core = serde_json::Map::new();
        for (name, span) in ["okxSwap", "binancePreview", "binanceExecute"]
            .into_iter()
            .zip(m.core)
        {
            core.insert(
                name.into(),
                json!({"sinceAcceptedStartMs":ms(origin,span.started),
                "sinceAcceptedFinishMs":ms(origin,span.finished),
                "sinceNotificationStartMs":ms(self.trigger.ingress,span.started),
                "sinceNotificationFinishMs":ms(self.trigger.ingress,span.finished),
                "localAttemptMs":ms(span.started,span.finished),
                "finishMeans":"local_attempt_finished_or_dropped_not_chain_or_broadcast_time"}),
            );
        }
        row.insert("coreHttp".into(), Value::Object(core));
        value
    }
}
tokio::task_local! {static CURRENT:Arc<ActionTrace>;}
pub(crate) async fn scope<F: Future>(trace: Arc<ActionTrace>, future: F) -> F::Output {
    CURRENT.scope(trace, future).await
}
pub(crate) fn wallet_queue_entered() {
    let _ = CURRENT.try_with(|t| t.lock().queue_entered = Some(Instant::now()));
}
pub(crate) fn wallet_lock_acquired() {
    let _ = CURRENT.try_with(|t| t.lock().lock_acquired = Some(Instant::now()));
}
pub(crate) fn dispatch_started() {
    let _ = CURRENT.try_with(|t| t.lock().dispatch_started = Some(Instant::now()));
}
/// Drop completes a local attempt even on error/cancellation. It does not imply response/success.
pub(crate) struct CoreGuard {
    trace: Option<Arc<ActionTrace>>,
    kind: CoreRequest,
}
impl CoreGuard {
    pub fn begin(kind: CoreRequest) -> Self {
        let trace = CURRENT.try_with(Arc::clone).ok();
        if let Some(trace) = &trace {
            trace.lock().core[kind.index()].started = Some(Instant::now());
        }
        Self { trace, kind }
    }
}
impl Drop for CoreGuard {
    fn drop(&mut self) {
        if let Some(trace) = &self.trace {
            trace.lock().core[self.kind.index()].finished = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests;
