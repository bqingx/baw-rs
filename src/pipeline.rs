//! Instance-level pipeline state machine. No network or signing side effects.
//!
//! The runtime must durably save a successful `claim` BEFORE submission. Restored
//! in-flight actions become uncertain and require reconciliation, never a retry.
//! Network effects and durable checkpointing are owned by the instance task runtime.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub task_id: String,
    pub wallets: Vec<String>,
    pub completion: Completion,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Completion {
    Submitted,
    Confirmed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pending,
    InFlight,
    Submitted,
    Confirmed,
    Failed,
    Uncertain,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub steps: Vec<Step>,
    pub outcomes: Vec<Vec<Outcome>>,
}

#[derive(Debug)]
pub struct Pipeline {
    state: Snapshot,
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Pipeline {
    pub fn new(steps: Vec<Step>) -> Result<Self, String> {
        if steps.is_empty() || steps.len() > 256 {
            return Err("pipeline requires 1..256 steps".into());
        }
        let mut ids = BTreeSet::new();
        for step in &steps {
            if !valid_name(&step.task_id) || !ids.insert(&step.task_id) {
                return Err("task IDs must be valid and unique within a pipeline".into());
            }
            let mut wallets = BTreeSet::new();
            if step.wallets.is_empty() || step.wallets.len() > 16 {
                return Err("task requires 1..16 wallets".into());
            }
            for wallet in &step.wallets {
                if !valid_name(wallet) || !wallets.insert(wallet) {
                    return Err("wallet names must be valid and unique within a task".into());
                }
            }
        }
        let outcomes = steps
            .iter()
            .map(|s| vec![Outcome::Pending; s.wallets.len()])
            .collect();
        Ok(Self {
            state: Snapshot {
                version: 1,
                steps,
                outcomes,
            },
        })
    }

    /// Validate persisted state before accepting it. A restart is not permission
    /// to repeat an action which might already have reached the remote service.
    pub fn restore(mut state: Snapshot) -> Result<Self, String> {
        if state.version != 1 {
            return Err("unsupported pipeline state version".into());
        }
        Self::new(state.steps.clone())?;
        Self::validate_progress(&state)?;
        for result in state.outcomes.iter_mut().flatten() {
            if *result == Outcome::InFlight {
                *result = Outcome::Uncertain;
            }
        }
        Ok(Self { state })
    }

    /// Live snapshots have immutable topology validated by new/restore. Recheck
    /// mutable progress, not every name/set or a second cloned recovery snapshot.
    pub(crate) fn from_live(state: Snapshot) -> Result<Self, String> {
        Self::validate_progress(&state)?;
        Ok(Self { state })
    }

    fn validate_progress(state: &Snapshot) -> Result<(), String> {
        if state.version != 1
            || state.steps.is_empty()
            || state.outcomes.len() != state.steps.len()
            || state
                .steps
                .iter()
                .zip(&state.outcomes)
                .any(|(s, row)| s.wallets.len() != row.len())
        {
            return Err("pipeline state shape/version mismatch".into());
        }
        let mut blocked = false;
        for (step, row) in state.steps.iter().zip(&state.outcomes) {
            if blocked && row.iter().any(|s| *s != Outcome::Pending) {
                return Err("persisted successor started before its predecessor completed".into());
            }
            blocked |= !Self::complete(step, row);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }

    fn complete(step: &Step, row: &[Outcome]) -> bool {
        row.iter().all(|s| {
            *s == Outcome::Confirmed
                || (step.completion == Completion::Submitted && *s == Outcome::Submitted)
        })
    }

    /// All members must satisfy the configured completion boundary.
    pub fn active_step(&self) -> Option<usize> {
        self.state
            .steps
            .iter()
            .zip(&self.state.outcomes)
            .position(|(s, r)| !Self::complete(s, r))
    }

    pub fn paused(&self) -> bool {
        self.state
            .outcomes
            .iter()
            .flatten()
            .any(|s| matches!(s, Outcome::Failed | Outcome::Uncertain))
    }

    /// Claim members independently, allowing wallets within a task to run in parallel.
    /// Persistence and cross-pipeline wallet coordination belong to the runtime.
    pub fn claim(&mut self, step: usize, wallet: &str) -> Result<(), String> {
        if self.paused() || self.active_step() != Some(step) {
            return Err("pipeline paused or task is not active".into());
        }
        let member = self.member(step, wallet)?;
        let result = &mut self.state.outcomes[step][member];
        if *result != Outcome::Pending {
            return Err("action already claimed; reconcile instead".into());
        }
        *result = Outcome::InFlight;
        Ok(())
    }

    fn member(&self, step: usize, wallet: &str) -> Result<usize, String> {
        self.state
            .steps
            .get(step)
            .and_then(|s| s.wallets.iter().position(|w| w == wallet))
            .ok_or_else(|| "wallet is not a member of this task".into())
    }

    /// Record evidence from the executor/reconciler, not user-supplied success flags.
    pub fn record(&mut self, step: usize, wallet: &str, next: Outcome) -> Result<(), String> {
        let member = self.member(step, wallet)?;
        let current = self.state.outcomes[step][member];
        let allowed = match current {
            Outcome::InFlight => matches!(
                next,
                Outcome::Submitted | Outcome::Confirmed | Outcome::Failed | Outcome::Uncertain
            ),
            Outcome::Submitted => matches!(
                next,
                Outcome::Confirmed | Outcome::Failed | Outcome::Uncertain
            ),
            Outcome::Uncertain => matches!(
                next,
                Outcome::Submitted | Outcome::Confirmed | Outcome::Failed
            ),
            _ => false,
        };
        if !allowed {
            return Err("invalid action transition".into());
        }
        self.state.outcomes[step][member] = next;
        Ok(())
    }
}

#[cfg(test)]
#[path = "pipeline_bench.rs"]
mod bench;
#[cfg(test)]
mod tests {
    use super::*;
    fn example() -> Vec<Step> {
        [
            ("task1", ["w1", "w2"]),
            ("task2", ["w2", "w3"]),
            ("task3", ["w3", "w4"]),
        ]
        .into_iter()
        .map(|(id, ws)| Step {
            task_id: id.into(),
            wallets: ws.into_iter().map(String::from).collect(),
            completion: Completion::Confirmed,
        })
        .collect()
    }
    #[test]
    fn overlapping_wallets_run_in_order_with_parallel_members() {
        let mut p = Pipeline::new(example()).unwrap();
        for i in 0..3 {
            assert_eq!(p.active_step(), Some(i));
            let wallets = p.snapshot().steps[i].wallets.clone();
            if i < 2 {
                assert!(p
                    .claim(i + 1, &p.snapshot().steps[i + 1].wallets[0])
                    .is_err());
            }
            for w in &wallets {
                p.claim(i, w).unwrap();
            }
            for w in &wallets {
                p.record(i, w, Outcome::Submitted).unwrap();
            }
            assert_eq!(p.active_step(), Some(i));
            p.record(i, &wallets[0], Outcome::Confirmed).unwrap();
            assert_eq!(p.active_step(), Some(i));
            p.record(i, &wallets[1], Outcome::Confirmed).unwrap();
        }
        assert_eq!(p.active_step(), None);
    }
    #[test]
    fn restart_never_reclaims_a_possibly_submitted_action() {
        let mut p = Pipeline::new(example()).unwrap();
        p.claim(0, "w1").unwrap();
        let encoded = serde_json::to_vec(&p.snapshot()).unwrap();
        let mut p = Pipeline::restore(serde_json::from_slice(&encoded).unwrap()).unwrap();
        assert!(p.paused());
        assert!(p.claim(0, "w1").is_err());
        assert!(p.claim(0, "w2").is_err());
        p.record(0, "w1", Outcome::Confirmed).unwrap();
        assert!(!p.paused());
        assert!(p.claim(0, "w1").is_err());
        p.claim(0, "w2").unwrap();
    }
    #[test]
    fn submission_boundary_is_explicit_and_failure_pauses_successors() {
        let mut steps = example();
        steps[0].completion = Completion::Submitted;
        let mut p = Pipeline::new(steps).unwrap();
        for w in ["w1", "w2"] {
            p.claim(0, w).unwrap();
            p.record(0, w, Outcome::Submitted).unwrap();
        }
        assert_eq!(p.active_step(), Some(1));
        p.record(0, "w1", Outcome::Failed).unwrap();
        assert!(p.paused());
        assert!(p.claim(1, "w2").is_err());
    }
    #[test]
    fn rejects_bad_definitions_snapshots_and_transitions() {
        let mut steps = example();
        steps[1].task_id = "task1".into();
        assert!(Pipeline::new(steps).is_err());
        let mut steps = example();
        steps[0].wallets[1] = "w1".into();
        assert!(Pipeline::new(steps).is_err());
        let mut p = Pipeline::new(example()).unwrap();
        assert!(p.claim(0, "w9").is_err());
        assert!(p.record(0, "w1", Outcome::Confirmed).is_err());
        let mut s = p.snapshot();
        s.outcomes[1][0] = Outcome::Confirmed;
        assert!(Pipeline::restore(s).is_err());
        let mut s = p.snapshot();
        s.version = 2;
        assert!(Pipeline::restore(s).is_err());
        let mut s = p.snapshot();
        s.outcomes[0].clear();
        assert!(Pipeline::restore(s).is_err());
    }
}
