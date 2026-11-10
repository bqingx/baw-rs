//! Explicit-wallet command grammar. The API origin is fixed.
pub mod selfcheck;
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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

#[derive(Parser)]
#[command(
    name = "baw-rs",
    version,
    about = "Native wallet client with explicit consent and preview-bound signing",
    after_help = "Named wallets: baw-rs --wallet alice wallet status (needs `baw-rs wallets serve`).\n\
                  Wallet lifecycle: baw-rs wallets --help (never requires --wallet)."
)]
pub struct Cli {
    /// Name of a registered wallet. Required for every wallet-scoped command.
    #[arg(long, global = true, value_name = "NAME", value_parser = nonempty)]
    pub wallet: Option<String>,
    /// Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry.
    #[arg(long, global = true, value_name = "PATH")]
    pub registry: Option<PathBuf>,
    /// Wallet directory of the request being executed. Never a command-line argument: only
    /// the resident daemon sets it, right after parsing a forwarded argv for a named wallet.
    #[arg(skip)]
    pub wallet_dir: Option<PathBuf>,
    /// Instance identity of the request being executed. Never a command-line argument: only
    /// the resident daemon sets it (after resolving the wallet's identity file) so auth can
    /// journal under the same identity it loaded.
    #[arg(skip)]
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
    /// Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust
    /// `--from` as the sender. Removes the local guard against sending from a wallet other
    /// than the one named; the backend still enforces developer mode.
    #[arg(long = "skip-prechecks", global = true, env = "BAW_RS_SKIP_PRECHECKS")]
    pub skip_prechecks: bool,
    #[command(subcommand)]
    pub command: Command,
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
    /// Instance-owned multi-wallet tasks and sequential pipelines; no --wallet.
    #[command(alias = "task")]
    Pipeline {
        #[command(subcommand)]
        command: crate::daemon::wallets::PipelineAction,
    },
    Auth {
        #[command(subcommand)]
        command: crate::core::auth::AuthCommand,
    },
    MarketOrder {
        #[command(subcommand)]
        command: crate::trade::MarketCommand,
    },
    ContractCall {
        #[command(subcommand)]
        command: crate::exec::signing::ContractCommand,
    },
    SignMessage {
        #[command(subcommand)]
        command: crate::exec::signing::SignCommand,
    },
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Token approvals: list, detail, revoke.
    Approvals {
        #[command(subcommand)]
        command: crate::trade::approvals::ApprovalsCommand,
    },
    /// Check whether the installed CLI meets a required version.
    CliCheck(crate::cli::selfcheck::CliCheckArgs),
    /// Check whether a skill has a newer version available.
    SkillCheck(crate::cli::selfcheck::SkillCheckArgs),
    /// Wallet lifecycle and named resident daemon. Never requires --wallet.
    #[command(name = "wallets", alias = "vaults")]
    Wallets(crate::daemon::wallets::Wallets),
}

impl Cli {
    /// Wallet directory of the request in flight. Only the resident daemon sets it, so a
    /// failure here is an internal wiring error, not a caller mistake.
    pub fn wallet_dir_path(&self) -> Result<&Path, String> {
        self.wallet_dir
            .as_deref()
            .ok_or_else(|| "wallet directory unavailable outside the resident daemon".to_string())
    }
}

/// Selector flags the resident daemon owns; the named path strips its own before forwarding.
const NAMED_STRIPPED: [&str; 2] = ["--wallet", "--registry"];

