use baw_rs::cli::{Cli, Command, WalletCommand};
use clap::Parser;
use std::process::Command as Process;

fn binary() -> Process {
    let mut command = Process::new(env!("CARGO_BIN_EXE_baw-rs"));
    command
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID");
    command
}

#[test]
fn executable_errors_are_single_sanitized_json_without_session_access() {
    for args in [
        vec!["wallet", "status"],
        vec![
            "--wallet-dir",
            "/synthetic-does-not-exist",
            "--instance-id",
            "synthetic-instance",
            "wallet",
            "status",
        ],
        vec![
            "--wallet-dir",
            "/synthetic",
            "--instance-id",
            "x",
            "wallet",
            "send",
            "secret-argument",
        ],
    ] {
        let output = binary().args(args).output().unwrap();
        assert!(!output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(text.lines().count(), 1);
        let row: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(row["success"], false);
        assert!(row["error"]["message"].is_string());
        assert!(!text.contains("secret-argument"));
    }
}

#[test]
fn explicit_wallet_selection_and_read_only_allowlist() {
    let args = [
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "synthetic-instance",
        "--json",
        "wallet",
        "status",
    ];
    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Command::Wallet {
            command: WalletCommand::Status
        }
    ));
    for command in ["send", "swap", "login", "execute", "arbitrary-endpoint"] {
        assert!(Cli::try_parse_from([
            "baw-rs",
            "--wallet-dir",
            "/synthetic",
            "--instance-id",
            "x",
            "wallet",
            command
        ])
        .is_err());
    }
    assert!(Cli::try_parse_from([
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "x",
        "wallet",
        "gas-price"
    ])
    .is_err());
}

/// The trimmed build must not keep the removed families reachable by accident.
#[test]
fn removed_command_families_are_not_parsed() {
    for family in [
        "prediction",
        "defi",
        "x402-payment",
        "tracker",
        "signal",
        "leaderboard",
        "limit-order",
        "fast",
        "batch",
    ] {
        assert!(
            Cli::try_parse_from([
                "baw-rs",
                "--wallet-dir",
                "/synthetic",
                "--instance-id",
                "x",
                family
            ])
            .is_err(),
            "removed family still parses: {family}"
        );
    }
}

/// The flattened CLI: the wallet is a global flag, management is wallet-free, and the
/// pre-flattening `vaults` namespace still parses.
#[test]
fn flattened_wallet_flag_and_management_namespace() {
    let out = binary().args(["wallet", "status"]).output().unwrap();
    assert!(!out.status.success());
    let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(row["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--wallet <NAME>"));
    for namespace in ["wallets", "vaults"] {
        let out = binary()
            .args([
                "--registry",
                "/synthetic-absent-registry",
                namespace,
                "list",
            ])
            .output()
            .unwrap();
        assert!(!out.status.success(), "{namespace} list should fail");
        let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let message = row["error"]["message"].as_str().unwrap();
        assert!(message.contains("does not exist"), "{namespace}: {message}");
        assert!(
            !message.contains("--wallet <NAME>"),
            "{namespace} management must not demand a wallet: {message}"
        );
    }
}
