//! Explicit-wallet command grammar. Batch remains read-only; API origin is fixed.
use crate::client::Client;
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

pub fn envelope(result: Result<Value, String>) -> Value {
    match result {
        Ok(data) => json!({"success":true,"data":data}),
        Err(message) => json!({"success":false,"error":{"message":message}}),
    }
}

pub fn emit_json(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

pub async fn run_batch<R: BufRead, W: Write, E: Write>(
    client: &mut Client,
    mut input: R,
    output: &mut W,
    errors: &mut E,
    timing: bool,
) -> io::Result<bool> {
    let mut all_ok = true;
    while let Some(line) = read_bounded_line(&mut input)? {
        let start = Instant::now();
        let result = match line.and_then(|line| parse_batch_line(&line)) {
            Ok(command) => client.execute(&command).await,
            Err(error) => Err(error),
        };
        all_ok &= result.is_ok();
        emit_json(output, &envelope(result))?;
        if timing {
            writeln!(
                errors,
                "command_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            )?;
        }
    }
    Ok(all_ok)
}

// Drain oversized lines without ever allocating their full contents. One line -> one result.
fn read_bounded_line(input: &mut impl BufRead) -> io::Result<Option<Result<String, String>>> {
    let mut line = Vec::new();
    let mut oversized = false;
    let mut saw_bytes = false;
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            if !saw_bytes {
                return Ok(None);
            }
            break;
        }
        saw_bytes = true;
        let end = chunk.iter().position(|b| *b == b'\n');
        let count = end.unwrap_or(chunk.len());
        if !oversized {
            if line.len().saturating_add(count) > MAX_BATCH_LINE {
                oversized = true;
                line.clear();
            } else {
                line.extend_from_slice(&chunk[..count]);
            }
        }
        input.consume(count + usize::from(end.is_some()));
        if end.is_some() {
            break;
        }
    }
    if oversized {
        return Ok(Some(Err("batch line exceeds 65536 bytes".into())));
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(
        String::from_utf8(line).map_err(|_| "batch line must be UTF-8".into()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_drains_oversized_and_invalid_utf8_lines_then_recovers() {
        let mut bytes = vec![b'x'; MAX_BATCH_LINE * 4];
        bytes.extend_from_slice(b"\n\xff\n[\"wallet\",\"status\"]\r\n");
        let mut reader = std::io::BufReader::with_capacity(17, std::io::Cursor::new(bytes));
        assert!(read_bounded_line(&mut reader)
            .unwrap()
            .unwrap()
            .unwrap_err()
            .contains("exceeds"));
        assert!(read_bounded_line(&mut reader)
            .unwrap()
            .unwrap()
            .unwrap_err()
            .contains("UTF-8"));
        let next = read_bounded_line(&mut reader).unwrap().unwrap().unwrap();
        assert!(matches!(
            parse_batch_line(&next).unwrap(),
            WalletCommand::Status
        ));
        assert!(read_bounded_line(&mut reader).unwrap().is_none());
    }

    #[test]
    fn bounded_reader_processes_final_unterminated_line_and_empty_line() {
        let mut reader = std::io::Cursor::new(b"\n[\"wallet\",\"status\"]");
        assert!(
            parse_batch_line(&read_bounded_line(&mut reader).unwrap().unwrap().unwrap()).is_err()
        );
        assert!(matches!(
            parse_batch_line(&read_bounded_line(&mut reader).unwrap().unwrap().unwrap()).unwrap(),
            WalletCommand::Status
        ));
        assert!(read_bounded_line(&mut reader).unwrap().is_none());
    }
}

pub const MAX_BATCH_LINE: usize = 65_536;

#[derive(Parser)]
#[command(
    name = "baw-rs",
    version,
    about = "Native wallet client with explicit consent and preview-bound signing"
)]
pub struct Cli {
    #[arg(long, env = "BINANCE_BAW_DIR", value_parser = nonempty_path)]
    pub wallet_dir: PathBuf,
    #[arg(long, env = "BINANCE_INSTANCE_ID", value_parser = nonempty)]
    pub instance_id: String,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub timing: bool,
    #[arg(long, global = true)]
    pub confirm: bool,
    #[arg(long, global = true)]
    pub operation_id: Option<String>,
    #[arg(long, global = true)]
    pub accept_preview_sha256: Option<String>,
    /// Route this command through the resident warm server when it is running.
    #[arg(long, global = true)]
    pub warm: bool,
    #[command(subcommand)]
    pub command: Command,
}

fn nonempty_path(value: &str) -> Result<PathBuf, String> {
    nonempty(value).map(PathBuf::from)
}

fn nonempty(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("value must be nonempty".into())
    } else {
        Ok(value.into())
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Auth {
        #[command(subcommand)]
        command: crate::auth::AuthCommand,
    },
    MarketOrder {
        #[command(subcommand)]
        command: crate::trade::MarketCommand,
    },
    ContractCall {
        #[command(subcommand)]
        command: crate::signing::ContractCommand,
    },
    SignMessage {
        #[command(subcommand)]
        command: crate::signing::SignCommand,
    },
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Token approvals: list, detail, revoke.
    Approvals {
        #[command(subcommand)]
        command: crate::approvals::ApprovalsCommand,
    },
    /// Limit orders: buy, sell, list, cancel.
    LimitOrder {
        #[command(subcommand)]
        command: crate::limit_order::LimitOrderCommand,
    },
    /// Prediction markets: discovery, positions, trading.
    Prediction {
        #[command(subcommand)]
        command: crate::prediction::PredictionCommand,
    },
    /// DeFi: protocols, opportunities, positions, deposit/redeem/LP/claim.
    Defi {
        #[command(subcommand)]
        command: crate::defi::DefiCommand,
    },
    /// x402 payments: preview an HTTP 402 challenge, then sign an option.
    X402Payment {
        #[command(subcommand)]
        command: crate::x402::X402Command,
    },
    /// Read one JSON argv array per stdin line, retaining the in-memory session.
    Batch,
    /// Resident server: reuse one session and connection pool over a private socket.
    Serve,
}

#[derive(Args, Debug)]
pub struct Chain {
    #[arg(long = "binanceChainId", value_parser = nonempty)]
    pub binance_chain_id: String,
}

#[derive(Subcommand, Debug)]
pub enum WalletCommand {
    Status,
    Address,
    Chains,
    Settings,
    GasPrice(Chain),
    TxLock(Chain),
    AgentId,
    /// Normalized balances (including RWA), or lossless tokenList with --raw.
    Balance(crate::portfolio::BalanceArgs),
    TxHistory(crate::portfolio::HistoryArgs),
    LeftQuota,
    Send(crate::trade::SendArgs),
    /// Speed up a pending transaction.
    SpeedUp(crate::wallet_ops::SpeedUpArgs),
    /// Cancel a pending transaction.
    CancelTx(crate::wallet_ops::CancelTxArgs),
    /// Resolve a domain name (.eth/.bnb/.ai Agent ID) to an address.
    ResolveDomain(crate::wallet_ops::ResolveDomainArgs),
}

#[derive(Parser)]
#[command(
    name = "batch-command",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct BatchCommand {
    #[command(subcommand)]
    command: Command,
}

pub fn parse_batch_line(line: &str) -> Result<WalletCommand, String> {
    if line.len() > MAX_BATCH_LINE {
        return Err("batch line exceeds 65536 bytes".into());
    }
    let args: Vec<String> =
        serde_json::from_str(line).map_err(|_| "batch line must be a JSON array of strings")?;
    if args.iter().any(|arg| {
        matches!(
            arg.split('=').next().unwrap_or(""),
            "--wallet-dir"
                | "--instance-id"
                | "--json"
                | "--timing"
                | "--help"
                | "--version"
                | "--confirm"
                | "--operation-id"
                | "--accept-preview-sha256"
        )
    }) {
        return Err("global overrides are forbidden in batch".into());
    }
    let parsed =
        BatchCommand::try_parse_from(std::iter::once("batch-command".to_owned()).chain(args))
            .map_err(|_| "unsupported or malformed read-only batch command")?;
    match parsed.command {
        Command::Wallet {
            command: WalletCommand::Send(_),
        } => Err("mutations are forbidden in batch".into()),
        Command::Wallet { command } => Ok(command),
        Command::Batch => Err("nested batch is forbidden".into()),
        _ => Err("only read-only wallet commands are supported in batch".into()),
    }
}
