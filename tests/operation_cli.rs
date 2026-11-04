use baw_rs::cli::{Cli, Command, WalletCommand};
use clap::{CommandFactory, Parser};

#[test]
fn command_tree_has_no_conflicting_options() {
    Cli::command().debug_assert();
}

#[test]
fn operation_lookup_uses_global_operation_id_in_either_position() {
    for args in [
        vec![
            "baw-rs",
            "--wallet",
            "alice",
            "wallet",
            "operation",
            "--operation-id",
            "op-1",
        ],
        vec![
            "baw-rs",
            "--wallet",
            "alice",
            "--operation-id",
            "op-1",
            "wallet",
            "operation",
        ],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.operation_id.as_deref(), Some("op-1"));
        assert!(matches!(
            cli.command,
            Command::Wallet {
                command: WalletCommand::Operation(_)
            }
        ));
    }
}
