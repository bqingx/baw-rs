//! Executable safety checks use no credentials and must never reach HTTP.
use std::process::Command;
#[test]
fn unauthorized_writes_fail_before_wallet_access() {
    let cases = [
        vec!["auth", "signin"],
        vec!["auth", "verify", "--qrCodeId", "synthetic"],
        vec!["auth", "signout"],
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
            "0.000000000000000001",
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
        vec!["contract-call", "execute", "--requestId", "synthetic"],
        vec!["sign-message", "execute", "--requestId", "synthetic"],
    ];
    for case in cases {
        for consent in [vec![], vec!["--confirm"], vec!["--operation-id", "only-id"]] {
            let out = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
                .env_remove("BINANCE_BAW_DIR")
                .env_remove("BINANCE_INSTANCE_ID")
                .args([
                    "--wallet-dir",
                    "/synthetic-missing-consent-test",
                    "--instance-id",
                    "synthetic",
                ])
                .args(&consent)
                .args(&case)
                .output()
                .unwrap();
            assert!(!out.status.success());
            let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert!(
                row["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("requires"),
                "{case:?}: {row}"
            );
            assert!(out.stderr.is_empty());
        }
    }
}
#[test]
fn blanket_batch_and_unreviewed_signing_fail_before_session_access() {
    for (args, expected) in [
        (
            vec!["--confirm", "--operation-id", "blanket", "batch"],
            "batch cannot authorize",
        ),
        (
            vec![
                "--confirm",
                "--operation-id",
                "unreviewed",
                "contract-call",
                "execute",
                "--requestId",
                "synthetic",
            ],
            "requires --accept-preview-sha256",
        ),
        (
            vec![
                "--confirm",
                "--operation-id",
                "unreviewed",
                "sign-message",
                "execute",
                "--requestId",
                "synthetic",
            ],
            "requires --accept-preview-sha256",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
            .env_remove("BINANCE_BAW_DIR")
            .env_remove("BINANCE_INSTANCE_ID")
            .args([
                "--wallet-dir",
                "/synthetic-inaccessible-wallet",
                "--instance-id",
                "synthetic",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let row: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            row["error"]["message"].as_str().unwrap().contains(expected),
            "{row}"
        );
    }
}
#[test]
fn batch_rejects_mutations_and_consent_overrides() {
    for line in [
        r#"["auth","signin"]"#,
        r#"["contract-call","execute","--requestId","x"]"#,
        r#"["wallet","status","--confirm","--operation-id","x"]"#,
    ] {
        assert!(baw_rs::cli::parse_batch_line(line).is_err());
    }
}
