use baw_rs::cli::{emit_json, envelope, forward_argv, Cli, Command};
use baw_rs::{client::Client, session, wallets};
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

/// Original argv minus the selectors the server already owns, so the resident
/// process parses exactly the command the caller typed.
fn warm_argv(raw: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in raw {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--wallet-dir" || arg == "--instance-id" || arg == "--instance-id-file" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--wallet-dir=")
            || arg.starts_with("--instance-id=")
            || arg.starts_with("--instance-id-file=")
            || arg == "--warm"
        {
            continue;
        }
        out.push(arg.clone());
    }
    out
}

async fn warm_request(cli: &Cli, raw: &[String]) -> Option<Value> {
    let dir = cli.wallet_dir_path().ok()?;
    baw_rs::warm::send(dir, &warm_argv(raw)).await
}

/// Emit one sanitized JSON envelope and map it to an exit status.
fn emit(result: Result<Value, String>) -> io::Result<bool> {
    let success = result.is_ok();
    emit_json(&mut io::stdout().lock(), &envelope(result))?;
    Ok(success)
}

/// Named-wallet path: registry resolution, then one IPC request. There is no cold fallback,
/// no second wallet, and no legacy selector is consulted.
async fn named(cli: &Cli, raw: &[String], name: &str, registry: &Path) -> Result<Value, String> {
    match &cli.command {
        Command::Serve => {
            return Err(
                "`serve` is not a wallet-scoped command: use `baw-rs wallets serve` for the named registry, or drop --wallet to run the legacy single-wallet daemon"
                    .into(),
            )
        }
        Command::Task { command } => {
            return wallets::dispatch_task(registry, name, command.clone()).await
        }
        _ => {}
    }
    let argv = forward_argv(raw)?;
    let require_session = !matches!(cli.command, Command::Auth { .. });
    wallets::dispatch_named(registry, name, argv, require_session).await
}

async fn run() -> io::Result<bool> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let named_flag = raw
        .iter()
        .any(|a| a == "--wallet" || a.starts_with("--wallet="));
    // `openfour` is a chain-only read: it must work without a wallet directory, an
    // instance identity or a session, so it is parsed and run before the wallet CLI.
    if raw
        .first()
        .is_some_and(|s| baw_rs::openfour::NAMES.contains(&s.as_str()))
    {
        let parsed = match baw_rs::openfour::Args::try_parse_from(raw.iter()) {
            Ok(args) => args,
            Err(error) => {
                let _ = error.print();
                return Ok(false);
            }
        };
        let result = baw_rs::openfour::run(&parsed.command).await;
        return emit(result);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => {
            baw_rs::signing::set_skip_prechecks_default(cli.skip_prechecks);
            baw_rs::signing::apply_skip_prechecks(cli.skip_prechecks);
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
            let message = if named_flag
                && raw
                    .iter()
                    .any(|a| baw_rs::openfour::NAMES.contains(&a.as_str()))
            {
                "openfour commands are chain-only reads and do not take --wallet"
            } else if named_flag {
                "invalid command after --wallet <NAME>; pass a wallet-scoped subcommand such as `wallet status`, or use `baw-rs wallets --help` for management"
            } else if raw
                .iter()
                .any(|a| a == "--wallet-dir" || a.starts_with("--wallet-dir="))
            {
                "invalid or unsupported command; use --help"
            } else {
                "invalid or unsupported command; use --help — wallet-scoped commands need --wallet <NAME> (or the legacy --wallet-dir pair)"
            };
            return emit(Err(message.into()));
        }
    };
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
        let result = wallets::run(action.action.clone(), registry).await;
        return emit(result);
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

    // Legacy single-wallet path: the wallet dimension is still mandatory, so an invocation
    // with neither --wallet nor --wallet-dir fails here instead of guessing.
    if let Err(message) = baw_rs::dispatch::require_wallet_scope(&cli) {
        return emit(Err(message));
    }
    let wallet_dir = match cli.wallet_dir_path() {
        Ok(path) => path.to_path_buf(),
        Err(message) => return emit(Err(message)),
    };
    let mut cli = cli;
    // Resolve the identity before anything opens a wallet path, so a missing or ambiguous
    // selector never looks like a wallet error.
    match session::resolve_instance(cli.instance_id.clone(), cli.instance_id_file.as_deref()) {
        Ok(instance) => cli.instance_id = instance,
        Err(message) => {
            return emit(Err(message));
        }
    }
    if cli.warm {
        if let Some(response) = warm_request(&cli, &raw).await {
            let success = response["success"] == true;
            emit_json(&mut io::stdout().lock(), &response)?;
            if cli.timing {
                writeln!(
                    io::stderr(),
                    "command_ms={:.3}",
                    start.elapsed().as_secs_f64() * 1000.0
                )?;
            }
            return Ok(success);
        }
        // No live server: fall through to a normal cold call.
    }
    if let Err(message) = baw_rs::dispatch::preflight(&cli) {
        return emit(Err(message));
    }
    if matches!(cli.command, Command::Serve) {
        let result = baw_rs::warm::serve(&cli).await;
        return emit(result);
    }
    let loaded = match &cli.command {
        Command::Auth { command } => {
            baw_rs::auth::bootstrap(&wallet_dir, &cli.instance_id, command)
        }
        _ => session::load(&wallet_dir, &cli.instance_id),
    };
    let credentials = match loaded {
        Ok(credentials) => credentials,
        Err(message) => {
            return emit(Err(message));
        }
    };
    let mut client = match Client::new(credentials) {
        Ok(client) => client,
        Err(message) => {
            return emit(Err(message));
        }
    };
    let mut output = io::stdout().lock();
    let mut errors = io::stderr().lock();
    let result = baw_rs::dispatch::execute(&mut client, &cli).await;
    let success = result.is_ok();
    emit_json(&mut output, &envelope(result))?;
    if cli.timing {
        writeln!(
            errors,
            "command_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        )?;
    }
    Ok(success)
}
