//! Opt-in single-wallet path. No quote, registry discovery or subprocess per order.
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "fast_tests.rs"]
mod tests;

#[derive(clap::Args, Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Buy {
    #[arg(long)]
    pub core: String,
    #[arg(long)]
    pub token: String,
    #[arg(long = "wallet")]
    pub from: String,
    #[arg(long = "amount-wei")]
    pub budget: String,
    #[arg(long = "min-out-raw")]
    #[serde(rename = "minAmountOut")]
    pub min_amount_out: String,
    #[arg(long = "allow-zero-min-out")]
    #[serde(default, rename = "allowZeroMinOut")]
    pub allow_zero_min_out: bool,
}
pub(crate) fn uint256(s: &str) -> Result<num_bigint::BigUint, String> {
    let (digits, radix, max) = match s.strip_prefix("0x") {
        Some(h) => (h, 16, 64),
        None => (s, 10, 78),
    };
    if digits.is_empty()
        || digits.len() > max
        || !digits.bytes().all(|b| {
            if radix == 16 {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err(
            "uint256 must be a raw decimal or 0x hex string, without signs or padding".into(),
        );
    }
    let n = num_bigint::BigUint::parse_bytes(digits.as_bytes(), radix).ok_or("invalid uint256")?;
    if n.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(n)
}
pub fn build(a: &Buy) -> Result<Value, String> {
    for address in [&a.core, &a.token, &a.from] {
        crate::signing::address("56", address)?;
        if address[2..].bytes().all(|b| b == b'0') {
            return Err("zero address is forbidden".into());
        }
    }
    let budget = uint256(&a.budget)?;
    let min = uint256(&a.min_amount_out)?;
    if min.bits() == 0 && !a.allow_zero_min_out {
        return Err("zero minAmountOut requires explicit allowZeroMinOut=true".into());
    }
    if budget.bits() == 0 {
        return Err("budget must be positive".into());
    }
    let input = crate::openfour::encode_budget(&a.token, &budget, &min)?;
    Ok(
        json!({"binanceChainId":"56","from":a.from,"to":a.core,"value":budget.to_str_radix(10),"inputData":input}),
    )
}
#[derive(Parser)]
#[command(name = "fast", about = "Explicit opt-in native-budget OpenFour path")]
pub struct Args {
    #[command(subcommand)]
    pub command: Action,
}
#[derive(Subcommand)]
pub enum Action {
    /// Offline only: one Buy JSON object on stdin, no session or RPC.
    Build,
    /// Persistent stdin JSONL, one pinned wallet; no per-order CLI subprocess.
    Serve {
        #[arg(long)]
        wallet_dir: std::path::PathBuf,
        #[arg(long)]
        instance_id_file: std::path::PathBuf,
    },
}
pub async fn run(args: Args) -> Result<Option<Value>, String> {
    match args.command {
        Action::Build => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| "stdin read failed")?;
            if bytes.len() > 65536 {
                return Err("request too large".into());
            }
            let a = serde_json::from_slice(&bytes).map_err(|_| "invalid Buy JSON")?;
            build(&a).map(Some)
        }
        Action::Serve {
            wallet_dir,
            instance_id_file,
        } => {
            serve(&wallet_dir, &instance_id_file).await?;
            Ok(None)
        }
    }
}

use crate::{
    client::Client,
    execution::{require_consent, OperationGuard},
};
use std::{path::Path, time::Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(rename = "operationId")]
    operation_id: String,
    confirm: bool,
    #[serde(default, rename = "skipPrechecks")]
    skip_prechecks: bool,
    buy: Buy,
}
#[derive(Default, Serialize)]
struct Timings {
    queue_ms: f64,
    state_check_ms: f64,
    encode_ms: f64,
    journal_ms: f64,
    total_ms: f64,
    #[serde(flatten)]
    fire: crate::signing::FireTimings,
}
#[cfg(test)]
async fn process(client: &mut Client, dir: &Path, line: &str, received: Instant) -> Value {
    process_checked(client, dir, line, received, &mut || Ok(())).await
}

async fn process_checked<F: FnMut() -> Result<(), String> + Send>(
    client: &mut Client,
    dir: &Path,
    line: &str,
    received: Instant,
    check: &mut F,
) -> Value {
    let mut timing = Timings {
        queue_ms: received.elapsed().as_secs_f64() * 1000.0,
        ..Default::default()
    };
    let result = async {
        let prep = Instant::now();
        let request: Request =
            serde_json::from_str(line).map_err(|_| "invalid fast request JSON")?;
        let id = require_consent(request.confirm, Some(&request.operation_id))?;
        timing.fire.local_prep_ms += prep.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let input = build(&request.buy);
        timing.encode_ms = started.elapsed().as_secs_f64() * 1000.0;
        let input = input?;
        if client.credentials().session_id.is_empty() {
            return Err("wallet session is missing".into());
        }
        let started = Instant::now();
        let guard = OperationGuard::begin(
            dir,
            id,
            "openfour.fast-fire",
            &input,
            &client.credentials().client_id,
        );
        timing.journal_ms += started.elapsed().as_secs_f64() * 1000.0;
        let mut guard = guard?;
        let args = crate::signing::ContractFire {
            binance_chain_id: "56".into(),
            from: request.buy.from,
            to: request.buy.core,
            value: input["value"].as_str().map(str::to_owned),
            input_data: input["inputData"].as_str().map(str::to_owned),
            gas_limit: None,
        };
        let mut preexecute_state_ms = 0.0;
        let mut recheck = || {
            let started = Instant::now();
            let result = check();
            preexecute_state_ms += started.elapsed().as_secs_f64() * 1000.0;
            result
        };
        let result = crate::signing::memory_fire_checked(
            client,
            dir,
            &args,
            request.skip_prechecks,
            &mut timing.fire,
            &mut recheck,
        )
        .await;
        timing.state_check_ms += preexecute_state_ms;
        let started = Instant::now();
        // Keep recovery identifiers in the durable journal, not arbitrary backend payloads.
        let recovery = result
            .as_ref()
            .map(|v| v["execution"].clone())
            .map_err(Clone::clone);
        let finished = guard.finish(&recovery);
        timing.journal_ms += started.elapsed().as_secs_f64() * 1000.0;
        finished?;
        result
    }
    .await;
    timing.total_ms = received.elapsed().as_secs_f64() * 1000.0;
    let mut output = crate::cli::envelope(result);
    if output["success"] == false {
        if let Some(preview) = timing.fire.preview_output.take() {
            output["preview"] = preview;
        }
    }
    output["timings"] = json!(timing);
    output
}

