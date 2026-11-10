//! Read-only chain reconciliation and explicit, non-replaying continuation.
use crate::daemon::resident::{wallet_for, Daemon};
use crate::task::pipeline::{Outcome, Pipeline};
use crate::task::prepare as preparation;
use crate::task::runtime::{
    check_chain, hex_hash, now, operation_id, quantity, save, transaction_hash, Scheduler, Stored,
};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
mod tests;

pub(super) async fn receipt_status(
    rpc: &crate::chain::rpc::Lease,
    hash: &str,
    sender: &str,
    confirmations: u64,
) -> Result<Option<bool>, String> {
    if confirmations == 0 || !hex_hash(hash) {
        return Err("invalid receipt request".into());
    }
    let receipt = rpc.read("eth_getTransactionReceipt", json!([hash])).await?;
    if receipt.is_null() {
        return Ok(None);
    }
    if receipt["transactionHash"]
        .as_str()
        .is_none_or(|s| !s.eq_ignore_ascii_case(hash))
    {
        return Err("receipt transaction mismatch".into());
    }
    if receipt["from"]
        .as_str()
        .is_none_or(|s| !s.eq_ignore_ascii_case(sender))
    {
        return Err("receipt sender does not match the selected wallet".into());
    }
    let success = match receipt["status"].as_str() {
        Some("0x1") => true,
        Some("0x0") => false,
        _ => return Err("invalid receipt status".into()),
    };
    let block = quantity(&receipt["blockNumber"])?;
    let head = quantity(&rpc.read("eth_blockNumber", json!([])).await?)?;
    if head < block || head - block < confirmations - 1 {
        return Ok(None);
    }
    let canonical = rpc
        .read(
            "eth_getBlockByNumber",
            json!([receipt["blockNumber"], false]),
        )
        .await?;
    if !receipt["blockHash"].as_str().is_some_and(hex_hash)
        || canonical["hash"] != receipt["blockHash"]
        || quantity(&canonical["number"])? != block
    {
        return Err("receipt block is not canonical".into());
    }
    Ok(Some(success))
}

fn resumable(state: &Stored) -> Result<(), String> {
    if !matches!(
        state.status.as_str(),
        "PAUSED" | "INTERRUPTED" | "RECONCILED"
    ) {
        return Err("only paused/interrupted/reconciled pipelines may resume; cancelled tasks stay cancelled".into());
    }
    let recovery = state
        .recovery
        .as_ref()
        .ok_or("legacy state has no persisted deadlines/bindings; continuation refused")?;
    let pipeline = Pipeline::from_live(state.snapshot.clone())?;
    if pipeline.paused()
        || state
            .snapshot
            .outcomes
            .iter()
            .flatten()
            .any(|s| *s == Outcome::InFlight)
    {
        return Err("unresolved or reverted action; reconcile before resume, never replay".into());
    }
    if let Some(index) = pipeline.active_step() {
        for (member, action) in state.definition.steps[index].actions.iter().enumerate() {
            let key = format!("{index}:{}", action.wallet);
            if state.snapshot.outcomes[index][member] == Outcome::Pending
                && recovery.preparation_started.contains(&key)
                && !recovery.prepared.contains_key(&key)
            {
                return Err("allowance preparation missing or uncertain; continuation refused, no implicit reapproval".into());
            }
        }
        if (recovery.triggered.contains(&index)
            || state.snapshot.outcomes[index]
                .iter()
                .any(|s| *s != Outcome::Pending))
            && !recovery.deadlines.contains_key(&index)
        {
            return Err("started task has no persisted deadline; continuation refused".into());
        }
        if recovery
            .deadlines
            .get(&index)
            .is_some_and(|deadline| now() >= *deadline)
        {
            return Err("original task deadline expired; resume cannot extend it".into());
        }
        if state.snapshot.outcomes[index]
            .iter()
            .any(|s| *s != Outcome::Pending)
            && !recovery.triggered.contains(&index)
        {
            return Err("action evidence without persisted trigger; continuation refused".into());
        }
    }
    Ok(())
}
impl Scheduler {
    pub(super) async fn reconcile(&self, daemon: &Arc<Daemon>, id: &str) -> Result<Value, String> {
        let registry = daemon.registry.as_path();
        let run = self.get(registry, id).await?;
        let mut state = run.state.lock().await;
        if run.active.load(Ordering::SeqCst) || state.status == "RUNNING" {
            return Err("pipeline still draining; reconcile after it stops".into());
        }
        let mut pipeline = Pipeline::from_live(state.snapshot.clone())?;
        let Some(index) = pipeline.active_step() else {
            return Ok(json!({"id":id,"status":state.status,"complete":true,"writesSubmitted":0}));
        };
        let config = state.definition.clone();
        let rpc = self.rpc.acquire(
            config
                .rpc_url
                .as_deref()
                .ok_or("read-only reconciliation requires rpc_url")?,
        )?;
        check_chain(&rpc, &config.chain_id).await?;
        let mut observations = Vec::new();
        for (member, action) in config.steps[index].actions.iter().enumerate() {
            let outcome = state.snapshot.outcomes[index][member];
            if !matches!(outcome, Outcome::Submitted | Outcome::Uncertain) {
                continue;
            }
            let key = format!("{index}:{}", action.wallet);
            let wallet = wallet_for(&daemon.wallets, &action.wallet, &daemon.registry).await?;
            if let Some(recovery) = &state.recovery {
                let expected = recovery
                    .bindings
                    .get(&action.wallet)
                    .ok_or("wallet binding absent")?;
                if wallet.pipeline_binding()? != *expected {
                    return Err(
                        "wallet identity/session changed; reconciliation cannot bind evidence"
                            .into(),
                    );
                }
            }
            let evidence = state.results.get(&key).cloned().unwrap_or(Value::Null);
            let evidence = if transaction_hash(&evidence).is_some() {
                evidence
            } else {
                let record = crate::exec::operations::run(
                    &wallet.entry.dir,
                    &crate::exec::operations::OperationArgs {
                        id: Some(operation_id(id, index, &action.wallet)),
                        list: false,
                    },
                )
                .ok();
                record
                    .and_then(|v| v.get("recovery").cloned())
                    .filter(|v| transaction_hash(v).is_some())
                    .unwrap_or(evidence)
            };
            let Some(hash) = transaction_hash(&evidence) else {
                observations.push(json!({"wallet":action.wallet,"status":"UNRESOLVED","reason":"no transaction hash in execution result or operation journal; no retry authorized"}));
                continue;
            };
            let sender =
                match preparation::receipt_sender(&state, index, &action.wallet, &config.chain_id)?
                {
                    Some(sender) => sender,
                    None => wallet.chain_address(&config.chain_id).await?,
                };
            let result =
                receipt_status(&rpc, hash, &sender, config.steps[index].confirmations).await?;
            match result {
                Some(success) => {
                    pipeline.record(index,&action.wallet,if success {Outcome::Confirmed} else {Outcome::Failed})?;
                    state.snapshot = pipeline.snapshot();
                    state.results.insert(key,evidence);
                    // Checkpoint each verified result; never lose earlier verified members
                    // because a later node query fails.
                    save(registry,&state)?;
                    observations.push(json!({"wallet":action.wallet,"status":if success {"CONFIRMED"} else {"FAILED"}}));
                }
                None => observations.push(json!({"wallet":action.wallet,"status":"UNRESOLVED","reason":"receipt absent or confirmations insufficient"})),
            }
        }
        if state.status != "CANCELLED" {
            state.status = if pipeline.active_step().is_none() {
                "COMPLETED"
            } else if pipeline.paused() {
                "PAUSED"
            } else {
                "RECONCILED"
            }
            .into();
        }
        save(registry, &state)?;
        self.notify_state(registry, &state);
        Ok(
            json!({"id":id,"status":state.status,"observations":observations,"snapshot":state.snapshot,"writesSubmitted":0}),
        )
    }

