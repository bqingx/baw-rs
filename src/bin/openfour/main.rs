//! `openfour`: Open Four / four.meme bonding-curve quote and offline buy encoding.
//!
//! A separate binary from `baw-rs`: it never opens a wallet, a registry, a session or the
//! resident daemon. Its output is a calldata template; any execution goes through
//! `baw-rs --wallet NAME contract-call ...`, which simulates it on the backend first.
use clap::Parser;
use std::io::{self, Write};
use std::process::ExitCode;

mod four;

#[tokio::main]
async fn main() -> ExitCode {
    let args = match four::Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    let result = four::run(&args.command).await;
    let success = result.is_ok();
    let mut out = io::stdout().lock();
    if baw_rs::cli::emit_json(&mut out, &baw_rs::cli::envelope(result)).is_err() {
        let _ = writeln!(io::stderr(), "I/O failed");
        return ExitCode::FAILURE;
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
