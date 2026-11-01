//! Explicit-wallet command grammar. The API origin is fixed.
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
                  Wallet lifecycle: baw-rs wallets --help (never requires --wallet).\n\
                  Legacy single wallet: --wallet-dir plus --instance-id-file."
)]
pub struct Cli {
    /// Name of a registered wallet. Required for every wallet-scoped command unless the
    /// legacy --wallet-dir pair is given.
    #[arg(long, global = true, value_name = "NAME", value_parser = nonempty)]
    pub wallet: Option<String>,
    /// Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry.
    #[arg(long, global = true, value_name = "PATH")]
    pub registry: Option<PathBuf>,
    /// Legacy single-wallet directory (compatibility; with --instance-id-file).
    #[arg(long, env = "BINANCE_BAW_DIR", value_parser = nonempty_path)]
    pub wallet_dir: Option<PathBuf>,
    #[arg(long, env = "BINANCE_INSTANCE_ID", default_value = "")]
    pub instance_id: String,
    /// Read the instance identity from a private (0600) file instead of the command line.
    #[arg(long = "instance-id-file", value_name = "PATH")]
    pub instance_id_file: Option<PathBuf>,
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
    /// Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust
    /// `--from` as the sender. Removes the local guard against sending from a wallet other
    /// than the one named; the backend still enforces developer mode.
    #[arg(long = "skip-prechecks", global = true, env = "BAW_RS_SKIP_PRECHECKS")]
    pub skip_prechecks: bool,
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
    /// Native event task control for the wallet named by --wallet.
    Task {
        #[command(subcommand)]
        command: crate::wallets::TaskAction,
    },
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
    /// Check whether the installed CLI meets a required version.
    CliCheck(crate::selfcheck::CliCheckArgs),
    /// Check whether a skill has a newer version available.
    SkillCheck(crate::selfcheck::SkillCheckArgs),
    /// Wallet lifecycle and named resident daemon. Never requires --wallet.
    #[command(name = "wallets", alias = "vaults")]
    Wallets(crate::wallets::Wallets),
    /// Resident server: reuse one session and connection pool over a private socket.
    Serve,
}

impl Cli {
    /// Wallet directory for the legacy single-wallet path. Only callable after
    /// `dispatch::require_wallet_scope` accepted the invocation.
    pub fn wallet_dir_path(&self) -> Result<&Path, String> {
        self.wallet_dir.as_deref().ok_or_else(|| {
            "wallet-scoped command requires --wallet <NAME>; legacy callers may pass --wallet-dir with --instance-id or --instance-id-file".to_string()
        })
    }
}

/// Selector flags the resident daemon owns; the named path strips its own and rejects the rest.
const NAMED_STRIPPED: [&str; 2] = ["--wallet", "--registry"];
const NAMED_REJECTED: [&str; 4] = [
    "--wallet-dir",
    "--instance-id",
    "--instance-id-file",
    "--warm",
];

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
        if NAMED_REJECTED.contains(&flag) {
            return Err(format!(
                "--wallet mode does not accept {flag}; it selects the wallet from the registry and the daemon owns the identity"
            ));
        }
        if NAMED_STRIPPED.contains(&flag) {
            // Keep `=value` inside the same token; otherwise drop the following value.
            skip_next = !arg.contains('=');
            continue;
        }
        if arg == "--" {
            // `--` is accepted as a no-op separator for symmetry with `wallets run NAME --`.
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
    Balance(crate::portfolio::BalanceArgs),
    TxHistory(crate::portfolio::HistoryArgs),
    LeftQuota,
    Send(crate::trade::SendArgs),
    /// Speed up a pending transaction by resubmitting with higher gas.
    SpeedUp(crate::wallet_ops::SpeedUpArgs),
    /// Cancel a pending transaction by sending a replacement tx with higher gas.
    #[command(name = "cancel")]
    Cancel(crate::wallet_ops::CancelTxArgs),
    /// Resolve a domain name (.eth/.bnb/.ai Agent ID) to an address.
    ResolveDomain(crate::wallet_ops::ResolveDomainArgs),
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
    fn named_mode_rejects_legacy_selectors_and_empty_argv() {
        for rejected in [
            "--wallet-dir",
            "--instance-id",
            "--instance-id-file",
            "--warm",
        ] {
            let error =
                forward(&["--wallet", "alice", rejected, "x", "wallet", "status"]).unwrap_err();
            assert!(error.contains(rejected), "{error}");
        }
        assert!(forward(&["--wallet", "alice"])
            .unwrap_err()
            .contains("missing command"));
        assert!(forward(&["--wallet=alice", "--"]).is_err());
    }

    #[test]
    fn wallet_flag_and_legacy_pair_parse_exactly_once() {
        let named =
            Cli::try_parse_from(["baw-rs", "--wallet", "alice", "wallet", "status"]).unwrap();
        assert_eq!(named.wallet.as_deref(), Some("alice"));
        assert!(named.wallet_dir.is_none());
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

        let legacy = Cli::try_parse_from([
            "baw-rs",
            "--wallet-dir",
            "/synthetic",
            "--instance-id",
            "synthetic",
            "wallet",
            "status",
        ])
        .unwrap();
        assert!(legacy.wallet.is_none());
        assert_eq!(legacy.wallet_dir.as_deref(), Some(Path::new("/synthetic")));

        // Task control is wallet-scoped at the top level.
        let task = Cli::try_parse_from([
            "baw-rs",
            "--wallet",
            "alice",
            "task",
            "arm",
            "sell-1",
            "--confirm",
        ])
        .unwrap();
        assert!(matches!(task.command, Command::Task { .. }));
    }
}