    pub(super) async fn resume(
        self: &Arc<Self>,
        daemon: Arc<Daemon>,
        id: &str,
        confirm: bool,
    ) -> Result<Value, String> {
        if !confirm {
            return Err(
                "resume requires --confirm; it may execute only not-yet-claimed actions".into(),
            );
        }
        if self.stopping.load(Ordering::SeqCst) {
            return Err("instance is stopping".into());
        }
        let inspection = self.admit_execution()?;
        let registry = daemon.registry.as_path();
        let run = self.get(registry, id).await?;
        {
            let mut state = run.state.lock().await;
            if run.active.load(Ordering::SeqCst) {
                return Err("pipeline still running or draining".into());
            }
            resumable(&state)?;
            let recovery = state.recovery.as_ref().ok_or("recovery state missing")?;
            for (name, expected) in &recovery.bindings {
                let wallet = wallet_for(&daemon.wallets, name, &daemon.registry).await?;
                if wallet.pipeline_binding()? != *expected {
                    return Err("wallet identity/session changed; resume refused".into());
                }
                tokio::time::timeout(Duration::from_secs(30), wallet.preload())
                    .await
                    .map_err(|_| "wallet preparation timed out")??;
                if wallet.pipeline_binding()? != *expected {
                    return Err("wallet changed during resume preparation".into());
                }
            }
            let plans =
                preparation::compile_plans(&daemon, &state.definition, &recovery.bindings).await?;
            if let Some(url) = &state.definition.rpc_url {
                self.rpc.acquire(url)?;
            }
            resumable(&state)?;
            if self.stopping.load(Ordering::SeqCst) {
                return Err("instance stopped during resume preparation".into());
            }
            state
                .recovery
                .as_mut()
                .ok_or("recovery state missing")?
                .plans = plans;
            state.status = "RUNNING".into();
            save(registry, &state)?;
            run.cancel.store(false, Ordering::SeqCst);
            run.active.store(true, Ordering::SeqCst);
        }
        self.launch(daemon, run, inspection);
        Ok(json!({"id":id,"status":"RUNNING","replay":false,"deadlineRenewed":false}))
    }
}
