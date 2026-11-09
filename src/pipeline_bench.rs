//! Manual local bookkeeping benchmark. No IPC, RPC, credentials or signing.
use super::*;
use std::{hint::black_box, time::Instant};
#[test]
#[ignore = "manual release-only local bookkeeping benchmark; never a chain latency claim"]
fn local_pipeline_bookkeeping_benchmark() {
    if cfg!(debug_assertions) {
        panic!("run with cargo test --release");
    }
    for (steps, samples) in [(3usize, 1000usize), (256, 30)] {
        let definition = (0..steps)
            .map(|i| Step {
                task_id: format!("task{i}"),
                wallets: vec![format!("w{i}"), format!("w{}", i + 1)],
                completion: Completion::Confirmed,
            })
            .collect::<Vec<_>>();
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let started = Instant::now();
            let mut snapshot = Pipeline::new(definition.clone()).unwrap().snapshot();
            for i in 0..steps {
                let wallets = snapshot.steps[i].wallets.clone();
                let mut pipeline = Pipeline::from_live(snapshot.clone()).unwrap();
                for wallet in &wallets {
                    pipeline.claim(i, wallet).unwrap();
                }
                snapshot = pipeline.snapshot();
                for wallet in &wallets {
                    let mut pipeline = Pipeline::from_live(snapshot.clone()).unwrap();
                    pipeline.record(i, wallet, Outcome::Submitted).unwrap();
                    pipeline.record(i, wallet, Outcome::Confirmed).unwrap();
                    snapshot = pipeline.snapshot();
                }
            }
            black_box(snapshot);
            times.push(started.elapsed().as_nanos());
        }
        times.sort_unstable();
        println!("steps={steps} wallets_per_step=2 samples={samples} p50_us={:.3} p95_us={:.3} scope=full_pipeline_in_memory_bookkeeping_excludes_fsync_network_and_signing",times[samples/2] as f64/1000.0,times[samples*95/100] as f64/1000.0);
    }
}