/// Forwarded argv for `--wallet <NAME> ...`: the caller's command with the named-path
/// selectors removed, so the daemon parses exactly the business command. `--wallet` and
/// `--registry` never appear in the result.
pub fn forward_argv(raw: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(raw.len());
    let mut skip_next = false;
    for arg in raw {
        if skip_next {
            skip_next = false;
            continue;
        }
        let flag = arg.split('=').next().unwrap_or(arg.as_str());
        if NAMED_STRIPPED.contains(&flag) {
            // Keep `=value` inside the same token; otherwise drop the following value.
            skip_next = !arg.contains('=');
            continue;
        }
        if arg == "--" {
            // `--` is accepted as a no-op separator, so `--` never becomes a positional.
            continue;
        }
        out.push(arg.clone());
    }
    if out.is_empty() {
        return Err(
            "missing command after --wallet <NAME>; for example `baw-rs --wallet alice wallet status`"
                .into(),
        );
    }
    Ok(out)
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
    Balance(crate::query::portfolio::BalanceArgs),
    TxHistory(crate::query::portfolio::HistoryArgs),
    LeftQuota,
    Send(crate::trade::SendArgs),
    /// Speed up a pending transaction by resubmitting with higher gas.
    SpeedUp(crate::trade::wallet_ops::SpeedUpArgs),
    /// Cancel a pending transaction by sending a replacement tx with higher gas.
    #[command(name = "cancel")]
    Cancel(crate::trade::wallet_ops::CancelTxArgs),
    /// Resolve a domain name (.eth/.bnb/.ai Agent ID) to an address.
    ResolveDomain(crate::trade::wallet_ops::ResolveDomainArgs),
    /// Read-only view of the local intent journal: was this operation id submitted?
    Operation(crate::exec::operations::OperationArgs),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(args: &[&str]) -> Result<Vec<String>, String> {
        let raw: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        forward_argv(&raw)
    }

    #[test]
    fn named_flag_is_global_and_strips_itself_from_the_forwarded_argv() {
        assert_eq!(
            forward(&["--wallet", "alice", "wallet", "status"]).unwrap(),
            ["wallet", "status"]
        );
        // Both flag positions and both `=value` spellings.
        assert_eq!(
            forward(&[
                "--wallet=alice",
                "--registry",
                "/r",
                "wallet",
                "balance",
                "--json"
            ])
            .unwrap(),
            ["wallet", "balance", "--json"]
        );
        assert_eq!(
            forward(&[
                "--confirm",
                "--wallet",
                "alice",
                "--operation-id",
                "op-1",
                "auth",
                "signin"
            ])
            .unwrap(),
            ["--confirm", "--operation-id", "op-1", "auth", "signin"]
        );
        // `--` is accepted as a separator, then dropped.
        assert_eq!(
            forward(&["--wallet", "alice", "--", "wallet", "status"]).unwrap(),
            ["wallet", "status"]
        );
        for out in [
            forward(&["--wallet", "alice", "wallet", "status"]).unwrap(),
            forward(&["--wallet=alice", "wallet", "status"]).unwrap(),
        ] {
            assert!(!out.iter().any(|a| a.contains("--wallet")));
        }
    }

    #[test]
    fn named_mode_rejects_empty_argv() {
        assert!(forward(&["--wallet", "alice"])
            .unwrap_err()
            .contains("missing command"));
        assert!(forward(&["--wallet=alice", "--"]).is_err());
    }

    #[test]
    fn wallet_flag_parses_exactly_once_and_removed_selectors_are_unknown() {
        let named =
            Cli::try_parse_from(["baw-rs", "--wallet", "alice", "wallet", "status"]).unwrap();
        assert_eq!(named.wallet.as_deref(), Some("alice"));
        assert!(named.wallet_dir.is_none());
        assert!(named.instance_id.is_empty());
        assert!(named.registry.is_none());
        assert!(matches!(named.command, Command::Wallet { .. }));

        let management =
            Cli::try_parse_from(["baw-rs", "--registry", "/r", "wallets", "list"]).unwrap();
        assert!(management.wallet.is_none());
        assert!(management.wallet_dir.is_none());
        assert_eq!(management.registry.as_deref(), Some(Path::new("/r")));
        assert!(matches!(management.command, Command::Wallets(_)));

        // `vaults` remains a hidden alias of `wallets`.
        let alias = Cli::try_parse_from(["baw-rs", "vaults", "list"]).unwrap();
        assert!(matches!(alias.command, Command::Wallets(_)));

        // The retired single-wallet entry points are no longer part of the grammar.
        for retired in [
            vec!["baw-rs", "--wallet-dir", "/synthetic", "wallet", "status"],
            vec!["baw-rs", "--instance-id", "synthetic", "wallet", "status"],
            vec![
                "baw-rs",
                "--instance-id-file",
                "/synthetic.key",
                "wallet",
                "status",
            ],
            vec!["baw-rs", concat!("--wa", "rm"), "wallet", "status"],
            vec!["baw-rs", "serve"],
        ] {
            assert!(
                Cli::try_parse_from(retired.clone()).is_err(),
                "retired entry point still parses: {retired:?}"
            );
        }

        // `task` is an alias of the instance scheduler, never wallet-scoped.
        let task = Cli::try_parse_from(["baw-rs", "task", "arm", "sell-1", "--confirm"]).unwrap();
        assert!(matches!(task.command, Command::Pipeline { .. }));
        assert!(task.wallet.is_none());
    }
}