// Bounds both frame size and queued orders. Queue clock starts when a full line is read,
// before waiting for a queue slot. It excludes bytes still in the OS/stdin buffers.
async fn run_stream<R, W, F>(
    client: &mut Client,
    dir: &Path,
    reader: R,
    mut writer: W,
    mut check: F,
) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut() -> Result<(), String> + Send,
{
    let (send, mut receive) = tokio::sync::mpsc::channel::<(Instant, String)>(16);
    let read = async move {
        let mut reader = BufReader::new(reader);
        loop {
            let mut frame = Vec::new();
            loop {
                let available = reader.fill_buf().await.map_err(|_| "stdin read failed")?;
                if available.is_empty() {
                    if frame.is_empty() {
                        drop(send);
                        return Ok::<(), String>(());
                    }
                    return Err("incomplete JSONL frame; newline required".into());
                }
                let newline = available.iter().position(|b| *b == b'\n');
                let count = newline.map_or(available.len(), |n| n + 1);
                if frame.len() + count > 65536 {
                    return Err("JSONL frame exceeds 64 KiB".into());
                }
                frame.extend_from_slice(&available[..count]);
                reader.consume(count);
                if newline.is_some() {
                    break;
                }
            }
            let received = Instant::now();
            let line = String::from_utf8(frame).map_err(|_| "JSONL must be UTF-8")?;
            send.send((received, line))
                .await
                .map_err(|_| "request worker stopped")?;
        }
    };
    let work = async {
        while let Some((received, line)) = receive.recv().await {
            let dequeued = Instant::now();
            check()?;
            let state_check_ms = dequeued.elapsed().as_secs_f64() * 1000.0;
            let mut output = process_checked(client, dir, &line, received, &mut check).await;
            output["timings"]["queue_ms"] =
                json!(dequeued.duration_since(received).as_secs_f64() * 1000.0);
            output["timings"]["state_check_ms"] =
                json!(state_check_ms + output["timings"]["state_check_ms"].as_f64().unwrap_or(0.0));
            let mut bytes = serde_json::to_vec(&output).map_err(|_| "response encoding failed")?;
            bytes.push(b'\n');
            writer
                .write_all(&bytes)
                .await
                .map_err(|_| "stdout write failed; never retry uncertain order")?;
            writer
                .flush()
                .await
                .map_err(|_| "stdout flush failed; never retry uncertain order")?;
        }
        Ok::<(), String>(())
    };
    // A bad later frame must not cancel an execute already sent. Reader completion
    // drops its sender; drain all accepted orders before returning the framing error.
    // If the worker fails first (e.g. stdout broken), stop accepting further input.
    tokio::pin!(read, work);
    tokio::select! {
        biased;
        result = &mut read => {
            work.await?;
            result
        }
        result = &mut work => result,
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
fn stamp(path: &Path) -> Result<Stamp, String> {
    use std::os::unix::fs::MetadataExt;
    let file = crate::vaults::private_file(path, false)?;
    let m = file.metadata().map_err(|_| "cannot inspect pinned file")?;
    Ok(Stamp {
        dev: m.dev(),
        ino: m.ino(),
        size: m.size(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    })
}
async fn serve(dir: &Path, instance_file: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    crate::vaults::private_dir(dir)?;
    let m = std::fs::metadata(dir).map_err(|_| "cannot inspect wallet")?;
    let entry = crate::vaults::Entry {
        dir: dir.to_owned(),
        instance_file: instance_file.to_owned(),
        device: m.dev(),
        inode: m.ino(),
    };
    entry.check()?;
    let lease = crate::vaults::private_file(&dir.join(".baw-rs-resident.lock"), true)?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(|_| {
        "wallet already owned by resident daemon; fast runner is opt-in and never takes it over"
    })?;
    let session_file = dir.join("session.json");
    let before = stamp(&session_file)?;
    let identity_stamp = stamp(instance_file)?;
    let instance = crate::session::resolve_instance(String::new(), Some(instance_file))?;
    let credentials = crate::session::load(dir, &instance)?;
    if before != stamp(&session_file)? || identity_stamp != stamp(instance_file)? {
        return Err("session changed during load; restart runner".into());
    }
    let mut client = Client::new(credentials)?;
    run_stream(
        &mut client,
        dir,
        tokio::io::stdin(),
        tokio::io::stdout(),
        || {
            entry.check()?;
            if before != stamp(&session_file)? || identity_stamp != stamp(instance_file)? {
                return Err(
                    "session or identity changed; restart runner, do not replay orders".into(),
                );
            }
            Ok(())
        },
    )
    .await
}
