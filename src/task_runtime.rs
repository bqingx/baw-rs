//! Instance-owned pipeline controller. Private durable state precedes all side effects.
#[cfg(test)]
#[path = "event_backfill_tests.rs"]
mod event_backfill_tests;
#[path = "task_prepare.rs"]
mod preparation;
#[path = "task_recovery.rs"]
mod recovery;
#[cfg(test)]
#[path = "task_validation_tests.rs"]
mod validation_tests;
use super::*;
use crate::pipeline::{Completion, Outcome, Pipeline, Snapshot, Step};
use crate::task_timing::{self, TriggerObservation};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Load { config: Definition },
    Arm { id: String, confirm: bool },
    Status { id: String },
    Cancel { id: String },
    Reconcile { id: String },
    Resume { id: String, confirm: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub steps: Vec<Task>,
    /// Single shared chain endpoint, also used for receipt verification.
    pub rpc_url: Option<String>,
    pub chain_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub task_id: String,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    #[serde(default = "confirmed")]
    pub completion: Completion,
    pub valid_for_ms: u64,
    #[serde(default = "one")]
    pub confirmations: u64,
}
fn confirmed() -> Completion {
    Completion::Confirmed
}
fn one() -> u64 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    Immediate,
    Time {
        unix_ms: u64,
    },
    Block {
        number: u64,
    },
    Event {
        address: String,
        topics: Vec<String>,
        from_block: u64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub wallet: String,
    /// Wallet-relative command. Only contract-call fire and OKX swap are allowed.
    pub argv: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    definition: Definition,
    snapshot: Snapshot,
    status: String,
    results: BTreeMap<String, Value>,
    #[serde(default)]
    recovery: Option<Recovery>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    deadlines: BTreeMap<usize, u64>,
    triggered: std::collections::BTreeSet<usize>,
    bindings: BTreeMap<String, String>,
    #[serde(default)]
    preparation_deadlines: BTreeMap<usize, u64>,
    #[serde(default)]
    preparation_started: std::collections::BTreeSet<String>,
    #[serde(default)]
    prepared: BTreeMap<String, Value>,
    #[serde(skip)]
    plans: BTreeMap<String, Arc<preparation::Plan>>,
}
struct Run {
    state: Mutex<Stored>,
    cancel: Arc<AtomicBool>,
    active: AtomicBool,
}
#[derive(Default)]
pub(super) struct Scheduler {
    runs: Mutex<BTreeMap<String, Arc<Run>>>,
    stopping: AtomicBool,
    pub(super) rpc: Arc<crate::chain_rpc::SharedRpc>,
    notifications: crate::task_notifications::Notifier,
    inspection_gate: Arc<tokio::sync::RwLock<()>>,
}
#[path = "task_inspection.rs"]
mod inspection;
pub(crate) use inspection::describe_definition;
fn now() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}
fn notification_event(state: &Stored) -> Option<crate::task_notifications::Event> {
    if !matches!(
        state.status.as_str(),
        "COMPLETED" | "CANCELLED" | "PAUSED" | "RECONCILED" | "INTERRUPTED"
    ) {
        return None;
    }
    let mut event = crate::task_notifications::Event {
        task_id: state.definition.id.clone(),
        kind: "paused",
        completion: "incomplete",
        wallets: state
            .definition
            .steps
            .iter()
            .flat_map(|s| s.actions.iter().map(|a| a.wallet.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        confirmed: 0,
        submitted: 0,
        unresolved: 0,
        failed: 0,
        pending: 0,
        awaiting_app: 0,
        outcomes: Vec::new(),
    };
    for row in &state.snapshot.outcomes {
        event.outcomes.push(
            row.iter()
                .map(|outcome| match outcome {
                    Outcome::Confirmed => {
                        event.confirmed += 1;
                        "confirmed"
                    }
                    Outcome::Submitted => {
                        event.submitted += 1;
                        "submitted"
                    }
                    Outcome::InFlight => {
                        event.unresolved += 1;
                        "in_flight"
                    }
                    Outcome::Uncertain => {
                        event.unresolved += 1;
                        "uncertain"
                    }
                    Outcome::Failed => {
                        event.failed += 1;
                        "failed"
                    }
                    Outcome::Pending => {
                        event.pending += 1;
                        "pending"
                    }
                })
                .collect(),
        );
    }
    fn awaiting_app(value: &Value, depth: usize) -> bool {
        value["status"] == "PENDING_CONFIRMATION"
            || (depth > 0
                && ["execution", "autoApproval"]
                    .iter()
                    .any(|key| value.get(*key).is_some_and(|v| awaiting_app(v, depth - 1))))
    }
    let pipeline = Pipeline::from_live(state.snapshot.clone()).ok()?;
    let active = pipeline.active_step();
    let mut other_unknown = false;
    if let Some(index) = active {
        for (member, action) in state.definition.steps[index].actions.iter().enumerate() {
            let outcome = state.snapshot.outcomes[index][member];
            if !matches!(
                outcome,
                Outcome::Pending | Outcome::Uncertain | Outcome::InFlight
            ) {
                continue;
            }
            let key = format!("{index}:{}", action.wallet);
            let preparation_unknown = outcome == Outcome::Pending
                && state.recovery.as_ref().is_some_and(|r| {
                    r.preparation_started.contains(&key) && !r.prepared.contains_key(&key)
                });
            let app = state.results.get(&key).is_some_and(|v| awaiting_app(v, 3))
                || (state
                    .recovery
                    .as_ref()
                    .is_none_or(|r| !r.prepared.contains_key(&key))
                    && state
                        .results
                        .get(&format!("prepare:{key}"))
                        .is_some_and(|v| awaiting_app(v, 3)));
            if outcome == Outcome::Pending && (preparation_unknown || app) {
                event.pending -= 1;
                event.unresolved += 1;
                event.outcomes[index][member] = "preparation_unknown";
            }
            if app {
                event.awaiting_app += 1;
            } else if preparation_unknown
                || matches!(outcome, Outcome::Uncertain | Outcome::InFlight)
            {
                other_unknown = true;
            }
        }
    }
    let expired = active.is_some_and(|index| {
        state
            .recovery
            .as_ref()
            .is_some_and(|r| r.deadlines.get(&index).is_some_and(|d| now() >= *d))
    });
    let awaiting_receipt =
        active.is_some_and(|index| state.snapshot.outcomes[index].contains(&Outcome::Submitted));
    event.kind = match state.status.as_str() {
        "COMPLETED" => {
            event.completion = if event.submitted == 0 {
                "confirmed"
            } else {
                "submitted"
            };
            "completed"
        }
        "CANCELLED" => "cancelled",
        _ if (event.awaiting_app > 0
            && (other_unknown || event.failed > 0 || awaiting_receipt))
            || (event.failed > 0 && (other_unknown || awaiting_receipt)) =>
        {
            "mixed"
        }
        _ if event.awaiting_app > 0 => "awaiting_app",
        _ if other_unknown => "unknown",
        _ if event.failed > 0 => "failed",
        _ if awaiting_receipt => "unknown",
        _ if expired => "expired",
        _ => "paused",
    };
    Some(event)
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn operation_id(id: &str, step: usize, wallet: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "p-{:x}",
        Sha256::digest(json!([id, step, wallet]).to_string().as_bytes())
    )[..64]
        .to_owned()
}
fn argv(config: &Definition, step: usize, action: &Action) -> Result<Vec<String>, String> {
    validate_argv(&action.argv)?;
    for arg in &action.argv {
        if matches!(
            arg.split('=').next().unwrap_or(""),
            "--confirm" | "--operation-id" | "--skip-prechecks" | "--accept-preview-sha256"
        ) {
            return Err("task owns confirmation/operation ID and requires prechecks".into());
        }
    }
    let mut args = action.argv.clone();
    args.extend([
        "--confirm".into(),
        "--operation-id".into(),
        operation_id(&config.id, step, &action.wallet),
    ]);
    let mut full = vec!["baw-rs".into()];
    full.extend(args.clone());
    let cli = Cli::try_parse_from(full).map_err(|_| "invalid task action command")?;
    if cli.skip_prechecks {
        return Err("tasks require prechecks; disable BAW_RS_SKIP_PRECHECKS".into());
    }
    let chain = match &cli.command {
        Command::ContractCall {
            command: crate::signing::ContractCommand::Fire(a),
        } => &a.binance_chain_id,
        Command::MarketOrder {
            command: crate::trade::MarketCommand::Swap(a),
        } => &a.quote.binance_chain_id,
        _ => {
            return Err(
                "task actions only support contract-call fire or OKX market-order swap".into(),
            )
        }
    };
    if chain != &config.chain_id {
        return Err("task action chain differs from pipeline chain".into());
    }
    crate::dispatch::preflight(&cli)?;
    Ok(args)
}
fn validate(config: &Definition) -> Result<Pipeline, String> {
    validate_definition(config, true)
}
// Terminal archives are data, not executable CLI input. Validate their identity/topology,
// while all new loads and nonterminal continuation still require current action parsing.
fn validate_definition(config: &Definition, executable: bool) -> Result<Pipeline, String> {
    if !safe_id(&config.id) {
        return Err("invalid pipeline ID".into());
    }
    let chain: u64 = config
        .chain_id
        .parse()
        .map_err(|_| "pipeline requires decimal EVM chain ID")?;
    if chain == 0 || chain.to_string() != config.chain_id {
        return Err("invalid chain ID".into());
    }
    let mut steps = Vec::new();
    for (i, task) in config.steps.iter().enumerate() {
        if task.valid_for_ms == 0
            || now().checked_add(task.valid_for_ms).is_none()
            || !(1..=64).contains(&task.confirmations)
        {
            return Err(
                "valid_for_ms must be positive without deadline overflow; confirmations 1..64"
                    .into(),
            );
        }
        if executable {
            for action in &task.actions {
                argv(config, i, action)?;
            }
        }
        if let Trigger::Event {
            address,
            topics,
            from_block,
        } = &task.trigger
        {
            crate::signing::address(&config.chain_id, address)?;
            if *from_block == 0
                || topics.is_empty()
                || topics.len() > 4
                || topics.iter().any(|s| !hex_hash(s))
            {
                return Err("event needs nonzero from_block and 1..4 exact topic hashes".into());
            }
        }
        if (task.completion == Completion::Confirmed
            || matches!(task.trigger, Trigger::Block { .. } | Trigger::Event { .. }))
            && config.rpc_url.is_none()
        {
            return Err("block/event triggers and confirmed completion require rpc_url".into());
        }
        steps.push(Step {
            task_id: task.task_id.clone(),
            wallets: task.actions.iter().map(|a| a.wallet.clone()).collect(),
            completion: task.completion,
        });
    }
    if let Some(url) = &config.rpc_url {
        Arc::new(crate::chain_rpc::SharedRpc::default()).acquire(url)?;
    }
    Pipeline::new(steps)
}
fn hex_hash(s: &str) -> bool {
    s.len() == 66 && s.starts_with("0x") && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn quantity(v: &Value) -> Result<u64, String> {
    let s = v
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .ok_or("invalid RPC quantity")?;
    if s.is_empty() {
        return Err("empty RPC quantity".into());
    }
    u64::from_str_radix(s, 16).map_err(|_| "RPC quantity overflow".into())
}
fn path(registry: &Path, id: &str) -> Result<PathBuf, String> {
    if !safe_id(id) {
        return Err("invalid pipeline ID".into());
    }
    crate::wallets::private_dir(registry)?;
    Ok(registry.join(format!("pipeline-{id}.json")))
}
const MAX_STATE_BYTES: usize = 4 * 1024 * 1024;
fn save(registry: &Path, stored: &Stored) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let target = path(registry, &stored.definition.id)?;
    // Never publish a state that `load` would refuse: a task must stay recoverable.
    let encoded = serde_json::to_vec(stored).map_err(|_| "cannot encode pipeline state")?;
    if encoded.len() > MAX_STATE_BYTES {
        return Err("pipeline state too large; not saved".into());
    }
    let tmp = registry.join(format!(".pipeline-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&tmp)
            .map_err(|_| "cannot create pipeline state")?;
        file.write_all(&encoded)
            .map_err(|_| "cannot write pipeline state")?;
        file.flush().map_err(|_| "cannot flush pipeline state")?;
        file.sync_all().map_err(|_| "cannot sync pipeline state")?;
        std::fs::rename(&tmp, &target).map_err(|_| "cannot publish pipeline state")?;
        File::open(registry)
            .and_then(|f| f.sync_all())
            .map_err(|_| "cannot sync pipeline directory")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
fn load(registry: &Path, id: &str) -> Result<Stored, String> {
    let mut bytes = Vec::new();
    crate::wallets::private_file(&path(registry, id)?, false)?
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read pipeline state")?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err("pipeline state too large".into());
    }
    let mut state: Stored = serde_json::from_slice(&bytes).map_err(|_| "invalid pipeline state")?;
    if state.definition.id != id {
        return Err("pipeline state ID mismatch".into());
    }
    let expected = if matches!(state.status.as_str(), "COMPLETED" | "CANCELLED") {
        validate_definition(&state.definition, false)?
    } else {
        validate(&state.definition)?
    }
    .snapshot();
    if expected.steps != state.snapshot.steps {
        return Err("pipeline definition/state mismatch".into());
    }
    state.snapshot = Pipeline::restore(state.snapshot)?.snapshot();
    if state.status == "RUNNING" {
        state.status = "INTERRUPTED".into();
    }
    Ok(state)
}
impl Scheduler {
    async fn get(&self, registry: &Path, id: &str) -> Result<Arc<Run>, String> {
        let mut runs = self.runs.lock().await;
        if let Some(run) = runs.get(id) {
            return Ok(run.clone());
        }
        let state = load(registry, id)?;
        let run = Arc::new(Run {
            state: Mutex::new(state),
            cancel: Arc::new(AtomicBool::new(false)),
            active: AtomicBool::new(false),
        });
        runs.insert(id.into(), run.clone());
        Ok(run)
    }
    pub(super) async fn control(
        self: &Arc<Self>,
        daemon: Arc<Daemon>,
        control: Control,
    ) -> Result<Value, String> {
        let registry = daemon
            .registry
            .as_deref()
            .ok_or("instance pipelines require the named-wallet daemon")?;
        match control {
            Control::Load { config } => {
                let pipeline = validate(&config)?;
                // Resolve every member before publishing a runnable definition.
                for task in &config.steps {
                    for action in &task.actions {
                        wallet_for(&daemon.wallets, &action.wallet, daemon.registry.as_ref())
                            .await?;
                    }
                }
                let mut runs = self.runs.lock().await;
                if runs.contains_key(&config.id)
                    || path(registry, &config.id)?.symlink_metadata().is_ok()
                {
                    return Err(
                        "pipeline ID already exists; use a new ID, never replace execution history"
                            .into(),
                    );
                }
                let id = config.id.clone();
                let state = Stored {
                    definition: config,
                    snapshot: pipeline.snapshot(),
                    status: "LOADED".into(),
                    results: BTreeMap::new(),
                    recovery: None,
                };
                save(registry, &state)?;
                runs.insert(
                    id.clone(),
                    Arc::new(Run {
                        state: Mutex::new(state),
                        cancel: Arc::new(AtomicBool::new(false)),
                        active: AtomicBool::new(false),
                    }),
                );
                Ok(json!({"id":id,"status":"LOADED"}))
            }
            Control::Status { id } => {
                let run = self.get(registry, &id).await?;
                let state = run.state.lock().await;
                Ok(
                    json!({"id":id,"status":state.status,"snapshot":state.snapshot,"results":state.results}),
                )
            }
            Control::Cancel { id } => {
                let run = self.get(registry, &id).await?;
                run.cancel.store(true, Ordering::SeqCst);
                let mut state = run.state.lock().await;
                // A concurrent resume may have reset the flag while we waited.
                run.cancel.store(true, Ordering::SeqCst);
                if state.status != "COMPLETED" {
                    state.status = "CANCELLED".into();
                    save(registry, &state)?;
                    if !run.active.load(Ordering::SeqCst) {
                        self.notify_state(registry, &state);
                    }
                }
                Ok(
                    json!({"id":id,"status":state.status,"note":"already admitted submissions drain; cancellation cannot undo a transaction"}),
                )
            }
            Control::Arm { id, confirm } => {
                if self.stopping.load(Ordering::SeqCst) {
                    return Err("instance is stopping; pipeline not armed".into());
                }
                if !confirm {
                    return Err("arming a pipeline requires --confirm for all its actions".into());
                }
                let inspection = self.admit_execution()?;
                let run = self.get(registry, &id).await?;
                {
                    let mut state = run.state.lock().await;
                    if state.status != "LOADED" || run.active.load(Ordering::SeqCst) {
                        return Err(
                            "pipeline already used or interrupted; reconcile, do not rearm".into(),
                        );
                    }
                    let mut recovery = Recovery::default();
                    // Endpoint conflict is rejected before authorization is consumed.
                    if let Some(url) = &state.definition.rpc_url {
                        self.rpc.acquire(url)?;
                    }
                    let names = state
                        .definition
                        .steps
                        .iter()
                        .flat_map(|step| step.actions.iter().map(|action| action.wallet.clone()))
                        .collect::<std::collections::BTreeSet<_>>();
                    let mut preparation = tokio::task::JoinSet::new();
                    for name in names {
                        let wallet =
                            wallet_for(&daemon.wallets, &name, daemon.registry.as_ref()).await?;
                        preparation.spawn(async move {
                            tokio::time::timeout(Duration::from_secs(30), wallet.preload())
                                .await
                                .map_err(|_| "wallet preparation timed out".to_owned())??;
                            Ok::<_, String>((name, wallet.pipeline_binding()?))
                        });
                    }
                    while let Some(result) = preparation.join_next().await {
                        let (name, binding) =
                            result.map_err(|_| "wallet preparation worker failed")??;
                        recovery.bindings.insert(name, binding);
                    }
                    recovery.plans =
                        preparation::compile_plans(&daemon, &state.definition, &recovery.bindings)
                            .await?;
                    if run.cancel.load(Ordering::SeqCst) || self.stopping.load(Ordering::SeqCst) {
                        return Err(
                            "pipeline cancelled or instance stopped during preparation".into()
                        );
                    }
                    state.recovery = Some(recovery);
                    state.status = "RUNNING".into();
                    save(registry, &state)?;
                    run.active.store(true, Ordering::SeqCst);
                }
                self.launch(daemon, run, inspection);
                Ok(json!({"id":id,"status":"RUNNING"}))
            }
            Control::Reconcile { id } => self.reconcile(&daemon, &id).await,
            Control::Resume { id, confirm } => self.resume(daemon, &id, confirm).await,
        }
    }
    fn admit_execution(&self) -> Result<tokio::sync::OwnedRwLockReadGuard<()>, String> {
        self.inspection_gate.clone().try_read_owned().map_err(|_| {
            "manual overview refresh in progress; task not started, retry explicitly".into()
        })
    }
    pub(crate) fn admit_inspection(&self) -> Option<tokio::sync::OwnedRwLockWriteGuard<()>> {
        self.inspection_gate.clone().try_write_owned().ok()
    }
    fn launch(
        self: &Arc<Self>,
        daemon: Arc<Daemon>,
        run: Arc<Run>,
        inspection: tokio::sync::OwnedRwLockReadGuard<()>,
    ) {
        let scheduler = self.clone();
        tokio::spawn(async move {
            let _inspection = inspection;
            let result = scheduler.execute(daemon.clone(), run.clone()).await;
            let mut state = run.state.lock().await;
            if result.is_err() && state.status == "RUNNING" {
                state.status = "PAUSED".into();
            }
            if let Err(error) = result {
                state
                    .results
                    .insert("runtime".into(), json!({"error":error}));
            }
            if let Some(registry) = &daemon.registry {
                if save(registry, &state).is_err() {
                    state.status = "PERSISTENCE_ERROR".into();
                } else {
                    scheduler.notify_state(registry, &state);
                }
            }
            run.active.store(false, Ordering::SeqCst);
        });
    }
    pub(super) async fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let runs = self.runs.lock().await.values().cloned().collect::<Vec<_>>();
        for run in &runs {
            run.cancel.store(true, Ordering::SeqCst);
        }
        loop {
            if runs.iter().all(|r| !r.active.load(Ordering::SeqCst)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        self.notifications.flush().await;
    }
    pub(super) fn notification_status(&self) -> Value {
        self.notifications.status()
    }
    fn notify_state(&self, registry: &Path, state: &Stored) {
        if let Some(event) = notification_event(state) {
            self.notifications.publish(registry, event);
        }
    }
    #[cfg(test)]
    pub(super) fn with_notifications(notifications: crate::task_notifications::Notifier) -> Self {
        Self {
            notifications,
            ..Self::default()
        }
    }
    async fn execute(&self, daemon: Arc<Daemon>, run: Arc<Run>) -> Result<(), String> {
        let config = run.state.lock().await.definition.clone();
        let registry = daemon.registry.as_deref().ok_or("registry missing")?;
        for (index, task) in config.steps.iter().enumerate() {
            if run.cancel.load(Ordering::SeqCst) {
                return Err("pipeline cancelled before next task".into());
            }
            let already_triggered = {
                let mut state = run.state.lock().await;
                let pipeline = Pipeline::from_live(state.snapshot.clone())?;
                if pipeline.active_step().is_none_or(|active| active > index) {
                    continue;
                }
                if pipeline.active_step() != Some(index) || pipeline.paused() {
                    return Err("unresolved predecessor; reconcile before continuing".into());
                }
                let metadata = state
                    .recovery
                    .as_mut()
                    .ok_or("persisted task recovery state missing")?;
                if metadata
                    .deadlines
                    .get(&index)
                    .is_some_and(|deadline| now() >= *deadline)
                {
                    return Err("original task deadline expired; never renewed on resume".into());
                }
                metadata.triggered.contains(&index)
            };
            // One endpoint across preparation, listening and confirmation. Opening stays
            // lazy unless allowance preparation or this step's trigger needs RPC.
            let task_rpc = config
                .rpc_url
                .as_deref()
                .map(|url| self.rpc.acquire(url))
                .transpose()?;
            preparation::prepare_step(self, &daemon, &run, &config, index, already_triggered)
                .await?;
            let deadline = {
                let mut state = run.state.lock().await;
                if run.cancel.load(Ordering::SeqCst) {
                    return Err("pipeline cancelled during preparation".into());
                }
                let metadata = state.recovery.as_mut().ok_or("recovery state missing")?;
                let deadline = *metadata.deadlines.entry(index).or_insert(
                    now()
                        .checked_add(task.valid_for_ms)
                        .ok_or("task deadline overflow")?,
                );
                state.results.insert("phase".into(),json!({"task":index,"status":if already_triggered {"EXECUTING"} else {"STARTING_LISTENER"}}));
                save(registry, &state)?;
                deadline
            };
            let trigger_timing = if already_triggered {
                TriggerObservation::resumed()
            } else {
                let observed = wait_trigger(
                    &run,
                    &task.trigger,
                    deadline,
                    &self.rpc,
                    config.rpc_url.as_deref(),
                    &config.chain_id,
                )
                .await?;
                let mut state = run.state.lock().await;
                state
                    .recovery
                    .as_mut()
                    .ok_or("recovery state missing")?
                    .triggered
                    .insert(index);
                // Persist the trigger together with the admission claims below. No action
                // can run before that durable save; failures still checkpoint in launch().
                observed
            };
            let mut prepared = Vec::new();
            for (member, action) in task.actions.iter().enumerate() {
                let plan = {
                    let state = run.state.lock().await;
                    match state.snapshot.outcomes[index][member] {
                        Outcome::Confirmed | Outcome::Submitted => continue,
                        Outcome::Pending => {}
                        _ => {
                            return Err(
                                "unresolved action cannot be replayed; reconcile first".into()
                            )
                        }
                    }
                    state
                        .recovery
                        .as_ref()
                        .and_then(|r| r.plans.get(&format!("{index}:{}", action.wallet)))
                        .cloned()
                        .ok_or("compiled task plan missing")?
                };
                if plan.wallet.pipeline_binding()? != plan.binding {
                    return Err("wallet identity/session changed after task authorization".into());
                }
                let proof = if plan.prepare.is_some() {
                    Some(run.state.lock().await.recovery.as_ref()
                        .and_then(|r| r.prepared.get(&format!("{index}:{}",action.wallet)))
                        .cloned().ok_or("task swap allowance was not prepared; no on-trigger approval allowed")?)
                } else {
                    None
                };
                prepared.push((action.wallet.clone(), plan, proof));
            }
            if run.cancel.load(Ordering::SeqCst) || now() >= deadline {
                return Err("cancelled or expired before admission".into());
            }
            let (admission_start, admission_end) = {
                let mut state = run.state.lock().await;
                let mut pipeline = Pipeline::from_live(state.snapshot.clone())?;
                for (name, _, _) in &prepared {
                    pipeline.claim(index, name)?;
                }
                state.snapshot = pipeline.snapshot();
                state
                    .results
                    .insert("phase".into(), json!({"task":index,"status":"EXECUTING"}));
                state.results.insert(
                    format!("triggerTiming:{index}"),
                    trigger_timing.step_json(None, None),
                );
                let start = Instant::now();
                save(registry, &state)?;
                let end = Instant::now();
                // No extra durable write: complete observations join the existing result save.
                state.results.insert(
                    format!("triggerTiming:{index}"),
                    trigger_timing.step_json(Some(start), Some(end)),
                );
                (start, end)
            };
            let mut jobs = tokio::task::JoinSet::new();
            for (name, plan, proof) in prepared {
                let run = run.clone();
                let rpc = self.rpc.clone();
                let trace = task_timing::ActionTrace::new(
                    trigger_timing.clone(),
                    admission_start,
                    admission_end,
                );
                jobs.spawn(async move {
                    let started = std::time::Instant::now();
                    trace.worker_started();
                    let bound_wallet = plan.wallet.clone();
                    let expected = plan.binding.clone();
                    let check = Arc::new(move || {
                        if bound_wallet.pipeline_binding()? != expected {
                            Err("wallet identity/session changed; task request blocked".into())
                        } else {
                            Ok(())
                        }
                    });
                    let execute = async {
                        match proof {
                            Some(proof) => {
                                crate::execution::allowance_scope(
                                    crate::execution::TaskAllowance::Prepared(proof),
                                    plan.wallet.execute_compiled(&plan.execute),
                                )
                                .await
                            }
                            None => plan.wallet.execute_compiled(&plan.execute).await,
                        }
                    };
                    let result = task_timing::scope(
                        trace.clone(),
                        crate::execution::binding_scope(
                            check,
                            crate::execution::rpc_scope(
                                rpc,
                                crate::execution::task_scope(deadline, run.cancel.clone(), execute),
                            ),
                        ),
                    )
                    .await;
                    let returned = Instant::now();
                    let elapsed = returned.duration_since(started).as_secs_f64() * 1000.0;
                    let observation = trace.snapshot_at(returned);
                    (name, result, elapsed, observation, returned)
                });
            }
            // Never return/drop JoinSet while an admitted submission is still running.
            let mut drain_error = None;
            while let Some(joined) = jobs.join_next().await {
                let (wallet, result, elapsed, mut observation, returned) = match joined {
                    Ok(row) => row,
                    Err(_) => {
                        drain_error = Some("wallet worker failed; outcome uncertain".to_owned());
                        continue;
                    }
                };
                let collected_ms = returned.elapsed().as_secs_f64() * 1000.0;
                observation["workerResultCollectMs"] = json!(collected_ms);
                observation["notificationToResultCollectMs"] = json!(observation
                    ["notificationToActionReturnMs"]
                    .as_f64()
                    .map(|value| value + collected_ms));
                let (outcome, mut evidence) = match result {
                    Ok(value) if submitted(&value) => (Outcome::Submitted, value),
                    Ok(value) => (Outcome::Uncertain, value),
                    Err(error) => (Outcome::Uncertain, json!({"error":error})),
                };
                if let Value::Object(row) = &mut evidence {
                    row.insert("walletExecuteMs".into(), json!(elapsed));
                    row.insert("triggerTiming".into(), observation);
                }
                let lock_requested = Instant::now();
                let mut state = run.state.lock().await;
                if let Some(Value::Object(timing)) = evidence.get_mut("triggerTiming") {
                    timing.insert(
                        "resultStateLockWaitMs".into(),
                        json!(lock_requested.elapsed().as_secs_f64() * 1000.0),
                    );
                }
                let update = (|| {
                    let mut pipeline = Pipeline::from_live(state.snapshot.clone())?;
                    pipeline.record(index, &wallet, outcome)?;
                    state.snapshot = pipeline.snapshot();
                    state.results.insert(format!("{index}:{wallet}"), evidence);
                    let started = Instant::now();
                    save(registry, &state)?;
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    // Filled in memory after this save; a subsequent existing save persists it.
                    // Never add a telemetry-only fsync to the trigger or result path.
                    if let Some(Value::Object(timing)) = state
                        .results
                        .get_mut(&format!("{index}:{wallet}"))
                        .and_then(|v| v.get_mut("triggerTiming"))
                    {
                        timing.insert("resultSaveMs".into(), json!(elapsed));
                    }
                    Ok::<_, String>(())
                })();
                if let Err(error) = update {
                    drain_error = Some(error);
                }
            }
            if let Some(error) = drain_error {
                return Err(error);
            }
            if task.completion == Completion::Confirmed {
                let rpc = task_rpc.as_ref().ok_or("receipt RPC missing")?;
                check_chain(rpc, &config.chain_id).await?;
                for (member, action) in task.actions.iter().enumerate() {
                    if run.state.lock().await.snapshot.outcomes[index][member] == Outcome::Confirmed
                    {
                        continue;
                    }
                    let evidence = run
                        .state
                        .lock()
                        .await
                        .results
                        .get(&format!("{index}:{}", action.wallet))
                        .cloned()
                        .ok_or("action was not admitted")?;
                    let hash = transaction_hash(&evidence).ok_or("backend returned no transaction hash; reconcile order ID before continuing")?;
                    let wallet = run
                        .state
                        .lock()
                        .await
                        .recovery
                        .as_ref()
                        .and_then(|r| r.plans.get(&format!("{index}:{}", action.wallet)))
                        .map(|p| p.wallet.clone())
                        .ok_or("compiled receipt wallet missing")?;
                    let fixed_sender = {
                        let state = run.state.lock().await;
                        preparation::receipt_sender(
                            &state,
                            index,
                            &action.wallet,
                            &config.chain_id,
                        )?
                    };
                    let sender = match fixed_sender {
                        Some(sender) => sender,
                        None => wallet.chain_address(&config.chain_id).await?,
                    };
                    let result =
                        wait_receipt(rpc, hash, &sender, deadline, task.confirmations, &run)
                            .await?;
                    let mut state = run.state.lock().await;
                    let mut pipeline = Pipeline::from_live(state.snapshot.clone())?;
                    pipeline.record(
                        index,
                        &action.wallet,
                        if result {
                            Outcome::Confirmed
                        } else {
                            Outcome::Failed
                        },
                    )?;
                    state.snapshot = pipeline.snapshot();
                    save(registry, &state)?;
                    if !result {
                        return Err("transaction reverted; successors paused".into());
                    }
                }
            }
            let state = run.state.lock().await;
            let pipeline = Pipeline::from_live(state.snapshot.clone())?;
            if pipeline.paused() || pipeline.active_step().is_some_and(|i| i <= index) {
                return Err("task incomplete or uncertain; successors paused".into());
            }
        }
        let mut state = run.state.lock().await;
        if run.cancel.load(Ordering::SeqCst) {
            return Err("pipeline cancelled".into());
        }
        state.status = "COMPLETED".into();
        save(registry, &state)
    }
}
fn transaction_hash(v: &Value) -> Option<&str> {
    [
        v.get("txHash"),
        v.pointer("/execution/txHash"),
        v.pointer("/execution/execution/txHash"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .find(|s| hex_hash(s))
}
fn submitted(v: &Value) -> bool {
    for candidate in [
        Some(v),
        v.get("execution"),
        v.pointer("/execution/execution"),
    ]
    .into_iter()
    .flatten()
    {
        if matches!(
            candidate["status"].as_str(),
            Some("PENDING_CONFIRMATION" | "SUBMIT_FAIL" | "FAILED" | "UNCERTAIN")
        ) {
            return false;
        }
    }
    if matches!(
        v["status"].as_str(),
        Some("UNCERTAIN" | "SWAP_NOT_SUBMITTED" | "FAILED")
    ) {
        return false;
    }
    // An order ID acknowledges a backend request, not a bound transaction submission.
    // Without a valid hash the result remains unresolved even for completion=submitted.
    transaction_hash(v).is_some()
}
async fn check_chain(rpc: &crate::chain_rpc::Lease, chain: &str) -> Result<(), String> {
    if quantity(&rpc.read("eth_chainId", json!([])).await?)?.to_string() != chain {
        return Err("shared RPC chain mismatch".into());
    }
    Ok(())
}
async fn wait_trigger(
    run: &Run,
    trigger: &Trigger,
    deadline: u64,
    manager: &Arc<crate::chain_rpc::SharedRpc>,
    url: Option<&str>,
    chain: &str,
) -> Result<TriggerObservation, String> {
    let rpc = if matches!(trigger, Trigger::Block { .. } | Trigger::Event { .. }) {
        let lease = manager.acquire(url.ok_or("trigger RPC missing")?)?;
        check_chain(&lease, chain).await?;
        Some(lease)
    } else {
        None
    };
    let mut subscription = match trigger {
        Trigger::Block { .. } => Some(
            rpc.as_ref()
                .ok_or("RPC missing")?
                .subscribe(json!(["newHeads"]))
                .await?,
        ),
        Trigger::Event {
            address, topics, ..
        } => Some(
            rpc.as_ref()
                .ok_or("RPC missing")?
                .subscribe(json!(["logs", {"address":address,"topics":topics}]))
                .await?,
        ),
        _ => None,
    };
    // Subscribe before the initial head/backfill read so events during setup are buffered.
    let mut head = if let Some(rpc) = &rpc {
        Some(quantity(&rpc.read("eth_blockNumber", json!([])).await?)?)
    } else {
        None
    };
    let mut live: Option<(crate::chain_rpc::ReceivedNotification, Instant)> = None;
    let mut block_ingress = None;
    let mut cursor = match trigger {
        Trigger::Event { from_block, .. } => *from_block,
        _ => 0,
    };
    let mut backfilled = false;
    let mut ready_marked = false;
    loop {
        if run.cancel.load(Ordering::SeqCst) {
            return Err("cancelled while waiting for trigger".into());
        }
        if now() >= deadline {
            return Err("task expired before trigger".into());
        }
        match trigger {
            Trigger::Immediate => return Ok(TriggerObservation::simple("immediate")),
            Trigger::Time { unix_ms } if now() >= *unix_ms => {
                return Ok(TriggerObservation::simple("time"))
            }
            Trigger::Block { number } => {
                if let Some((header, dequeued)) = live.take() {
                    head = Some(quantity(&header.value["number"])?);
                    block_ingress = Some((header.received_at, dequeued));
                }
                if head.is_some_and(|head| head >= *number) {
                    let (ingress, dequeued) = block_ingress
                        .map(|(a, b)| (Some(a), Some(b)))
                        .unwrap_or((None, None));
                    return Ok(TriggerObservation::accepted(
                        if ingress.is_some() {
                            "live_head"
                        } else {
                            "initial_head"
                        },
                        ingress,
                        dequeued,
                        Instant::now(),
                        None,
                    ));
                }
            }
            Trigger::Event {
                address, topics, ..
            } => {
                let rpc = rpc.as_ref().ok_or("RPC missing")?;
                let initial_head = head.ok_or("initial head missing")?;
                let is_backfill = !backfilled;
                let logs = if is_backfill {
                    if initial_head >= cursor {
                        if initial_head - cursor >= 500 {
                            return Err(
                                "event backfill exceeds 500 blocks; narrow from_block".into()
                            );
                        }
                        // Alchemy Free permits at most 10 blocks per eth_getLogs call.
                        // This is planned pagination, never a retry or another transport.
                        let end = initial_head.min(cursor.saturating_add(9));
                        rpc.read("eth_getLogs", json!([{"address":address,"topics":topics,"fromBlock":format!("0x{cursor:x}"),"toBlock":format!("0x{end:x}")}])).await?.as_array().cloned().ok_or("invalid log result")?.into_iter().map(|log|(log,None,None)).collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                } else {
                    live.take()
                        .map(|(event, dequeued)| {
                            (event.value, Some(event.received_at), Some(dequeued))
                        })
                        .into_iter()
                        .collect::<Vec<_>>()
                };
                for (log, ingress, dequeued) in &logs {
                    let candidate = Instant::now();
                    if log["removed"] == true {
                        continue;
                    }
                    if log["address"]
                        .as_str()
                        .is_none_or(|s| !s.eq_ignore_ascii_case(address))
                    {
                        return Err("log address mismatch".into());
                    }
                    let actual = log["topics"].as_array().ok_or("log topics missing")?;
                    if topics.iter().enumerate().any(|(i, t)| {
                        actual
                            .get(i)
                            .and_then(Value::as_str)
                            .is_none_or(|s| !s.eq_ignore_ascii_case(t))
                    }) {
                        return Err("log topics mismatch".into());
                    }
                    let height = quantity(&log["blockNumber"])?;
                    if is_backfill && height > initial_head.min(cursor.saturating_add(9)) {
                        return Err("backfill log outside requested interval".into());
                    }
                    if height < cursor {
                        continue;
                    }
                    let (canonical, canonical_timing) = rpc
                        .read_observed("eth_getBlockByNumber", json!([log["blockNumber"], false]))
                        .await?;
                    if !log["blockHash"].as_str().is_some_and(hex_hash)
                        || canonical["hash"] != log["blockHash"]
                        || quantity(&canonical["number"])? != height
                    {
                        return Err("event block is no longer canonical".into());
                    }
                    return Ok(TriggerObservation::accepted(
                        if is_backfill {
                            "backfill_log"
                        } else {
                            "live_log"
                        },
                        *ingress,
                        *dequeued,
                        candidate,
                        Some(canonical_timing),
                    ));
                }
                if is_backfill {
                    let end = initial_head.min(cursor.saturating_add(9));
                    cursor = cursor.max(end.checked_add(1).ok_or("block overflow")?);
                    backfilled = cursor > initial_head;
                    // Do not consume a live notification between pages: it would be
                    // overwritten while the next page is read. The subscription buffers it.
                    if !backfilled {
                        continue;
                    }
                }
            }
            _ => {}
        }
        if !ready_marked && (!matches!(trigger, Trigger::Event { .. }) || backfilled) {
            let mut state = run.state.lock().await;
            if state.status == "RUNNING" && !run.cancel.load(Ordering::SeqCst) {
                if let Some(phase) = state.results.get_mut("phase") {
                    phase["status"] = json!("WAITING");
                }
            }
            ready_marked = true;
        }
        if let Some(subscription) = &mut subscription {
            if let Ok(event) =
                tokio::time::timeout(Duration::from_millis(100), subscription.recv_observed()).await
            {
                live = Some((event?, Instant::now()));
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
async fn wait_receipt(
    rpc: &crate::chain_rpc::Lease,
    hash: &str,
    sender: &str,
    deadline: u64,
    confirmations: u64,
    run: &Run,
) -> Result<bool, String> {
    loop {
        if run.cancel.load(Ordering::SeqCst) || now() >= deadline {
            return Err("receipt wait cancelled or expired; transaction may still be live".into());
        }
        if let Some(success) = recovery::receipt_status(rpc, hash, sender, confirmations).await? {
            return Ok(success);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
