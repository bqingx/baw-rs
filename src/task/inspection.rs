//! Read-only, bounded and sanitized views; never arm, reconcile, save or grant deadlines.
use crate::cli::{Cli, Command};
use crate::task::pipeline::Outcome;
use crate::task::runtime::{
    argv, load, operation_id, safe_id, validate, Definition, Scheduler, Stored, Trigger,
};
use clap::Parser;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::Ordering;

pub(crate) struct Inspection {
    pub tasks: Vec<Value>,
    pub quiet: bool,
    pub complete: bool,
    pub resolved_operations: BTreeMap<String, std::collections::BTreeSet<String>>,
}
fn summary(state: &Stored, active: bool) -> Value {
    let mut counts = BTreeMap::new();
    for outcome in state.snapshot.outcomes.iter().flatten() {
        let label = match outcome {
            Outcome::Pending => "pending",
            Outcome::InFlight => "in_flight",
            Outcome::Submitted => "submitted",
            Outcome::Confirmed => "confirmed",
            Outcome::Failed => "failed",
            Outcome::Uncertain => "uncertain",
        };
        *counts.entry(label).or_insert(0usize) += 1;
    }
    let status = match state.status.as_str() {
        "LOADED" | "RUNNING" | "PAUSED" | "INTERRUPTED" | "COMPLETED" | "CANCELLED"
        | "RECONCILED" | "PERSISTENCE_ERROR" => state.status.as_str(),
        _ => "UNKNOWN",
    };
    let phase = state
        .results
        .get("phase")
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str)
        .filter(|s| {
            matches!(
                *s,
                "PREPARING" | "STARTING_LISTENER" | "WAITING" | "EXECUTING"
            )
        });
    let wallets = state
        .definition
        .steps
        .iter()
        .flat_map(|s| s.actions.iter().map(|a| a.wallet.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    let deadlines = state
        .recovery
        .as_ref()
        .map(|r| r.deadlines.clone())
        .unwrap_or_default();
    json!({"id":state.definition.id,"status":status,"active":active,"phase":phase,"wallets":wallets,"counts":counts,"deadlinesUnixMs":deadlines,"needsReview":matches!(status,"PAUSED"|"INTERRUPTED"|"PERSISTENCE_ERROR"|"UNKNOWN"),"retryAuthorized":false})
}
fn resolved(state: &Stored, output: &mut BTreeMap<String, std::collections::BTreeSet<String>>) {
    for (step, row) in state.snapshot.outcomes.iter().enumerate() {
        for (member, outcome) in row.iter().enumerate() {
            let wallet = &state.definition.steps[step].actions[member].wallet;
            let id = operation_id(&state.definition.id, step, wallet);
            if *outcome == Outcome::Confirmed {
                output.entry(wallet.clone()).or_default().insert(id.clone());
            }
            if state
                .recovery
                .as_ref()
                .and_then(|r| r.prepared.get(&format!("{step}:{wallet}")))
                .is_some_and(|v| v["autoApproval"]["confirmed"] == true)
            {
                let mut approval = id;
                approval.replace_range(..1, "a");
                output.entry(wallet.clone()).or_default().insert(approval);
            }
        }
    }
}
impl Scheduler {
    pub(crate) fn inspection_quiet(&self) -> bool {
        self.runs.try_lock().is_ok_and(|runs| {
            runs.values().all(|run| {
                !run.active.load(Ordering::SeqCst)
                    && run.state.try_lock().is_ok_and(|state| {
                        matches!(state.status.as_str(), "LOADED" | "COMPLETED" | "CANCELLED")
                    })
            })
        })
    }
    pub(crate) async fn inspect_local(&self, registry: &Path) -> Result<Inspection, String> {
        let mut output = Inspection {
            tasks: Vec::new(),
            quiet: true,
            complete: true,
            resolved_operations: BTreeMap::new(),
        };
        let mut known = std::collections::BTreeSet::new();
        match self.runs.try_lock() {
            Ok(runs) => {
                for (index, (id, run)) in runs.iter().enumerate() {
                    if index >= 256 {
                        output.complete = false;
                        output.quiet = false;
                        break;
                    }
                    known.insert(id.clone());
                    let active = run.active.load(Ordering::SeqCst);
                    match run.state.try_lock() {
                        Ok(state) => {
                            output.quiet &= !active
                                && matches!(
                                    state.status.as_str(),
                                    "LOADED" | "COMPLETED" | "CANCELLED"
                                );
                            resolved(&state, &mut output.resolved_operations);
                            output.tasks.push(summary(&state, active));
                        }
                        Err(_) => {
                            output.quiet = false;
                            output.complete = false;
                            output.tasks.push(json!({"id":id,"status":"BUSY","active":active,"retryAuthorized":false}));
                        }
                    }
                }
            }
            Err(_) => {
                output.quiet = false;
                output.complete = false;
            }
        }
        let registry = registry.to_owned();
        let (archives, complete) = tokio::task::spawn_blocking(move || {
            crate::daemon::wallets::private_dir(&registry)?;
            let mut files = Vec::new();
            let mut complete = true;
            for row in std::fs::read_dir(&registry).map_err(|_| "cannot inspect task directory")? {
                let row = row.map_err(|_| "cannot inspect task entry")?;
                let filename = row.file_name();
                let Some(name) = filename.to_str() else {
                    continue;
                };
                let Some(id) = name
                    .strip_prefix("pipeline-")
                    .and_then(|s| s.strip_suffix(".json"))
                else {
                    continue;
                };
                if !safe_id(id) || known.contains(id) {
                    continue;
                }
                if files.len() >= 256 {
                    complete = false;
                    break;
                }
                files.push(id.to_owned());
            }
            files.sort();
            let mut rows = Vec::new();
            for id in files {
                rows.push((id.clone(), load(&registry, &id)));
            }
            Ok::<_, String>((rows, complete))
        })
        .await
        .map_err(|_| "task inspection worker failed")??;
        output.complete &= complete;
        for (id, state) in archives {
            match state {
                Ok(state) => {
                    if !matches!(state.status.as_str(), "LOADED" | "COMPLETED" | "CANCELLED") {
                        output.quiet = false;
                    }
                    resolved(&state, &mut output.resolved_operations);
                    output.tasks.push(summary(&state, false));
                }
                Err(_) => {
                    output.quiet = false;
                    output.complete = false;
                    output
                        .tasks
                        .push(json!({"id":id,"status":"UNAVAILABLE","retryAuthorized":false}));
                }
            }
        }
        if !output.complete {
            output.quiet = false;
        }
        output
            .tasks
            .sort_by_key(|v| v["id"].as_str().unwrap_or_default().to_owned());
        Ok(output)
    }
}

pub(crate) fn describe_definition(config: Definition) -> Result<Value, String> {
    validate(&config)?;
    let mut steps = Vec::new();
    for (index, step) in config.steps.iter().enumerate() {
        let mut actions = Vec::new();
        for action in &step.actions {
            let args = argv(&config, index, action)?;
            let cli = Cli::try_parse_from(std::iter::once("baw-rs".to_owned()).chain(args))
                .map_err(|_| "invalid action")?;
            let intent = match cli.command {
                Command::MarketOrder {
                    command: crate::trade::MarketCommand::Swap(args),
                } => crate::trade::intent_summary(&args)?,
                Command::ContractCall {
                    command: crate::exec::signing::ContractCommand::Fire(args),
                } => {
                    crate::exec::signing::contract_payload(
                        &crate::exec::signing::ContractPreview {
                            binance_chain_id: args.binance_chain_id.clone(),
                            from: args.from.clone(),
                            to: Some(args.to.clone()),
                            value: args.value.clone(),
                            input_data: args.input_data.clone(),
                            unsigned_tx: None,
                            gas_limit: args.gas_limit,
                        },
                    )?;
                    crate::exec::signing::address(&config.chain_id, &args.from)?;
                    crate::exec::signing::address(&config.chain_id, &args.to)?;
                    let value = args
                        .value
                        .as_deref()
                        .map(crate::util::uint::uint256)
                        .transpose()?
                        .map(|n| n.to_string());
                    // Never echo calldata, policy paths or arbitrary signing payload fields.
                    json!({"operation":"contract-call.fire","chainId":args.binance_chain_id,"from":args.from,"to":args.to,"executionAuthorized":false,"requiresFreshPreview":true,"valueUnits":value,"gasLimit":args.gas_limit,"policyConfigured":args.policy.is_some(),"calldataIncluded":false})
                }
                _ => return Err("unsupported task intent".into()),
            };
            actions.push(json!({"wallet":action.wallet,"intent":intent}));
        }
        let trigger = match &step.trigger {
            Trigger::Immediate => json!({"kind":"immediate"}),
            Trigger::Time { unix_ms } => json!({"kind":"time","unixMs":unix_ms}),
            Trigger::Block { number } => json!({"kind":"block","number":number}),
            Trigger::Event {
                address,
                topics,
                from_block,
            } => json!({"kind":"event","address":address,"topics":topics,"fromBlock":from_block}),
        };
        steps.push(json!({"id":step.task_id,"trigger":trigger,"validForMs":step.valid_for_ms,"deadlineGranted":false,"preparationBudgetMaxMs":120000,"completion":step.completion,"confirmations":step.confirmations,"actions":actions}));
    }
    Ok(
        json!({"id":config.id,"chainId":config.chain_id,"steps":steps,"executionAuthorized":false,"loaded":false,"deadlineGranted":false,"networkRequests":0,"rpcConfigured":config.rpc_url.is_some(),"limitations":["description is not a quote, preview, approval or execution authorization","step deadlines are granted only by the original preparation/arm workflow, never this description"]}),
    )
}
