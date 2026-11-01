//! Typed resident contract tasks. No CLI parsing and no process-global request policy.
use super::*;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub binance_chain_id: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub input_data: String,
    pub gas_limit: Option<u64>,
}
impl Call {
    pub(super) fn validate(&self) -> Result<(), String> {
        crate::signing::contract_payload(&crate::signing::ContractPreview {
            binance_chain_id: self.binance_chain_id.clone(),
            from: self.from.clone(),
            to: Some(self.to.clone()),
            value: Some(self.value.clone()),
            input_data: Some(self.input_data.clone()),
            unsigned_tx: None,
            gas_limit: self.gas_limit,
        })?;
        crate::uint::uint256(&self.value)?;
        if crate::signing::is_solana(&self.binance_chain_id) {
            return Err("typed tasks support EVM calls only".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Book {
    active: Option<Task>,
    retired: BTreeMap<String, Result<Value, String>>,
}
struct Task {
    id: String,
    call: Call,
    deadline: u64,
    monotonic_deadline: Instant,
    binding: Binding,
    policy: crate::policy::Policy,
    native_guard: bool,
}
#[derive(Clone, PartialEq)]
pub(super) struct Binding {
    session: Stamp,
    identity: Option<Stamp>,
}
fn now_ms() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "invalid clock")?
        .as_millis() as u64)
}
impl Book {
    pub(super) fn reserved(&mut self) -> bool {
        if self.active.as_ref().is_some_and(|t| t.expired()) {
            let task = self.active.take().unwrap();
            self.retired
                .insert(task.id, Err("EXPIRED; task not executed".into()));
        }
        self.active.is_some()
    }
}
impl Task {
    fn expired(&self) -> bool {
        Instant::now() >= self.monotonic_deadline || now_ms().map_or(true, |n| n >= self.deadline)
    }
}
impl Wallet {
    pub(super) async fn trigger_task(
        &self,
        id: &str,
        confirm: bool,
        received: Instant,
    ) -> Result<Value, String> {
        crate::execution::require_consent(confirm, Some(id))?;
        let started = Instant::now();
        let mut book = self
            .tasks
            .try_lock()
            .map_err(|_| "BUSY; request not executed")?;
        book.reserved();
        if let Some(result) = book.retired.get(id) {
            return result.clone();
        }
        if !book.active.as_ref().is_some_and(|t| t.id == id) {
            return Err("unknown operation-id; no fallback".into());
        }
        let task = book.active.take().ok_or("task missing")?;
        // Retain a tombstone before any fallible or asynchronous work. No outcome retries.
        book.retired.insert(
            id.into(),
            Err("UNKNOWN; accepted operation must never be resubmitted".into()),
        );
        let result = async {
            if self.binding()? != task.binding {
                return Err("identity/session changed; task invalidated".into());
            }
            if task.expired() {
                return Err("EXPIRED; task not executed".into());
            }
            let mut state = self
                .state
                .try_lock()
                .map_err(|_| "BUSY; request not executed")?;
            let mut client = state.client.take().ok_or("prepared client invalidated")?;
            client.begin_trace(received);
            let input = json!({"call":task.call,"deadline_ms":task.deadline});
            let mut guard = crate::execution::OperationGuard::begin(
                &self.entry.dir,
                id,
                "resident.contract-fire",
                &input,
                &client.credentials().client_id,
            )?;
            let args = crate::signing::ContractFire {
                binance_chain_id: task.call.binance_chain_id.clone(),
                from: task.call.from.clone(),
                to: task.call.to.clone(),
                value: Some(task.call.value.clone()),
                input_data: Some(task.call.input_data.clone()),
                gas_limit: task.call.gas_limit,
                // The resident path already gates on its own task policy before this point.
                policy: None,
            };
            let mut timing = crate::signing::FireTimings::default();
            let mut check = || {
                let validate = || {
                    if task.native_guard
                        && self
                            .native_invalid
                            .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err("native source unsafe; no execute submitted".to_owned());
                    }
                    if task.expired() {
                        return Err("EXPIRED; no execute submitted".into());
                    }
                    if self.binding()? != task.binding {
                        return Err("identity/session changed; no execute submitted".into());
                    }
                    Ok(())
                };
                validate()?;
                if task.native_guard {
                    self.native_mark_submitting(id)?;
                    // Persist may take time. Enforce the same original deadline/source/binding
                    // again at the actual execute boundary, not just before disk I/O.
                    validate()?;
                    self.native_submitted
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            };
            let result = tokio::time::timeout(
                COMMAND_TIMEOUT,
                crate::signing::memory_fire_guarded(
                    &mut client,
                    &self.entry.dir,
                    &args,
                    false,
                    &mut timing,
                    &mut check,
                    Some(&task.policy),
                ),
            )
            .await;
            let completed = result.is_ok();
            let result =
                result.unwrap_or_else(|_| Err("UNKNOWN; command timed out; never retry".into()));
            let recovery = result
                .as_ref()
                .map(|v| v["execution"].clone())
                .map_err(Clone::clone);
            let finished = guard.finish(&recovery);
            let http_metrics = client.take_trace();
            if completed {
                state.client = Some(client);
            }
            finished?;
            let mut result = result?;
            result["policy"] = json!("matched resident task policy");
            result["timings"] = json!(timing);
            result["http_metrics"] = json!(http_metrics);
            result["total_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            Ok(result)
        }
        .await;
        book.retired.insert(id.into(), result.clone());
        result
    }
    fn binding(&self) -> Result<Binding, String> {
        self.entry.check()?;
        Ok(Binding {
            session: file_stamp(&self.entry.dir.join("session.json"))
                .ok_or("session missing or unsafe")?,
            identity: if self.inline_identity.is_some() {
                None
            } else {
                Some(file_stamp(&self.entry.instance_file).ok_or("identity missing or unsafe")?)
            },
        })
    }
    async fn typed_load(&self, state: &mut State, binding: &Binding) -> Result<(), String> {
        if state.client.is_some()
            && state.stamp == Some(binding.session)
            && state.identity_stamp == binding.identity
        {
            return Ok(());
        }
        state.client = None;
        let entry = self.entry.clone();
        let inline = self.inline_identity.clone();
        let (instance, credentials) = tokio::task::spawn_blocking(move || {
            entry.check()?;
            let instance = match inline {
                Some(s) => s.to_string(),
                None => session::resolve_instance(String::new(), Some(&entry.instance_file))?,
            };
            let credentials = session::load(&entry.dir, &instance)?;
            if credentials.session_id.is_empty() {
                return Err("wallet session missing".to_owned());
            }
            Ok::<_, String>((instance, credentials))
        })
        .await
        .map_err(|_| "wallet load worker failed")??;
        if self.binding()? != *binding {
            return Err("identity/session changed during load".into());
        }
        let mut client = self.build(credentials)?;
        client.payment_consumed = self.consumed.clone();
        state.client = Some(client);
        state.instance = Some(instance);
        state.stamp = Some(binding.session);
        state.identity_stamp = binding.identity;
        Ok(())
    }
    pub(super) async fn native_readiness(
        &self,
        chain: &str,
        from: &str,
    ) -> Result<Binding, String> {
        let mut book = self
            .tasks
            .try_lock()
            .map_err(|_| "BUSY; wallet task control busy")?;
        if book.reserved() {
            return Err("BUSY; typed task already reserved".into());
        }
        let binding = self.binding()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "BUSY; wallet command already in flight")?;
        self.typed_load(&mut state, &binding).await?;
        crate::signing::check_wallet_policy(
            state.client.as_mut().ok_or("client missing")?,
            chain,
            Some(from),
            false,
        )
        .await?;
        if self.binding()? != binding {
            return Err("session changed during native readiness".into());
        }
        Ok(binding)
    }
    pub(super) async fn native_fire(
        &self,
        id: String,
        deadline: (u64, Instant),
        call: Call,
        policy: Value,
        binding: Binding,
        received: Instant,
    ) -> Result<Value, String> {
        self.native_submitted
            .store(false, std::sync::atomic::Ordering::SeqCst);
        call.validate()?;
        let policy = crate::policy::Policy::parse(&policy.to_string())?;
        policy.check_intent(&crate::policy::Intent {
            chain_id: &call.binance_chain_id,
            to: &call.to,
            value: &call.value,
            input_data: Some(&call.input_data),
            gas_limit: call.gas_limit,
        })?;
        if self.binding()? != binding {
            return Err("native session/identity binding changed".into());
        }
        let (deadline, monotonic_deadline) = deadline;
        if deadline <= now_ms()? || Instant::now() >= monotonic_deadline {
            return Err("native deadline expired".into());
        }
        {
            let mut book = self
                .tasks
                .try_lock()
                .map_err(|_| "BUSY; typed task control busy")?;
            if book.reserved() || book.retired.contains_key(&id) || book.retired.len() >= 256 {
                return Err("BUSY or duplicate native operation".into());
            }
            book.active = Some(Task {
                id: id.clone(),
                call,
                deadline,
                monotonic_deadline,
                binding,
                policy,
                native_guard: true,
            });
        }
        self.trigger_task(&id, true, received).await
    }
    pub(super) async fn prepare_task(
        &self,
        id: String,
        deadline: u64,
        call: Call,
        policy: Option<Value>,
    ) -> Result<Value, String> {
        if self
            .native_reserved
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err("BUSY; wallet reserved by native event task".into());
        }
        crate::execution::require_consent(true, Some(&id))?;
        call.validate()?;
        let default_value = crate::uint::uint256(&call.value)?.to_str_radix(10);
        let policy=crate::policy::Policy::parse(&policy.unwrap_or_else(||json!({"version":1,"chainId":call.binance_chain_id,"to":call.to,"valueWei":default_value})).to_string())?;
        if policy.max_total_wei.is_some() {
            return Err("maxTotalWei is unsupported by resident preview fee evidence; refusing unenforceable safeguard".into());
        }
        policy.check_intent(&crate::policy::Intent {
            chain_id: &call.binance_chain_id,
            to: &call.to,
            value: &call.value,
            input_data: Some(&call.input_data),
            gas_limit: call.gas_limit,
        })?;
        let now = now_ms()?;
        if deadline <= now || deadline - now > 300_000 {
            return Err("deadline must be within the next 300000 ms".into());
        }
        let mut book = self
            .tasks
            .try_lock()
            .map_err(|_| "BUSY; request not executed")?;
        book.reserved();
        if book.retired.contains_key(&id) {
            return Err("operation-id is terminal; never resubmit".into());
        }
        let binding = self.binding()?;
        if let Some(task) = &book.active {
            if task.id == id
                && task.call == call
                && task.deadline == deadline
                && task.binding == binding
                && task.policy == policy
            {
                return Ok(json!({"status":"PREPARED","operation_id":id}));
            }
            return Err("BUSY or conflicting operation-id; request not executed".into());
        }
        if book.retired.len() >= 256 {
            return Err("task history full; restart resident after reconciliation".into());
        }
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "BUSY; request not executed")?;
        self.typed_load(&mut state, &binding).await?;
        crate::signing::check_wallet_policy(
            state.client.as_mut().ok_or("client missing")?,
            &call.binance_chain_id,
            Some(&call.from),
            false,
        )
        .await?;
        if self.binding()? != binding || now_ms()? >= deadline {
            return Err("identity/session changed or deadline expired during prepare".into());
        }
        book.active = Some(Task {
            id: id.clone(),
            call,
            deadline,
            monotonic_deadline: Instant::now()
                + Duration::from_millis(
                    deadline
                        .checked_sub(now_ms()?)
                        .filter(|ms| *ms > 0)
                        .ok_or("deadline expired during prepare")?,
                ),
            binding,
            policy,
            native_guard: false,
        });
        Ok(json!({"status":"PREPARED","operation_id":id}))
    }
    pub(super) async fn disarm_task(&self, id: &str) -> Result<Value, String> {
        let mut book = self
            .tasks
            .try_lock()
            .map_err(|_| "BUSY; request not executed")?;
        book.reserved();
        if book.active.as_ref().is_some_and(|t| t.id == id) {
            book.active = None;
            let result = Ok(json!({"status":"DISARMED","operation_id":id}));
            book.retired.insert(id.into(), result.clone());
            return result;
        }
        book.retired
            .get(id)
            .cloned()
            .unwrap_or_else(|| Err("unknown operation-id".into()))
    }
}
