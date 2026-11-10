//! Explicit read-only same-client connection diagnostic. Never arms a task or signs a wallet action.
//! Arguments: /private/wallet/session-directory /private/identity-file /private/okx-profile idle-seconds
use baw_rs::{
    cli::Cli,
    core::{client::Client, session},
    exec::dispatch,
};
use clap::Parser;
use serde_json::json;
use std::{path::Path, time::Duration};

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: http_readonly_probe SESSION_DIR IDENTITY_FILE OKX_PROFILE IDLE_SECONDS".into(),
        );
    }
    let idle: u64 = args[3].parse().map_err(|_| "invalid idle seconds")?;
    if !(1..=120).contains(&idle) {
        return Err("idle must be 1..120 seconds".into());
    }
    let identity = zeroize::Zeroizing::new(session::resolve_instance(
        String::new(),
        Some(Path::new(&args[1])),
    )?);
    let credentials = session::load(Path::new(&args[0]), &identity)?;
    let mut client = Client::new(credentials)?;
    // No user-supplied command/endpoint: this is always a quote, never a swap or write.
    let quote = Cli::try_parse_from([
        "baw-rs",
        "--wallet-dir",
        &args[0],
        "--instance-id-file",
        &args[1],
        "market-order",
        "quote",
        "--okx-config",
        &args[2],
        "--binanceChainId",
        "56",
        "--fromToken",
        "0x55d398326f99059ff775485246999027b3197955",
        "--toToken",
        "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "--fromTokenQty",
        "1",
        "--fromTokenDecimals",
        "18",
        "--slippage",
        "20",
    ])
    .map_err(|_| "invalid fixed quote diagnostic arguments")?;
    for phase in [
        "first",
        "immediate_repeat",
        "after_idle",
        "immediate_after_idle",
    ] {
        if phase == "after_idle" {
            tokio::time::sleep(Duration::from_secs(idle)).await;
        }
        let binance = client.observe_readonly_settings().await?;
        let value = dispatch::execute(&mut client, &quote).await?;
        let okx = value
            .get("httpTransport")
            .ok_or("quote transport observation missing")?;
        println!(
            "{}",
            json!({"phase":phase,"idleSeconds":idle,"binanceSettings":binance,"okxQuote":okx,
            "walletWrites":0,"taskArmed":false,"scope":"readonly endpoints only; not Preview/Execute"})
        );
    }
    Ok(())
}
#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => {}
        Err(_) => {
            eprintln!("read-only probe failed; stopped without retry or wallet write");
            std::process::exit(1);
        }
    }
}
