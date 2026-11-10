use baw_rs::cli::{emit_json, envelope, forward_argv, Cli, Command};
use baw_rs::wallets;
use clap::{error::ErrorKind, Parser};
use serde_json::Value;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(_) => {
            // Never print the I/O error's payload, which can contain caller input.
            let _ = writeln!(io::stderr(), "I/O failed");
            ExitCode::FAILURE
        }
    }
}

/// Emit one sanitized JSON envelope and map it to an exit status.
fn emit(result: Result<Value, String>) -> io::Result<bool> {
    let success = result.is_ok();
    emit_json(&mut io::stdout().lock(), &envelope(result))?;
    Ok(success)
}

/// Named-wallet path: registry resolution, then one IPC request. There is no cold fallback
/// and no second wallet: the daemon owns the wallet directory and identity.
async fn named(cli: &Cli, raw: &[String], name: &str, registry: &Path) -> Result<Value, String> {
    let argv = forward_argv(raw)?;
    let require_session = !matches!(cli.command, Command::Auth { .. });
    wallets::dispatch_named(registry, name, argv, require_session).await
}

async fn run() -> io::Result<bool> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let named_flag = raw
        .iter()
        .any(|a| a == "--wallet" || a.starts_with("--wallet="));
    let cli = match Cli::try_parse() {
        Ok(cli) => {
            baw_rs::signing::set_skip_prechecks_default(cli.skip_prechecks);
            cli
        }
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error.print()?;
            return Ok(true);
        }
        Err(_) => {
            let instance_task = raw
                .iter()
                .any(|arg| matches!(arg.as_str(), "pipeline" | "task"));
            let message = if instance_task {
                "invalid instance task command; use `baw-rs pipeline --help` (status/arm/cancel/reconcile/resume require a pipeline ID; no --wallet)"
            } else if named_flag {
                "invalid command after --wallet <NAME>; pass a wallet-scoped subcommand such as `wallet status`, or use `baw-rs wallets --help` for management"
            } else {
                "invalid or unsupported command; use --help — wallet-scoped commands need --wallet <NAME>; see `baw-rs wallets list`"
            };
            return emit(Err(message.into()));
        }
    };
    // Intent is pure local inspection, even with wallet/warm selectors supplied.
    // Do not resolve registries, identity files, sessions, or private OKX profiles.
    if let Command::MarketOrder { command } = &cli.command {
        if let Some(summary) = baw_rs::trade::local_summary(command) {
            return emit(summary);
        }
    }
    let start = Instant::now();

    // Management namespace: wallet lifecycle and the daemon. It never needs a wallet and
    // rejects one, so a name is never silently ignored.
    if let Command::Wallets(action) = &cli.command {
        if let Some(name) = &cli.wallet {
            return emit(Err(format!(
                "`wallets` manages the whole registry and takes no --wallet (got '{name}'); the wallet-scoped form is `baw-rs --wallet {name} <command>`"
            )));
        }
        let registry = match wallets::registry_path(cli.registry.as_deref()) {
            Ok(path) => path,
            Err(message) => return emit(Err(message)),
        };
        let result =
            wallets::run(action.action.clone(), registry, cli.operation_id.as_deref()).await;
        return emit(result);
    }

    if let Command::Pipeline { command } = &cli.command {
        if cli.wallet.is_some() {
            return emit(Err(
                "pipeline is instance-scoped and rejects wallet selectors".into(),
            ));
        }
        let registry = match wallets::registry_path(cli.registry.as_deref()) {
            Ok(path) => path,
            Err(error) => return emit(Err(error)),
        };
        return emit(wallets::dispatch_pipeline(&registry, command.clone(), cli.confirm).await);
    }

    // Named wallet: resolve against the registry and forward the rest to the daemon.
    if let Some(name) = cli.wallet.clone() {
        let registry = match wallets::registry_path(cli.registry.as_deref()) {
            Ok(path) => path,
            Err(message) => return emit(Err(message)),
        };
        let result = named(&cli, &raw, &name, &registry).await;
        let success = result.is_ok();
        emit_json(&mut io::stdout().lock(), &envelope(result))?;
        if cli.timing {
            writeln!(
                io::stderr(),
                "command_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            )?;
        }
        return Ok(success);
    }

    // No --wallet: the wallet dimension is mandatory, so do not guess a wallet, a
    // directory, or an identity.
    emit(Err(
        "wallet-scoped commands need --wallet <NAME>; see `baw-rs wallets list`".into(),
    ))
}
