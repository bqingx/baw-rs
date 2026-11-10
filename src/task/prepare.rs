//! Allowances are prepared before a step starts listening, never on its trigger path.
use crate::cli::{Cli, Command};
use crate::daemon::resident::{wallet_for, Daemon, Wallet};
use crate::task::pipeline::Outcome;
use crate::task::runtime::{argv, now, save, set_phase, Definition, Run, Scheduler, Stored};
use crate::task::state::Phase;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub(crate) struct Plan {
    pub(super) wallet: Arc<Wallet>,
    pub(super) execute: Cli,
    pub(super) prepare: Option<Cli>,
    pub(super) binding: String,
}

pub(super) async fn compile_plans(
    daemon: &Daemon,
    config: &Definition,
    bindings: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Arc<Plan>>, String> {
    let mut plans = BTreeMap::new();
    for (index, task) in config.steps.iter().enumerate() {
        for action in &task.actions {
            let wallet = wallet_for(&daemon.wallets, &action.wallet, &daemon.registry).await?;
            let binding = bindings
                .get(&action.wallet)
                .cloned()
                .ok_or("wallet binding missing")?;
            if wallet.pipeline_binding()? != binding {
                return Err("wallet changed while compiling task plans".into());
            }
            let mut args = argv(config, index, action)?;
            let execute = wallet.compile(args.clone())?;
            let prepare = if matches!(
                &execute.command,
                Command::MarketOrder {
                    command: crate::trade::MarketCommand::Swap(_)
                }
            ) {
                args.last_mut()
                    .ok_or("task operation ID missing")?
                    .replace_range(..1, "a");
                Some(wallet.compile(args)?)
            } else {
                None
            };
            plans.insert(
                format!("{index}:{}", action.wallet),
                Arc::new(Plan {
                    wallet,
                    execute,
                    prepare,
                    binding,
                }),
            );
        }
    }
    Ok(plans)
}

/// New records carry the original sender, so confirmation/reconciliation needs no wallet API read.
/// Legacy already-submitted records may still use the existing read-only lookup.
pub(super) fn receipt_sender(
    state: &Stored,
    index: usize,
    name: &str,
    chain: &str,
) -> Result<Option<String>, String> {
    let proof = state
        .recovery
        .as_ref()
        .and_then(|r| r.prepared.get(&format!("{index}:{name}")));
    match proof.filter(|p| p.get("walletPolicy").is_some()) {
        Some(proof) => {
            let policy = crate::exec::execution::PreparedWalletPolicy::from_ready(proof)?;
            Ok(Some(policy.sender_on_chain(chain)?.to_owned()))
        }
        None => Ok(None),
    }
}

pub(super) async fn prepare_step(
    scheduler: &Scheduler,
    daemon: &Arc<Daemon>,
    run: &Arc<Run>,
    config: &Definition,
    index: usize,
    triggered: bool,
) -> Result<(), String> {
    let registry = daemon.registry.as_path();
    let task = &config.steps[index];
    let mut work = Vec::new();
    for (member, action) in task.actions.iter().enumerate() {
        let key = format!("{index}:{}", action.wallet);
        let plan = {
            let state = run.state.lock().await;
            if matches!(
                state.snapshot.outcomes[index][member],
                Outcome::Confirmed | Outcome::Submitted
            ) {
                continue;
            }
            let metadata = state.recovery.as_ref().ok_or("recovery state missing")?;
            let plan = metadata
                .plans
                .get(&key)
                .cloned()
                .ok_or("compiled task plan missing")?;
            if plan.prepare.is_none() {
                continue;
            }
            if let Some(proof) = metadata.prepared.get(&key) {
                if proof["calldataPolicy"] != crate::trade::route_action::CALLDATA_POLICY {
                    return Err("prepared calldata policy changed or missing before listening; no implicit intent upgrade".into());
                }
                let policy = crate::exec::execution::PreparedWalletPolicy::from_ready(proof)?;
                policy.sender_on_chain(&config.chain_id)?;
                if policy.developer_mode_expires_at <= chrono::Utc::now().timestamp().max(0) as u64
                {
                    return Err("prepared developer mode expired before listening; no implicit policy refresh".into());
                }
                continue;
            }
            if triggered || metadata.preparation_started.contains(&key) {
                return Err("allowance preparation missing or uncertain; no replay and no on-trigger approval permitted".into());
            }
            plan
        };
        if plan.wallet.pipeline_binding()? != plan.binding {
            return Err("wallet identity/session changed before allowance preparation".into());
        }
        work.push((key, plan));
    }
    if work.is_empty() {
        return Ok(());
    }
    let deadline = {
        let mut state = run.state.lock().await;
        let metadata = state.recovery.as_mut().ok_or("recovery state missing")?;
        let bound = now()
            .checked_add(120_000)
            .ok_or("preparation deadline overflow")?
            .min(metadata.deadlines.get(&index).copied().unwrap_or(u64::MAX));
        let deadline = *metadata.preparation_deadlines.entry(index).or_insert(bound);
        if run.cancel.load(Ordering::SeqCst) || now() >= deadline {
            return Err("allowance preparation cancelled or expired".into());
        }
        for (key, _) in &work {
            metadata.preparation_started.insert(key.clone());
        }
        set_phase(&mut state, index, Phase::Preparing);
        save(registry, &state)?;
        deadline
    };
    let mut jobs = tokio::task::JoinSet::new();
    for (key, plan) in work {
        let rpc = scheduler.rpc.clone();
        let run = run.clone();
        jobs.spawn(async move {
            let bound = plan.wallet.clone();
            let expected = plan.binding.clone();
            let check = Arc::new(move || {
                if bound.pipeline_binding()? != expected {
                    Err("wallet identity/session changed during preparation".into())
                } else {
                    Ok(())
                }
            });
            let result = crate::exec::execution::binding_scope(
                check,
                crate::exec::execution::rpc_scope(
                    rpc,
                    crate::exec::execution::task_scope(
                        deadline,
                        run.cancel.clone(),
                        crate::exec::execution::allowance_scope(
                            crate::exec::execution::TaskAllowance::Prepare,
                            plan.wallet.execute_compiled(
                                plan.prepare.as_ref().expect("compiled preparation plan"),
                            ),
                        ),
                    ),
                ),
            )
            .await;
            (key, result)
        });
    }
    // Drain admitted writes even when another member fails or the operator cancels.
    let mut failed = false;
    while let Some(joined) = jobs.join_next().await {
        let (key, result) = match joined {
            Ok(row) => row,
            Err(_) => {
                failed = true;
                continue;
            }
        };
        let mut state = run.state.lock().await;
        match result {
            Ok(proof) if proof["status"] == "ALLOWANCE_READY" => {
                if let Some(metadata) = state.recovery.as_mut() {
                    metadata.prepared.insert(key.clone(), proof.clone());
                } else {
                    failed = true;
                }
                state.results.insert(format!("prepare:{key}"), proof);
            }
            Ok(value) => {
                failed = true;
                state.results.insert(format!("prepare:{key}"), value);
            }
            Err(error) => {
                failed = true;
                state
                    .results
                    .insert(format!("prepare:{key}"), json!({"error":error}));
            }
        }
        // A persistence error must not drop the remaining JoinSet/admitted requests.
        if save(registry, &state).is_err() {
            failed = true;
        }
    }
    if failed {
        return Err(
            "allowance preparation failed or uncertain; listener not started, no automatic replay"
                .into(),
        );
    }
    if run.cancel.load(Ordering::SeqCst) || now() >= deadline {
        return Err("allowance preparation cancelled or expired; listener not started".into());
    }
    Ok(())
}
