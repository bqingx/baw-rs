use baw_rs::{
    cli::{Cli, Command},
    trade::{self, MarketCommand, Provider},
};
use clap::Parser;

fn okx_swap() -> Vec<&'static str> {
    vec![
        "baw-rs",
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
    assert!(baw_rs::exec::dispatch::preflight(&cli).is_err());
    args.extend(["--confirm", "--operation-id", "okx-intent-1"]);
    let cli = Cli::try_parse_from(args).unwrap();
    baw_rs::exec::dispatch::preflight(&cli).unwrap();
    let Command::MarketOrder { command } = cli.command else {
        panic!()
    };
    let intent = trade::mutation_payload(&command).unwrap();
    assert_eq!(intent["args"]["provider"], "okx");
    assert_eq!(intent["calldataPolicy"], "pinned_okx_opaque_v3");
    assert_eq!(intent["args"]["okxMinOutputUnits"], "100");
    assert_eq!(intent["args"]["fromTokenQty"], "1.25");
    assert_eq!(intent["args"]["fromTokenDecimals"], 6);
    assert_eq!(intent["args"]["autoApprove"], false);
    assert_eq!(intent["args"]["maxFromTaxPercent"], "0");
    assert_eq!(intent["args"]["maxToTaxPercent"], "0");
}

#[test]
fn optional_fixed_floor_is_not_defaulted_and_remains_distinct_in_durable_intent() {
    let mut intents = Vec::new();
    for floor in [None, Some("1"), Some("100"), Some("101")] {
        let mut args = okx_swap();
        let p = args
            .iter()
            .position(|v| *v == "--okx-min-output-units")
            .unwrap();
        args.drain(p..p + 2);
        if let Some(value) = floor {
            args.extend(["--okx-min-output-units", value]);
        }
        args.extend(["--confirm", "--operation-id", "optional-floor"]);
        let cli = Cli::try_parse_from(args).unwrap();
        baw_rs::exec::dispatch::preflight(&cli).unwrap();
        let Command::MarketOrder {
            command: MarketCommand::Swap(a),
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(a.okx_min_output_units.as_deref(), floor);
        let intent = trade::mutation_payload(&MarketCommand::Swap(a)).unwrap();
        assert_eq!(
            intent["args"]["okxMinOutputUnits"],
            serde_json::json!(floor)
        );
        intents.push(intent);
    }
    for i in 0..intents.len() {
        for j in 0..i {
            assert_ne!(intents[i], intents[j]);
        }
    }
    assert_eq!(
        intents[2]["args"]["okxMinOutputUnits"], "100",
        "legacy explicit intent is unchanged"
    );
}

#[test]
fn explicit_fixed_floor_must_remain_a_positive_integer() {
    for value in ["0", "", "-1", "+1", "1.0", "1e3", " 1"] {
        let mut args = okx_swap();
        let p = args
            .iter()
            .position(|v| *v == "--okx-min-output-units")
            .unwrap();
        args[p + 1] = value;
        args.extend(["--confirm", "--operation-id", "invalid-floor"]);
        if let Ok(cli) = Cli::try_parse_from(args) {
            assert!(
                baw_rs::exec::dispatch::preflight(&cli).is_err(),
                "accepted {value:?}"
            );
        }
    }
}

#[test]
fn tax_caps_are_generic_explicit_and_part_of_the_durable_intent() {
    let mut args = okx_swap();
    args.extend([
        "--max-from-tax-percent",
        "3",
        "--max-to-tax-percent",
        "2",
        "--confirm",
        "--operation-id",
        "tax-intent",
    ]);
    let cli = Cli::try_parse_from(args).unwrap();
    baw_rs::exec::dispatch::preflight(&cli).unwrap();
    let Command::MarketOrder { command } = cli.command else {
        panic!()
    };
    let intent = trade::mutation_payload(&command).unwrap();
    assert_eq!(intent["args"]["maxFromTaxPercent"], "3");
    assert_eq!(intent["args"]["maxToTaxPercent"], "2");
}
#[test]
fn malformed_tax_caps_fail_before_wallet_access_or_submission() {
    for value in ["-1", "100", "1e-2", "0.000000001", "unknown"] {
        let mut args = okx_swap();
        args.extend([
            "--max-from-tax-percent",
            value,
            "--confirm",
            "--operation-id",
            "bad-tax",
        ]);
        if let Ok(cli) = Cli::try_parse_from(args) {
            assert!(
                baw_rs::exec::dispatch::preflight(&cli).is_err(),
                "accepted {value}"
            );
        }
    }
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
    baw_rs::exec::dispatch::preflight(&cli).unwrap();
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
    a.quote.okx_config = None;
    a.quote.from_token_decimals = None;
    a.okx_min_output_units = None;
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
    assert!(baw_rs::exec::dispatch::preflight(&cli)
        .unwrap_err()
        .contains("skip-prechecks"));
}

#[test]
fn default_provider_is_okx_without_an_explicit_provider_flag() {
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
    assert_eq!(a.quote.provider, Provider::Okx);
    assert!(trade::mutation_payload(&MarketCommand::Swap(a)).is_ok());
}

#[test]
fn deleted_compatibility_commands_and_flag_are_parse_errors() {
    for args in [
        vec!["baw-rs", "wallets", "register", "w1"],
        vec!["baw-rs", "wallets", "run", "w1", "--", "wallet", "status"],
        vec!["baw-rs", "market-order", "list"],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
    let mut args = okx_swap();
    args.push("--okx-allow-opaque-calldata");
    assert!(Cli::try_parse_from(args).is_err());
}

#[test]
fn removed_and_unknown_providers_are_parse_errors_not_fallbacks() {
    for provider in ["binance", "unknown-provider"] {
        let mut args = okx_swap();
        let p = args.iter().position(|x| *x == "okx").unwrap();
        args[p] = provider;
        assert!(Cli::try_parse_from(args).is_err());
    }
}
