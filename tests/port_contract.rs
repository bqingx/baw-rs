//! Migration acceptance checks: parsing only, no live wallet/network access.
use baw_rs::cli::Cli;
use clap::Parser;

#[test]
fn recognizes_previously_missing_command_families() {
    let prefix = [
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "synthetic",
    ];
    let cases: Vec<Vec<&str>> = vec![
        vec!["auth", "signin"],
        vec!["auth", "verify", "--qrCodeId", "synthetic-qr"],
        vec!["auth", "signout"],
        vec!["wallet", "balance"],
        vec!["wallet", "balance", "--raw"],
        vec!["wallet", "tx-history"],
        vec!["wallet", "left-quota"],
        vec![
            "market-order",
            "quote",
            "--binanceChainId",
            "56",
            "--fromToken",
            "0x0000000000000000000000000000000000000001",
            "--toToken",
            "0x0000000000000000000000000000000000000002",
            "--fromTokenQty",
            "1",
        ],
        vec![
            "market-order",
            "swap",
            "--binanceChainId",
            "56",
            "--fromToken",
            "0x0000000000000000000000000000000000000001",
            "--toToken",
            "0x0000000000000000000000000000000000000002",
            "--fromTokenQty",
            "1",
        ],
        vec!["market-order", "list"],
        vec![
            "wallet",
            "send",
            "--binanceChainId",
            "56",
            "--tokenAddress",
            "0x0000000000000000000000000000000000000001",
            "--recipient",
            "0x0000000000000000000000000000000000000002",
            "--amount",
            "1",
        ],
        vec![
            "contract-call",
            "preview",
            "--binanceChainId",
            "56",
            "--from",
            "0x0000000000000000000000000000000000000001",
            "--to",
            "0x0000000000000000000000000000000000000002",
        ],
        vec![
            "contract-call",
            "execute",
            "--requestId",
            "synthetic-preview",
        ],
        vec![
            "sign-message",
            "preview",
            "--binanceChainId",
            "56",
            "--signType",
            "EIP712",
            "--message",
            "{}",
        ],
        vec![
            "sign-message",
            "execute",
            "--requestId",
            "synthetic-preview",
        ],
        vec!["sign-message", "result", "--order-id", "synthetic-order"],
        vec!["sign-message", "history"],
    ];
    for case in cases {
        let args = prefix.into_iter().chain(case.iter().copied());
        assert!(
            Cli::try_parse_from(args).is_ok(),
            "missing command family {}",
            case.join(" ")
        );
    }
}
