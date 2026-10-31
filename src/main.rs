use baw_rs::cli::{emit_json, envelope, run_batch, Cli, Command};
use baw_rs::{client::Client, session};
use clap::{error::ErrorKind, Parser};
use std::io::{self, Write};
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

async fn warm_request(cli: &Cli, raw: &[String]) -> Option<serde_json::Value> {
    baw_rs::warm::send(&cli.wallet_dir, &warm_argv(raw)).await
}

async fn run() -> io::Result<bool> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().is_some_and(|s| s == "vaults") {
        let args = match baw_rs::vaults::Args::try_parse_from(raw.iter()) {
            Ok(args) => args,
            Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
                e.print()?;
                return Ok(true);
            }
            Err(_) => {
                emit_json(
                    &mut io::stdout().lock(),
                    &envelope(Err("invalid vaults command; use vaults --help".into())),
                )?;
                return Ok(false);
            }
        };
        let result = baw_rs::vaults::run(args).await;
        let success = result.is_ok();
        emit_json(&mut io::stdout().lock(), &envelope(result))?;
        return Ok(success);
    }
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
        let success = result.is_ok();
        emit_json(&mut io::stdout().lock(), &envelope(result))?;
        return Ok(success);
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
            emit_json(
                &mut io::stdout().lock(),
                &envelope(Err("invalid or unsupported command; use --help".into())),
            )?;
            return Ok(false);
        }
    };
    let start = Instant::now();
    let mut cli = cli;
    // Resolve the identity before anything opens a wallet path, so a missing or ambiguous
    // selector never looks like a wallet error.
    match session::resolve_instance(cli.instance_id.clone(), cli.instance_id_file.as_deref()) {
        Ok(instance) => cli.instance_id = instance,
        Err(message) => {
            emit_json(&mut io::stdout().lock(), &envelope(Err(message)))?;
            return Ok(false);
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
        emit_json(&mut io::stdout().lock(), &envelope(Err(message)))?;
        return Ok(false);
    }
    if matches!(cli.command, Command::Serve) {
        let result = baw_rs::warm::serve(&cli).await;
        let success = result.is_ok();
        emit_json(&mut io::stdout().lock(), &envelope(result))?;
        return Ok(success);
    }
    let loaded = match &cli.command {
        Command::Auth { command } => {
            baw_rs::auth::bootstrap(&cli.wallet_dir, &cli.instance_id, command)
        }
        _ => session::load(&cli.wallet_dir, &cli.instance_id),
    };
    let credentials = match loaded {
        Ok(credentials) => credentials,
        Err(message) => {
            emit_json(&mut io::stdout().lock(), &envelope(Err(message)))?;
            return Ok(false);
        }
    };
    let mut client = match Client::new(credentials) {
        Ok(client) => client,
        Err(message) => {
            emit_json(&mut io::stdout().lock(), &envelope(Err(message)))?;
            return Ok(false);
        }
    };
    let mut output = io::stdout().lock();
    let mut errors = io::stderr().lock();
    match &cli.command {
        Command::Batch => {
            run_batch(
                &mut client,
                io::stdin().lock(),
                &mut output,
                &mut errors,
                cli.timing,
            )
            .await
        }
        _ => {
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
    }
}
