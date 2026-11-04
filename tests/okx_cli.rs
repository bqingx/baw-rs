use baw_rs::{
    cli::{Cli, Command},
    trade::{self, MarketCommand, Provider},
};
use clap::Parser;

fn okx_swap() -> Vec<&'static str> {
    vec![
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "synthetic",
        "market-order",
        "swap",
        "--provider",
        "okx",
        "--okx-config",
        "/synthetic/okx/profile.json",
        "--binanceChainId",
        "56",
        "--fromToken",
        "0x1111111111111111111111111111111111111111",
        "--toToken",
        "0x2222222222222222222222222222222222222222",
        "--fromTokenQty",
        "1.25",
        "--fromTokenDecimals",
        "6",
        "--slippage",
        "0.5",
        "--mev",
        "false",
        "--acknowledge-token-risk",
        "--okx-min-output-units",
        "100",
    ]
}

#[test]
fn okx_swap_requires_consent_and_binds_provider_and_floor_in_journal_intent() {
    let mut args = okx_swap();
    let cli = Cli::try_parse_from(&args).unwrap();
    assert!(baw_rs::dispatch::preflight(&cli).is_err());
    args.extend(["--confirm", "--operation-id", "okx-intent-1"]);
    let cli = Cli::try_parse_from(args).unwrap();
    baw_rs::dispatch::preflight(&cli).unwrap();
    let Command::MarketOrder { command } = cli.command else {
        panic!()
    };
    let intent = trade::mutation_payload(&command).unwrap();
    assert_eq!(intent["args"]["provider"], "okx");
    assert_eq!(intent["args"]["okxMinOutputUnits"], "100");
    assert_eq!(intent["args"]["fromTokenQty"], "1.25");
    assert_eq!(intent["args"]["fromTokenDecimals"], 6);
    assert_eq!(intent["args"]["autoApprove"], false);
}

#[test]
fn auto_approval_is_explicit_and_is_part_of_the_durable_intent() {
    let mut args = okx_swap();
    args.extend([
        "--auto-approve",
        "--confirm",
        "--operation-id",
        "okx-auto-1",
    ]);
    let cli = Cli::try_parse_from(args).unwrap();
    baw_rs::dispatch::preflight(&cli).unwrap();
    let Command::MarketOrder {
        command: MarketCommand::Swap(mut a),
    } = cli.command
    else {
        panic!()
    };
    assert_eq!(
        trade::mutation_payload(&MarketCommand::Swap(a.clone())).unwrap()["args"]["autoApprove"],
        true
    );
    a.quote.provider = Provider::Binance;
    a.quote.okx_config = None;
    a.quote.from_token_decimals = None;
    a.okx_min_output_units = None;
    a.okx_allow_opaque_calldata = false;
    assert!(trade::mutation_payload(&MarketCommand::Swap(a)).is_err());
}

#[test]
fn okx_does_not_accept_skip_prechecks() {
    let mut args = okx_swap();
    args.extend([
        "--confirm",
        "--operation-id",
        "okx-skip-1",
        "--skip-prechecks",
    ]);
    let cli = Cli::try_parse_from(args).unwrap();
    assert!(baw_rs::dispatch::preflight(&cli)
        .unwrap_err()
        .contains("skip-prechecks"));
}

#[test]
fn default_provider_remains_binance_and_never_infers_okx() {
    let mut args = okx_swap();
    let p = args.iter().position(|x| *x == "--provider").unwrap();
    args.drain(p..p + 2);
    let cli = Cli::try_parse_from(args).unwrap();
    let Command::MarketOrder {
        command: MarketCommand::Swap(a),
    } = cli.command
    else {
        panic!()
    };
    assert_eq!(a.quote.provider, Provider::Binance);
    assert!(trade::mutation_payload(&MarketCommand::Swap(a)).is_err());
}

#[test]
fn unknown_provider_is_a_parse_error_not_a_fallback() {
    let mut args = okx_swap();
    let p = args.iter().position(|x| *x == "okx").unwrap();
    args[p] = "unknown-provider";
    assert!(Cli::try_parse_from(args).is_err());
}
